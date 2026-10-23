# Ctrl+Alt+Del in driver mode

[Русская версия](../ru/ctrl-alt-del.md)

In driver mode Ctrl+Alt+Del does nothing. In HW mode it works as on any keyboard. This
page explains why and lists the ways around it that were studied.

## Why it does not work

In driver mode the firmware types nothing: it only streams key depth, and EasyRazer
turns depth into keystrokes with `SendInput`. Windows marks such input as injected and
opens the Ctrl+Alt+Del screen (the secure attention sequence, SAS) only for input from
a keyboard driver. Privileges do not change this: `SendInput` from a SYSTEM process is
still injected input. What matters is the path the keystroke takes, not who sends it.

Use HW mode when you need Ctrl+Alt+Del.

## Approaches studied

| Approach | Result |
|---|---|
| [Switch to HW on Ctrl+Alt](#switch-to-hw-on-ctrlalt) | tested on hardware, does not work |
| [`SendSAS`](#sendsas) | works, but needs a service and a system-wide policy |
| [`uiAccess`, SYSTEM helper](#uiaccess-and-a-system-helper) | do not help without `SendSAS` |
| [Razer kernel driver](#razer-kernel-driver) | rejected |
| [Own virtual keyboard driver](#own-virtual-keyboard-driver) | feasible, needs driver signing |
| [Firmware typing in driver mode](#firmware-typing-in-driver-mode) | not found |
| [Firmware patch](#firmware-patch) | not pursued |

### Switch to HW on Ctrl+Alt

When the engine sees Ctrl and Alt held, it switches the keyboard to HW mode (about
50 ms) so the firmware types Del itself.

It does not work. The firmware presses a key only when the key crosses its actuation
point, so Ctrl and Alt held across the switch stay up for Windows until pressed again.
Del then arrives without modifiers.

### `SendSAS`

`SendSAS` (`sas.dll`) opens the Ctrl+Alt+Del screen directly. The caller must be a
service or run as LocalSystem, or be a `uiAccess` app signed with Authenticode and
installed in Program Files or System32. In every case the "Disable or enable software
Secure Attention Sequence" group policy (`SoftwareSASGeneration`) has to allow it.

Rejected: EasyRazer is a single portable `.exe` and does not install services or change
machine-wide policies.

### `uiAccess` and a SYSTEM helper

A signed `uiAccess` app can send input into elevated windows, and a SYSTEM helper has
even more rights. Both still send injected input, so neither opens the Ctrl+Alt+Del
screen on its own; each is useful here only as a way to call `SendSAS`.

### Razer kernel driver

Rejected as too troublesome and unsafe. The driver is undocumented and could not be
analysed, and current Razer software does not need it: Synapse 4 talks to the keyboard
over plain HID and types through `SendInput` too. Synapse has no Ctrl+Alt+Del handling
in driver mode either.

### Own virtual keyboard driver

A virtual HID keyboard (for example, a UMDF driver over VHF) is a real keyboard for
Windows, so Ctrl+Alt+Del from it would work. The cost: the driver must be signed by
Microsoft (EV certificate, attestation), installed separately, and must accept input
only from EasyRazer, otherwise any unprivileged process gets a way to type
non-injected input. Ready-made generic input drivers have the same security problem and
add an external dependency.

### Firmware typing in driver mode

If the firmware could type a key on a host command while in driver mode, Windows would
receive it as keyboard input. No such command is known. The firmware plays macros only
in HW mode ([V2 Analog protocol](devices/huntsman-v2-analog/protocol.md#macros)).
Static analysis of the V2 Analog firmware found code paths that might emit key events in
driver mode, but none has been confirmed on hardware.

### Firmware patch

A patched firmware could let the firmware type Ctrl+Alt+Del in driver mode. It is
possible in principle, but it risks bricking the keyboard and would cover one line
only.
