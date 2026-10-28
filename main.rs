// SkyBrowser driver (pure std, no dependencies): a standalone background
// app with one job — poll XInput and move the REAL system cursor, so the
// controller works fullscreen in every site and app:
//        left stick   fast cursor      right stick  precise cursor
//        A            left click       B            right click
//        LT / RT      scroll up/down   LB / RB      back / forward
//        X            hold left button (drag)  Y     on-screen keyboard
//        D-pad up/down volume                 D-pad left/right arrows
//        Start+Y      power menu (Win+X)
//
// Tunables live in the SPEED / DEADZONE / TRIGGER_* constants below.
// Pure mapping logic has unit tests at the bottom:
//   rustc --edition=2021 --test main.rs -o skytest.exe && skytest.exe
use std::env;
use std::fs;
use std::mem::size_of;
use std::path::PathBuf;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------- constants
const APP_NAME: &str = "SkyBrowser";

/// Pixels per second at full deflection.
const FAST_SPEED: f32 = 1500.0;
const PRECISE_SPEED: f32 = 280.0;
/// Stick deflection below this is ignored.
const DEADZONE: f32 = 0.15;
/// Trigger byte thresholds (press above, release below: hysteresis).
const TRIGGER_PRESS: u8 = 100;
const TRIGGER_RELEASE: u8 = 60;
/// Wheel notch repeat while a trigger is held.
const WHEEL_REPEAT_MS: u64 = 100;
/// Mouse wheel delta per notch.
const WHEEL_DELTA: i32 = 120;

// XInput button masks.
const BTN_A: u16 = 0x1000;
const BTN_B: u16 = 0x2000;
const BTN_X: u16 = 0x4000;
const BTN_Y: u16 = 0x8000;
const BTN_LB: u16 = 0x0100;
const BTN_RB: u16 = 0x0200;
const BTN_DPAD_UP: u16 = 0x0001;
const BTN_DPAD_DOWN: u16 = 0x0002;
const BTN_DPAD_LEFT: u16 = 0x0004;
const BTN_DPAD_RIGHT: u16 = 0x0008;
const BTN_START: u16 = 0x0010;
const BTN_SELECT: u16 = 0x0020;

// Toggle feedback tones.
const BEEP_ON_FREQ: u32 = 880;
const BEEP_OFF_FREQ: u32 = 220;

// SendInput mouse flags.
const MOUSE_LEFT_DOWN: u32 = 0x0002;
const MOUSE_LEFT_UP: u32 = 0x0004;
const MOUSE_RIGHT_DOWN: u32 = 0x0008;
const MOUSE_RIGHT_UP: u32 = 0x0010;
const MOUSE_X_DOWN: u32 = 0x0080;
const MOUSE_X_UP: u32 = 0x0100;
const MOUSE_WHEEL: u32 = 0x0800;
const XBUTTON_BACK: u32 = 0x0001;
const XBUTTON_FORWARD: u32 = 0x0002;

// SendInput keyboard flags.
const KEYEVENTF_KEYUP: u32 = 0x0002;
const VK_LWIN: u16 = 0x5B;
const VK_CONTROL: u16 = 0x11;
const VK_O: u16 = 0x4F;
const VK_X: u16 = 0x58;
const VK_LEFT: u16 = 0x25;
const VK_RIGHT: u16 = 0x27;
const VK_VOLUME_DOWN: u16 = 0xAE;
const VK_VOLUME_UP: u16 = 0xAF;

