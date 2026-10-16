# Experimental gamepad test

This opt-in test uses one connected keyboard in driver mode and one virtual
Xbox 360 controller. There is no gamepad editor and no saved gamepad profile.

Install both HIDMaestro v1.12.0 driver packages, version **1.12.0.2302**,
separately, following the upstream instructions. EasyRazer does not install
drivers or change Windows security settings.

Build from the repository root:

```powershell
cargo build -p easyrazer
```

Quit any running EasyRazer instance, then launch normally:

```powershell
.\target\debug\easyrazer.exe --gamepad-test
```

Only the helper requests administrator rights. Enable the app's existing
driver-mode switch. The test mapping overrides these keys in both layers:

| Keyboard | Gamepad |
| --- | --- |
| W/A/S/D | Left stick |
| Q/E | LT/RT |
| J/K/U/I | A/B/X/Y |

Stick and trigger values follow the full reported depth linearly. Equal
opposite directions cancel; face buttons activate at depth 128 of 255.
Mapped keys do not type in driver mode, and Snap Tap groups containing them
are suspended for the test. Other key assignments work normally.

Stopping driver mode or losing the keyboard returns the gamepad to neutral.
Closing EasyRazer closes the controller. Losing the helper stops driver mode;
restart the app to retry. Starting without the flag restores normal behaviour.
The flag is not added to Windows autostart.

Input is published directly into HIDMaestro's shared memory from the
keyboard callback. The helper passes only the input mapping and its two
event handles into EasyRazer; no TCP connection, frame acknowledgements or
periodic sender thread is involved. Local events control startup and stop.
The helper also watches the application process so a crash triggers cleanup.
Normal application exit closes the writer before waiting for helper cleanup.

The pinned HIDMaestro XUSB driver can keep shared mappings after the
controller disappears from XInput if another process holds cached XInput
handles without polling again. This prevents the next creation until that
process releases its handles. Normal shutdown, application crash and
immediate recreation were checked with short-lived XInput observers.
Compatibility with games is not verified.
