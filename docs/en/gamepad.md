# Gamepad editor

The Gamepad tab maps analog keyboard keys to an Xbox 360 controller. Assignments
and the enable switch belong to the active profile and travel with profile
copies and exported files. The default layout assigns WASD to the left stick
and Q/E to the left/right triggers. Existing empty layouts receive these defaults
once; custom assignments and later clearing of assignments are preserved.

The editor, including the controller preview, is inactive in hardware mode
and until both HIDMaestro driver packages are installed. Click **Install
HIDMaestro** in the tab and approve Windows' administrator prompt. EasyRazer
downloads the pinned upstream **v1.12.0** x64 release, verifies its SHA-256,
and installs both driver packages, version **1.12.0.2302**, in the background.
An animated indicator stays visible during download and installation; errors
leave the install button available to retry. Installation uses the upstream
signing tools: it creates a HIDMaestro code-signing certificate, trusts it in
Windows' local-machine Root and TrustedPublisher stores and deletes its private
key once the drivers are signed. Uninstalling HIDMaestro in Settings removes the
certificate from these stores. It does not change Secure Boot or test-signing
settings. Launch EasyRazer normally; the experimental
`--gamepad-test` flag is no longer used.

Enable driver mode to receive live key depth. Select a controller control and
assign a keyboard key, then apply the changes. Gamepad output starts when both
driver mode and the gamepad profile switch are enabled. Only the controller
helper requests administrator rights, and Windows asks each time it starts:
when EasyRazer starts with the gamepad enabled in the active profile (autostart
included) and each time Apply or Fn+G turns the gamepad on. A failed controller
start leaves the keyboard engine running; apply the configuration again to
retry. The watcher does not repeatedly request elevation
after a failure.

Fn+G toggles the gamepad in the active profile immediately, without pressing
Apply. EasyRazer binds it in every profile once; like Snap Tap's Fn+LShift, it
can be moved to another key of the Hypershift layer.

Both sticks use the full signed XInput range. Opposite directions subtract;
equal depths cancel. Triggers follow depth linearly from 0 to 255. Buttons,
stick clicks, bumpers and the D-pad activate at depth 128. Assigned keys are
suppressed in both keyboard layers, except for EasyRazer's own Fn switches
(next profile, Snap Tap, gamepad), and Snap Tap groups containing them are
suspended while gamepad output is enabled. These keys remain suppressed if the
helper fails; disable the gamepad switch to restore their keyboard bindings.

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
Synapse closes, apply the configuration again to restart the controller.
Live capture remains available in driver mode with the gamepad switch disabled.
The app requests depth snapshots only while the editor is visible.

Input is submitted directly to HIDMaestro shared memory from the keyboard
reader. The helper watches the app process so a crash also triggers cleanup.
Disabling the gamepad neutralizes input and closes the writer before signaling
the helper to stop. Cleanup is checked without blocking the keyboard reader or
the interface. A new helper cannot start while the previous helper is stopping;
apply again once cleanup finishes if a restart was requested during that time.
Cleanup errors are shown in the editor.

The pinned HIDMaestro XUSB driver can retain shared mappings after a controller
disappears from XInput if another process holds cached XInput handles without
polling again. A new controller may then fail to start until that process
releases its handles. Compatibility with individual games is not verified.
