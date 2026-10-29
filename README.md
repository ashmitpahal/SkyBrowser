# SkyBrowser V2 — your controller is your mouse and keyboard

SkyBrowser turns a standard Xbox-style controller into a full PC mouse and
keyboard. It reads the gamepad and moves the **real Windows cursor**, so it
works fullscreen in every browser, site, video player, and app. No window,
no setup — it just runs quietly in the background.

![Labelled controller diagram](controller-diagram.png)

Prefer pictures? See the illustrated [user guide (PDF)](SkyBrowserV2-Guide.pdf).

## Every control

| Control | Action |
|---|---|
| Left stick | Fast mouse cursor |
| Right stick | Precise mouse cursor |
| A | Left click |
| B | Right click |
| X (hold) + move stick | Hold to drag, release X to drop |
| Y | On-screen keyboard on / off |
| LT / RT (hold) | Scroll up / down (hold to keep scrolling) |
| LB / RB | Browser back / forward |
| D-pad up / down | Volume up / down (hold to repeat) |
| D-pad left / right | Arrow keys — tap skips video, hold keeps seeking |
| Start + Y | Power menu (hold Start first, then press Y) |
| Select + Start | Driver on / off (beeps: high = on, low = off) |

## Details

**Mouse.** Two sticks, two speeds — left is fast for crossing the screen,
right is precise for small targets. Both can be used together, and a physical
mouse never fights the driver.

**Click and drag.** A = left click, B = right click. For drags, hold X and
move a stick, then release X to drop. A is ignored mid-drag so you never
drop things early, and the release always fires so drags never get stuck.

**Scrolling.** Hold LT to scroll up, RT to scroll down. Holding keeps
scrolling, one notch at a time.

**Volume and video.** D-pad up / down turns the volume up / down (hold to
keep changing it). D-pad left / right are the arrow keys: tap to skip a
video backward / forward, hold to keep seeking.

**Keyboard.** Y toggles the Windows on-screen keyboard. Toggling needs no
setup, but *clicking* the keyboard's keys needs the Admin launcher, because
Windows ignores simulated clicks on that secure window from normal apps.

**Power menu.** Hold Start first, then press Y to open the Win+X quick menu
(Shut down, Restart, Sleep, Settings, and more).

**Driver on / off.** Press Select + Start together to pause the whole driver
(for games) and again to resume. It beeps so you know: high = on, low = off.

## Limit it to chosen programs (optional)

On first launch the app creates
`%LOCALAPPDATA%\SkyBrowser\whitelist.txt` (all lines commented = works
everywhere). Add program names, one per line, e.g. `msedge.exe`, and the
driver only works when one of those programs is in front. Delete the entries
to go back to everywhere.

## Running it

- Double-click `SkyBrowser.exe`. There is no window — it stays quiet in the
  background. Stop it in Task Manager when done.
- Run only **one copy** at a time, or they fight over your controller.
- Use `SkyBrowser Admin.bat` when you want clicks to reach the on-screen
  keyboard (one permission prompt per launch).
- Press Select + Start before launching a game so nothing collides.

## Files

| File | What it is |
|---|---|
| `SkyBrowser.exe` | The app (also attached to each Release) |
| `SkyBrowser Admin.bat` | Elevated launcher for on-screen keyboard clicks |
| `main.rs` | The whole driver source, tests included |
| `build.bat` | Runs tests, rebuilds the exe |
| `SkyBrowserV2-Guide.pdf` | Illustrated user guide |
| `controller-diagram.png` | Labelled controller diagram |

## Specs

Single ~208 KB file, about 9.5 MB of RAM, pure Rust standard library with
zero downloads.



MIT — free for anyone to use, see `LICENSE`. Copyright 2026 ashmit pahal.