// ---------------------------------------------------------------- FFI
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct XInputGamepad {
    buttons: u16,
    left_trigger: u8,
    right_trigger: u8,
    thumb_lx: i16,
    thumb_ly: i16,
    thumb_rx: i16,
    thumb_ry: i16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct XInputState {
    packet: u32,
    gamepad: XInputGamepad,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MouseInput {
    dx: i32,
    dy: i32,
    data: u32,
    flags: u32,
    time: u32,
    extra: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct KeyboardInput {
    vk: u16,
    scan: u16,
    flags: u32,
    time: u32,
    extra: usize,
}

#[repr(C)]
union InputUnion {
    mi: MouseInput,
    ki: KeyboardInput,
}

#[repr(C)]
struct Input {
    ty: u32,
    u: InputUnion,
}

// INPUT must be exactly 40 bytes on 64-bit Windows or SendInput misreads it.
const _: () = assert!(size_of::<Input>() == 40);

#[link(name = "kernel32")]
extern "C" {
    fn LoadLibraryW(name: *const u16) -> *mut std::ffi::c_void;
    fn GetProcAddress(
        module: *mut std::ffi::c_void,
        name: *const u8,
    ) -> *mut std::ffi::c_void;
    fn Beep(freq: u32, duration_ms: u32) -> i32;
    fn OpenProcess(desired_access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
    fn QueryFullProcessImageNameW(
        process: *mut std::ffi::c_void,
        flags: u32,
        name: *mut u16,
        size: *mut u32,
    ) -> i32;
    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
}

/// Signature of XInputGetState inside xinput1_*.dll.
type XInputGetStateFn = unsafe extern "C" fn(u32, *mut XInputState) -> u32;

fn wide_nul(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Resolve XInputGetState from the newest XInput DLL present, oldest last.
fn load_xinput() -> Option<XInputGetStateFn> {
    for dll in ["xinput1_4.dll", "xinput1_3.dll", "xinput9_1_0.dll"] {
        let name = wide_nul(dll);
        let handle = unsafe { LoadLibraryW(name.as_ptr()) };
        if handle.is_null() {
            continue;
        }
        let proc = unsafe { GetProcAddress(handle, b"XInputGetState\0".as_ptr()) };
        if !proc.is_null() {
            // Function pointer from a live DLL handle; safe to call.
            let f: XInputGetStateFn = unsafe { std::mem::transmute(proc) };
            return Some(f);
        }
    }
    None
}

#[link(name = "user32")]
extern "C" {
    fn SetCursorPos(x: i32, y: i32) -> i32;
    fn GetCursorPos(p: *mut Point) -> i32;
    fn SendInput(n: u32, p: *mut Input, size: i32) -> u32;
    fn GetSystemMetrics(n: i32) -> i32;
    fn GetForegroundWindow() -> *mut std::ffi::c_void;
    fn GetWindowThreadProcessId(hwnd: *mut std::ffi::c_void, pid: *mut u32) -> u32;
}

// ------------------------------------------------------- pure mapping logic
/// Raw i16 stick axis -> -1.0..1.0 with deadzone removed and rescaled.
fn stick(v: i16) -> f32 {
    let f = (v as f32 / 32768.0).clamp(-1.0, 1.0);
    let a = f.abs();
    if a < DEADZONE {
        0.0
    } else {
        f.signum() * (a - DEADZONE) / (1.0 - DEADZONE)
    }
}

/// Pixels to move this frame for a deadzoned deflection at max_speed.
fn step(deflection: f32, max_speed: f32, dt_secs: f32) -> f32 {
    deflection * max_speed * dt_secs
}

/// Rising-edge detector over a button bitmask.
#[derive(Default)]
struct Edges {
    prev: u16,
}

impl Edges {
    fn hit(&mut self, buttons: u16, mask: u16) -> bool {
        let now = buttons & mask != 0;
        let was = self.prev & mask != 0;
        if now {
            self.prev |= mask;
        } else {
            self.prev &= !mask;
        }
        now && !was
    }

    fn reset(&mut self) {
        self.prev = 0;
    }
}

/// Trigger press detector with hysteresis: fires once when pushed past
/// TRIGGER_PRESS, re-arms only after falling below TRIGGER_RELEASE.
#[derive(Default)]
struct Trigger {
    down: bool,
}

impl Trigger {
    /// True while the trigger is past the press point (for hold-to-repeat).
    fn held(&self) -> bool {
        self.down
    }

    /// Returns true on the frame the press engages.
    fn update(&mut self, value: u8) -> bool {
        if self.down {
            if value < TRIGGER_RELEASE {
                self.down = false;
            }
            false
        } else if value > TRIGGER_PRESS {
            self.down = true;
            true
        } else {
            false
        }
    }
}

/// Hold-to-drag state for X: engages the left button on rising edge while
/// live, always releases on falling edge so the button can never stick.
#[derive(Default)]
struct HoldButton {
    down: bool,
}

impl HoldButton {
    fn holding(&self) -> bool {
        self.down
    }

    /// Returns (press, release) events for this frame.
    fn update(&mut self, now: bool, live: bool) -> (bool, bool) {
        if self.down {
            if !now {
                self.down = false;
                (false, true)
            } else {
                (false, false)
            }
        } else if now && live {
            self.down = true;
            (true, false)
        } else {
            (false, false)
        }
    }

    /// Force-release (driver toggle-off, pad lost). True if a release is owed.
    fn cancel(&mut self) -> bool {
        if self.down {
            self.down = false;
            true
        } else {
            false
        }
    }
}

/// On/off switch driven by pressing Select+Start together.
/// Returns Some(new state) on the frame the combo engages.
#[derive(Default)]
struct Toggle {
    enabled: bool,
    prev_both: bool,
}

impl Toggle {
    fn on() -> Self {
        Self {
            enabled: true,
            prev_both: false,
        }
    }

    fn update(&mut self, start: bool, select: bool) -> Option<bool> {
        let both = start && select;
        let fire = both && !self.prev_both;
        self.prev_both = both;
        if fire {
            self.enabled = !self.enabled;
            Some(self.enabled)
        } else {
            None
        }
    }
}

/// Two-button combo edge detector (Start+Y power menu): fires once when
/// both buttons are down together, re-arms after either is released.
#[derive(Default)]
struct Combo {
    prev_both: bool,
}

impl Combo {
    fn update(&mut self, a: bool, b: bool) -> bool {
        let both = a && b;
        let fire = both && !self.prev_both;
        self.prev_both = both;
        fire
    }
}

/// Example allow-list written into the data dir on first launch.
/// Every line is a comment, so the default is "drive everywhere" and the
/// user restricts it by adding exe names.
const WHITELIST_TEMPLATE: &str = "# SkyBrowser allow-list: one program name per line.\r\n# While any non-comment line exists, the driver only injects input when\r\n# the foreground window belongs to a listed program. Delete all entries\r\n# (or this file) to drive everywhere again.\r\n#\r\n# Examples:\r\n# msedge.exe\r\n# chrome.exe\r\n# firefox.exe\r\n";

fn load_whitelist(dir: &PathBuf) -> Vec<String> {
    let text = fs::read_to_string(dir.join("whitelist.txt")).unwrap_or_default();
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_lowercase())
        .collect()
}

/// Pure allow-list decision: empty list allows everything; otherwise the
/// foreground image's file name must match (case-insensitive).
fn scope_allows(list: &[String], image: Option<&str>) -> bool {
    if list.is_empty() {
        return true;
    }
    let Some(path) = image else {
        return false;
    };
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    list.iter().any(|w| w == &name.to_lowercase())
}

fn foreground_image() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(0x1000, 0, pid); // PROCESS_QUERY_LIMITED_INFORMATION
        if handle.is_null() {
            return None;
        }
        let mut buf = [0u16; 260];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(handle);
        if ok == 0 {
            return None;
        }
        String::from_utf16(&buf[..size as usize]).ok()
    }
}

/// Caches the allow-list verdict per foreground PID so OpenProcess runs
/// only when the foreground window actually changes.
#[derive(Default)]
struct Scope {
    pid: u32,
    allowed: bool,
    ready: bool,
}

impl Scope {
    fn allows(&mut self, list: &[String]) -> bool {
        if list.is_empty() {
            return true;
        }
        let pid = unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return false;
            }
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            pid
        };
        if self.ready && pid == self.pid {
            return self.allowed;
        }
        self.pid = pid;
        self.ready = true;
        self.allowed = scope_allows(list, foreground_image().as_deref());
        self.allowed
    }
}

