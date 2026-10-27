# SkyBrowser — controller-driven PC driver

## What this is
`SkyBrowser.exe` turns a standard Xbox-style controller into a full PC
mouse: it reads the gamepad through XInput and moves the **real Windows
cursor**, so it works fullscreen in every browser, site, and app.
Pure Rust standard library, no downloads, ~300 KB. Standalone — no
browser, no window; it just runs quietly in the background.

## Controls
```
  CONTROLS (XBOX LAYOUT)
  LT scroll up / LB back              RB forward / RT scroll down
 .--------------------------------------------------------------.
 |  LEFT STICK                   SELECT + START     Y           |
 |  fast cursor                  driver off / on X     B        |
 |                                                  A           |
 |  D-pad: not used               RIGHT STICK                   |
 |                                precise cursor                |
 '--------------------------------------------------------------'

  A = left click   B = right click   X = hold to drag   Y = keyboard
```
- Left stick — fast cursor, Right stick — precise cursor
- A — left click, B — right click
- LT / RT — scroll up / down (hold to keep scrolling)
- LB — back, RB — forward
- X — hold to drag: keep X held and move a stick, release X to drop.
  A is ignored while X is held so drags never drop early.
- Y — on-screen keyboard on / off (toggle needs no setup; to *click*
  its keys, run the Admin launcher — Windows ignores simulated
  clicks on that secure window from non-admin apps)
- **Select + Start — driver off / on** (it beeps: high = on, low = off).
  Use it before launching a game so nothing collides.

## Limit it to chosen programs (optional)
On first launch the exe creates
`%LOCALAPPDATA%\SkyBrowser\whitelist.txt` (all lines commented = drive
everywhere). Add exe names, one per line, e.g. `msedge.exe`, and the
driver only injects input when one of those programs is in front.
Delete the entries to go back to everywhere.

## Files
- `SkyBrowser.exe` — double-click to run (stays resident with no window;
  end it in Task Manager when done).
- `SkyBrowser Admin.bat` — same thing, elevated: use it when you want
  clicks to reach the on-screen keyboard. One UAC prompt per launch.
- `main.rs` — the whole driver source, unit tests included.
- `build.bat` — gate: Rust tests, rebuild exe.

## Rebuild
Install Rust, then run `build.bat`.

## License
MIT — free for anyone to use, see `LICENSE`.

## Continuing this project in a new chat
Chat history does not survive a reboot. To continue: open a new chat,
point at this folder (`Desktop\SkyBrowser`), and say what to change.
Useful commands: `muse resume --last` (reopens the previous chat).
Built across Muse sessions starting with
`01a17c57-1dbe-7de2-84cd-8f881f25b3e8` (the old browser start page was
removed; the driver is standalone now).
