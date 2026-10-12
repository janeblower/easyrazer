# Huntsman V3 Pro Mini protocol

Not verified on hardware. Per the firmware 2.01.00 ([firmware.md](firmware.md)) the
protocol is that of the [V3 Pro](../huntsman-v3-pro/protocol.md): the same HID
interfaces, control on interface 3, u16 thresholds, Rapid Trigger and Snap Tap in
firmware, depth stream in report `0B`.

## Differences

- PID `02B0`.
- 60% board: sensor matrix 5 × 14, 61 keys in ANSI. No F-row, arrows, navigation
  block, numpad, media buttons or dial.
- Lighting matrix size is unknown, so the custom effect is not offered.