fn beep(on: bool) {
    unsafe {
        Beep(if on { BEEP_ON_FREQ } else { BEEP_OFF_FREQ }, 120);
    }
}

// ------------------------------------------------------------- input output
fn send_mouse(flags: u32, data: u32) {
    let mut input = Input {
        ty: 0, // INPUT_MOUSE
        u: InputUnion {
            mi: MouseInput {
                dx: 0,
                dy: 0,
                data,
                flags,
                time: 0,
                extra: 0,
            },
        },
    };
    unsafe {
        SendInput(1, &mut input, size_of::<Input>() as i32);
    }
}

fn left_click() {
    send_mouse(MOUSE_LEFT_DOWN, 0);
    send_mouse(MOUSE_LEFT_UP, 0);
}

fn right_click() {
    send_mouse(MOUSE_RIGHT_DOWN, 0);
    send_mouse(MOUSE_RIGHT_UP, 0);
}

/// Browser back (XBUTTON1) / forward (XBUTTON2); honored by every browser.
fn x_button(which: u32) {
    send_mouse(MOUSE_X_DOWN, which);
    send_mouse(MOUSE_X_UP, which);
}

fn wheel(notches: i32) {
    send_mouse(MOUSE_WHEEL, (notches * WHEEL_DELTA) as u32);
}

