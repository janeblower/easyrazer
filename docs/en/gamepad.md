# Gamepad editor

The Gamepad tab maps analog keyboard keys to an Xbox 360 controller. Assignments
and the enable switch belong to the active profile and travel with profile
copies and exported files. The default layout assigns WASD to the left stick
and Q/E to the left/right triggers. Existing empty layouts receive these defaults
once; custom assignments and later clearing of assignments are preserved.

The editor, including the controller preview, is inactive in hardware mode
and until the hidshim driver is installed. Click **Install hidshim** in the tab
and approve Windows' administrator prompt. EasyRazer downloads the pinned
[hidshim](https://github.com/janeblower/hidshim) **0.1.0** release, verifies its
SHA-256 and runs its installer. hidshim copies itself to `Program Files\hidshim`,
appears among installed apps and can be uninstalled there or in EasyRazer's
Settings. It signs its user-mode driver on the machine with a certificate whose
key exists only in memory during signing and trusts that certificate in Windows'
local-machine Root and TrustedPublisher stores; uninstalling removes it. It does
not change Secure Boot or test-signing settings. If HIDMaestro, which earlier
versions used, is still installed, EasyRazer offers to remove it first.

Enable driver mode to receive live key depth. Select a controller control and
assign a keyboard key, then apply the changes. The **Enabled in this profile**
switch takes effect at once, without Apply. Gamepad output starts when both
driver mode and the switch are on. EasyRazer drives the pad itself, without
administrator rights. Windows asks for them only when the pad has to be brought
back, once after each restart of Windows, and only when you turn the gamepad on
with the switch or Fn+G; starting EasyRazer never asks. If the pad cannot start,
the editor shows why with a **Retry** button, and the keyboard engine keeps
running.

Fn+G toggles the gamepad in the active profile immediately, without pressing
Apply. EasyRazer binds it in every profile once; like Snap Tap's Fn+LShift, it
can be moved to another key of the Hypershift layer.

Both sticks use the full signed XInput range. Opposite directions subtract;
equal depths cancel. Triggers follow depth linearly from 0 to 255. Buttons,
stick clicks, bumpers and the D-pad activate at depth 128. Assigned keys are
suppressed in both keyboard layers, except for EasyRazer's own Fn switches
(next profile, Snap Tap, gamepad), and Snap Tap groups containing them are
suspended while gamepad output is enabled. These keys remain suppressed if the
pad fails to start; disable the gamepad switch to restore their keyboard bindings.

Each stick direction and trigger has a **Maximum depth** slider beside its
assignment. At the default 100%, full output requires full reported key travel;
at 50%, half travel produces full output. Deeper presses stay at 100% output.
The controller preview uses the same scaling. Trigger highlighting marks full
output. Use **Clear selection** or Esc to clear the selected controller control;
Esc also clears key selections in the other tabs.

Switching profiles or assignments releases previous outputs. Leaving driver
mode, disconnecting the keyboard or closing EasyRazer releases keyboard keys
and closes that keyboard's virtual controller. While the focused window does not
accept injected keys, such as an app running as administrator, the controller
holds neutral and resumes when focus returns. After the keyboard reconnects or
Synapse closes, the controller comes back by itself.
Live capture remains available in driver mode with the gamepad switch disabled.
The app requests depth snapshots only while the editor is visible.

The keyboard reader submits input straight to the driver, and the pad
disappears from games as soon as EasyRazer closes or crashes. Compatibility with
individual games is not verified.
