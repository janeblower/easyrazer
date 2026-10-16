# Gamepad editor

The Gamepad tab maps analog keyboard keys to an Xbox 360 controller. Assignments
and the enable switch belong to the active profile and travel with profile
copies and exported files. The default layout assigns WASD to the left stick
and Q/E to the left/right triggers. Existing empty layouts receive these defaults
once; custom assignments and later clearing of assignments are preserved.

Install both HIDMaestro v1.12.0 driver packages, version **1.12.0.2302**,
separately, following the upstream instructions. EasyRazer does not install
drivers or change Windows security settings. Launch EasyRazer normally; the
experimental `--gamepad-test` flag is no longer used.

Enable driver mode to receive live key depth. Select a controller control and
assign a keyboard key, then apply the changes. Gamepad output starts when both
driver mode and the gamepad profile switch are enabled. Only the controller
helper requests administrator rights. A failed controller start leaves the
keyboard engine running; apply the configuration again to retry. The watcher
does not repeatedly request elevation after a failure.

The default Fn+F10 game-mode binding also toggles the gamepad in the active
profile immediately, without pressing Apply. Its Win-key blocking behavior remains.

Both sticks use the full signed XInput range. Opposite directions subtract;
equal depths cancel. Triggers follow depth linearly from 0 to 255. Buttons,
stick clicks, bumpers and the D-pad activate at depth 128. Assigned keys are
suppressed in both keyboard layers, and Snap Tap groups containing them are
suspended while gamepad output is enabled. These keys remain suppressed if the
helper fails; disable the gamepad switch to restore their keyboard bindings.

Switching profiles or assignments releases previous outputs. Leaving driver
mode, disconnecting the keyboard or closing EasyRazer releases keyboard keys
and closes that keyboard's virtual controller. After a temporary driver-mode
interruption, apply the configuration again if the controller needs restarting.
Live capture remains available in driver mode with the gamepad switch disabled.
The app requests depth snapshots only while the editor is visible.

Input is submitted directly to HIDMaestro shared memory from the keyboard
reader. The helper watches the app process so a crash also triggers cleanup.
Normal exit closes the writer before waiting for helper cleanup.

The pinned HIDMaestro XUSB driver can retain shared mappings after a controller
disappears from XInput if another process holds cached XInput handles without
polling again. A new controller may then fail to start until that process
releases its handles. Compatibility with individual games is not verified.