fn send_key(vk: u16, up: bool) {
    let mut input = Input {
        ty: 1, // INPUT_KEYBOARD
        u: InputUnion {
            ki: KeyboardInput {
                vk,
                scan: 0,
                flags: if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                extra: 0,
            },
        },
    };
    unsafe {
        SendInput(1, &mut input, size_of::<Input>() as i32);
    }
}

/// Toggle the Windows on-screen keyboard with its built-in shortcut
/// (Win+Ctrl+O, stock since Windows 10 — no Settings change needed).
/// Keys go down in order and up in reverse, like a real hand.
fn toggle_osk() {
    for vk in [VK_LWIN, VK_CONTROL, VK_O] {
        send_key(vk, false);
    }
    for vk in [VK_O, VK_CONTROL, VK_LWIN] {
        send_key(vk, true);
    }
}

/// Single momentary key press (down + up), e.g. one volume notch.
fn tap_key(vk: u16) {
    send_key(vk, false);
    send_key(vk, true);
}

/// Open the Win+X Quick Link menu (Shut down, Restart, Settings...).
fn power_menu() {
    send_key(VK_LWIN, false);
    send_key(VK_X, false);
    send_key(VK_X, true);
    send_key(VK_LWIN, true);
}

fn cursor_pos() -> Option<(f32, f32)> {
    let mut p = Point { x: 0, y: 0 };
    let ok = unsafe { GetCursorPos(&mut p) };
    if ok == 0 {
        None
    } else {
        Some((p.x as f32, p.y as f32))
    }
}

fn screen_size() -> (f32, f32) {
    let w = unsafe { GetSystemMetrics(0) }; // SM_CXSCREEN
    let h = unsafe { GetSystemMetrics(1) }; // SM_CYSCREEN
    (w.max(1) as f32, h.max(1) as f32)
}

fn poll_pad() -> Option<XInputGamepad> {
    use std::sync::OnceLock;
    static API: OnceLock<Option<XInputGetStateFn>> = OnceLock::new();
    let api = API.get_or_init(load_xinput).as_ref()?;
    for index in 0..4 {
        let mut state = XInputState::default();
        let rc = unsafe { api(index, &mut state) };
        if rc == 0 {
            return Some(state.gamepad);
        }
    }
    None
}

// ------------------------------------------------------------- data dir
fn app_dir() -> PathBuf {
    match env::var("LOCALAPPDATA") {
        Ok(v) if !v.is_empty() => PathBuf::from(v).join(APP_NAME),
        _ => env::temp_dir().join(APP_NAME),
    }
}

/// Create the data dir (and the documented whitelist example) on first
/// launch. The driver is standalone: no browser page, no window.
fn ensure_data_dir() -> PathBuf {
    let dir = app_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("{APP_NAME}: cannot create data dir {}: {e}", dir.display());
        std::process::exit(1);
    }
    let list = dir.join("whitelist.txt");
    if !list.is_file() {
        let _ = fs::write(&list, WHITELIST_TEMPLATE);
    }
    dir
}

// ------------------------------------------------------------ driver loop
struct WheelRepeat {
    last: Option<Instant>,
}

impl WheelRepeat {
    /// One notch on press edge, then repeat while held.
    fn poll(&mut self, held: bool, edge: bool, now: Instant) -> bool {
        if edge {
            self.last = Some(now);
            return true;
        }
        if held {
            if let Some(t) = self.last {
                if now.duration_since(t) >= Duration::from_millis(WHEEL_REPEAT_MS) {
                    self.last = Some(now);
                    return true;
                }
            }
        } else {
            self.last = None;
        }
        false
    }
}

