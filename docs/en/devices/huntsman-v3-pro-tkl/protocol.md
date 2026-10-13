# Huntsman V3 Pro TKL protocol

Not verified on hardware. Per the firmware 1.04.00 ([firmware.md](firmware.md)) the
protocol is that of the [V3 Pro](../huntsman-v3-pro/protocol.md): the same HID
interfaces, control on interface 3, u16 thresholds, Rapid Trigger and Snap Tap in
firmware, depth stream in report `0B`. The TKL 8 kHz (`02D0`) is the same.

## Differences

- PID `02A7`; `02D0` for the 8 kHz version.
- Tenkeyless board: sensor matrix 7 × 14, 84 keys in ANSI. No numpad and no
  PrtSc / ScrLk / Pause.
- Lighting matrix size is unknown, so the custom effect is not offered.
