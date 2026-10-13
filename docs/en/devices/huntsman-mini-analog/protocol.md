# Huntsman Mini Analog protocol

Not verified on hardware. Per the firmware 1.03.00 ([firmware.md](firmware.md)) the
protocol is that of the [V2 Analog](../huntsman-v2-analog/protocol.md): the same HID
interfaces, control on interface 3, 1-byte thresholds clamped to `15..250`, Rapid
Trigger and Snap Tap only on the host, depth stream in report `07`.

## Differences

- PID `0282`.
- 60% board: 68 keys in the firmware table, 61 of them in ANSI, the same set as the
  [V3 Pro Mini](../huntsman-v3-pro-mini/protocol.md). No F-row, arrows, navigation
  block, numpad, media buttons or dial.
- Lighting matrix size is unknown, so the custom effect is not offered.