fn drive() {
    let mut edges = Edges::default();
    let mut lt = Trigger::default();
    let mut rt = Trigger::default();
    let mut wheel_up = WheelRepeat { last: None };
    let mut wheel_down = WheelRepeat { last: None };
    let mut toggle = Toggle::on();
    let mut x_hold = HoldButton::default();
    let mut power = Combo::default();
    let mut vol_up = WheelRepeat { last: None };
    let mut vol_down = WheelRepeat { last: None };
    let mut arrow_l = HoldButton::default();
    let mut arrow_r = HoldButton::default();
    let whitelist = load_whitelist(&ensure_data_dir());
    let mut scope = Scope::default();
    let mut last = Instant::now();
    loop {
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f32().min(0.05);
        last = now;
        let Some(g) = poll_pad() else {
            edges.reset();
            if x_hold.cancel() {
                send_mouse(MOUSE_LEFT_UP, 0);
            }
            if arrow_l.cancel() {
                send_key(VK_LEFT, true);
            }
            if arrow_r.cancel() {
                send_key(VK_RIGHT, true);
            }
            std::thread::sleep(Duration::from_millis(250));
            last = Instant::now();
            continue;
        };
        // Select+Start toggles the whole driver (for games). State is
        // tracked every frame so toggling never fires a stale edge.
        let start = g.buttons & BTN_START != 0;
        let select = g.buttons & BTN_SELECT != 0;
        let toggled = if let Some(on) = toggle.update(start, select) {
            edges.reset();
            if x_hold.cancel() {
                send_mouse(MOUSE_LEFT_UP, 0);
            }
            if arrow_l.cancel() {
                send_key(VK_LEFT, true);
            }
            if arrow_r.cancel() {
                send_key(VK_RIGHT, true);
            }
            lt = Trigger::default();
            rt = Trigger::default();
            wheel_up = WheelRepeat { last: None };
            wheel_down = WheelRepeat { last: None };
            vol_up = WheelRepeat { last: None };
            vol_down = WheelRepeat { last: None };
            beep(on);
            true
        } else {
            false
        };
        let a = edges.hit(g.buttons, BTN_A);
        let b = edges.hit(g.buttons, BTN_B);
        let y = edges.hit(g.buttons, BTN_Y);
        let x_now = g.buttons & BTN_X != 0;
        let back = edges.hit(g.buttons, BTN_LB);
        let fwd = edges.hit(g.buttons, BTN_RB);
        let up_edge = lt.update(g.left_trigger);
        let down_edge = rt.update(g.right_trigger);
        let vol_up_edge = edges.hit(g.buttons, BTN_DPAD_UP);
        let vol_down_edge = edges.hit(g.buttons, BTN_DPAD_DOWN);
        let dpad_up = g.buttons & BTN_DPAD_UP != 0;
        let dpad_down = g.buttons & BTN_DPAD_DOWN != 0;
        let dpad_left = g.buttons & BTN_DPAD_LEFT != 0;
        let dpad_right = g.buttons & BTN_DPAD_RIGHT != 0;
        // Start+Y power menu. Tracked every frame so it never fires stale;
        // suppressed on toggle frames so Select+Start+Y only toggles.
        let y_now = g.buttons & BTN_Y != 0;
        let power_edge = power.update(start, y_now) && !toggled;
        let live = toggle.enabled && scope.allows(&whitelist);
        // Sticks: left fast, right precise. Y is up on XInput, down on
        // screen, hence the negation.
        let lx = stick(g.thumb_lx);
        let ly = -stick(g.thumb_ly);
        let rx = stick(g.thumb_rx);
        let ry = -stick(g.thumb_ry);
        let dx = step(lx, FAST_SPEED, dt) + step(rx, PRECISE_SPEED, dt);
        let dy = step(ly, FAST_SPEED, dt) + step(ry, PRECISE_SPEED, dt);
        if live && (dx != 0.0 || dy != 0.0) {
            // Read the live cursor so a physical mouse never fights us.
            if let Some((cx, cy)) = cursor_pos() {
                let (sw, sh) = screen_size();
                let nx = (cx + dx).clamp(0.0, sw - 1.0) as i32;
                let ny = (cy + dy).clamp(0.0, sh - 1.0) as i32;
                unsafe {
                    SetCursorPos(nx, ny);
                }
            }
        }
        if live {
            // A is ignored mid-drag: the button is already down, and a
            // click's release would drop the dragged object early.
            if a && !x_hold.holding() {
                left_click();
            }
            if b {
                right_click();
            }
            if back {
                x_button(XBUTTON_BACK);
            }
            if fwd {
                x_button(XBUTTON_FORWARD);
            }
            // Y is suppressed while Start is held: that means Start+Y power
            // menu, not the keyboard. (Hold Start first, then press Y.)
            if y && !start {
                toggle_osk();
            }
            if power_edge {
                power_menu();
            }
            if vol_up.poll(dpad_up, vol_up_edge, now) {
                tap_key(VK_VOLUME_UP);
            }
            if vol_down.poll(dpad_down, vol_down_edge, now) {
                tap_key(VK_VOLUME_DOWN);
            }
            if wheel_up.poll(lt.held(), up_edge, now) {
                wheel(1);
            }
            if wheel_down.poll(rt.held(), down_edge, now) {
                wheel(-1);
            }
        }
        // Hold-to-drag runs outside `live`: engaging needs live, but the
        // release must fire even if the driver was gated mid-hold.
        let (x_press, x_release) = x_hold.update(x_now, live);
        if x_press {
            send_mouse(MOUSE_LEFT_DOWN, 0);
        }
        if x_release {
            send_mouse(MOUSE_LEFT_UP, 0);
        }
        // Arrow keys behave like held keyboard keys (video seek): engaging
        // needs live, but the key-up must fire even if gated mid-hold.
        let (l_press, l_release) = arrow_l.update(dpad_left, live);
        if l_press {
            send_key(VK_LEFT, false);
        }
        if l_release {
            send_key(VK_LEFT, true);
        }
        let (r_press, r_release) = arrow_r.update(dpad_right, live);
        if r_press {
            send_key(VK_RIGHT, false);
        }
        if r_release {
            send_key(VK_RIGHT, true);
        }
        std::thread::sleep(Duration::from_millis(8));
    }
}

fn main() {
    drive();
}

// ------------------------------------------------------------------- tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadzone_kills_drift() {
        assert_eq!(stick(0), 0.0);
        assert_eq!(stick(2000), 0.0); // ~6% deflection
        assert_eq!(stick(-2000), 0.0);
    }

    #[test]
    fn full_deflection_is_full_speed() {
        assert!((stick(32767) - 1.0).abs() < 0.001);
        assert!((stick(-32768) + 1.0).abs() < 0.001);
    }

    #[test]
    fn fast_stick_beats_precise_stick() {
        // One simulated second at full deflection on each stick.
        let dt = 1.0 / 120.0;
        let mut fast = 0.0f32;
        let mut precise = 0.0f32;
        for _ in 0..120 {
            fast += step(stick(32767), FAST_SPEED, dt);
            precise += step(stick(32767), PRECISE_SPEED, dt);
        }
        assert!((fast - FAST_SPEED).abs() < 1.0, "fast={fast}");
        assert!((precise - PRECISE_SPEED).abs() < 1.0, "precise={precise}");
        assert!(fast > precise * 4.0);
    }

    #[test]
    fn edges_fire_once_per_press() {
        let mut e = Edges::default();
        assert!(e.hit(BTN_A, BTN_A));
        assert!(!e.hit(BTN_A, BTN_A)); // held: no repeat
        assert!(!e.hit(0, BTN_A)); // released
        assert!(e.hit(BTN_A, BTN_A)); // pressed again
        // Independent bits do not interfere.
        assert!(e.hit(BTN_B, BTN_B));
        assert!(!e.hit(BTN_A | BTN_B, BTN_A));
    }

    #[test]
    fn trigger_hysteresis() {
        let mut t = Trigger::default();
        assert!(!t.update(50));
        assert!(t.update(150)); // press engages
        assert!(!t.update(200)); // held: silent
        assert!(!t.update(61)); // above release: still silent
        assert!(!t.update(59)); // released, re-arms quietly
        assert!(t.update(101)); // fresh press fires again
    }

    #[test]
    fn trigger_reports_held_state() {
        let mut t = Trigger::default();
        assert!(!t.held());
        assert!(t.update(150)); // press engages
        assert!(t.held());
        assert!(!t.update(200)); // held: silent but still down
        assert!(t.held());
        assert!(!t.update(59)); // released
        assert!(!t.held());
    }

    #[test]
    fn wheel_repeats_while_held() {
        let mut w = WheelRepeat { last: None };
        let t0 = Instant::now();
        assert!(w.poll(true, true, t0)); // edge: immediate notch
        assert!(!w.poll(true, false, t0)); // same instant: quiet
        assert!(w.poll(true, false, t0 + Duration::from_millis(150))); // repeat
        assert!(!w.poll(false, false, t0 + Duration::from_millis(300))); // let go
        assert!(!w.poll(false, false, t0 + Duration::from_millis(500)));
    }

    #[test]
    fn hold_button_press_release() {
        let mut h = HoldButton::default();
        assert_eq!(h.update(false, true), (false, false));
        assert_eq!(h.update(true, true), (true, false)); // press engages
        assert_eq!(h.update(true, true), (false, false)); // held: silent
        assert_eq!(h.update(false, true), (false, true)); // release fires
        assert_eq!(h.update(false, true), (false, false));
    }

    #[test]
    fn hold_button_never_sticks() {
        let mut h = HoldButton::default();
        // Pressed while gated (driver off / wrong app): no engage.
        assert_eq!(h.update(true, false), (false, false));
        assert!(!h.holding());
        h.update(true, true);
        assert!(h.holding());
        // Released while gated: the release still fires.
        assert_eq!(h.update(false, false), (false, true));
        assert!(!h.holding());
        // Forced cancel (toggle-off, pad lost): release owed exactly once.
        let mut h2 = HoldButton::default();
        assert!(!h2.cancel()); // nothing held: quiet
        h2.update(true, true);
        assert!(h2.cancel()); // release owed
        assert!(!h2.cancel()); // second cancel: quiet
    }

    #[test]
    fn input_struct_layout() {
        assert_eq!(size_of::<Input>(), 40);
        assert_eq!(size_of::<KeyboardInput>(), 24);
    }

    #[test]
    fn xinput_resolves_on_windows() {
        assert!(
            load_xinput().is_some(),
            "no XInput DLL with XInputGetState found"
        );
    }

    #[test]
    fn toggle_combo_switches_once_per_hold() {
        let mut t = Toggle::on();
        assert!(t.enabled);
        assert_eq!(t.update(false, false), None);
        assert_eq!(t.update(true, false), None); // Start alone: nothing
        assert_eq!(t.update(true, true), Some(false)); // combo: off
        assert_eq!(t.update(true, true), None); // held: silent
        assert_eq!(t.update(false, true), None); // releasing
        assert_eq!(t.update(true, true), Some(true)); // combo: on again
        assert!(t.enabled);
    }

    #[test]
    fn power_combo_fires_once_per_hold() {
        let mut c = Combo::default();
        assert!(!c.update(false, false));
        assert!(!c.update(true, false)); // Start alone: nothing
        assert!(!c.update(false, true)); // Y alone: nothing (Y keeps its own action)
        assert!(c.update(true, true)); // Start+Y: fires
        assert!(!c.update(true, true)); // held: silent
        assert!(!c.update(true, false)); // Y released: quiet
        assert!(c.update(true, true)); // pressed again: fires
    }

    #[test]
    fn scope_matching() {
        let empty: Vec<String> = vec![];
        assert!(scope_allows(&empty, None));
        assert!(scope_allows(&empty, Some("C:\\Games\\game.exe")));
        let list = vec!["msedge.exe".to_string(), "firefox.exe".to_string()];
        assert!(scope_allows(
            &list,
            Some("C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe")
        ));
        assert!(scope_allows(&list, Some("FIREFOX.EXE")));
        assert!(!scope_allows(&list, Some("C:\\Games\\game.exe")));
        assert!(!scope_allows(&list, None)); // unknown: fail closed
    }
}
