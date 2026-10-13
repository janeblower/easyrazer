# Huntsman V3 Pro TKL firmware

Analysis of the official Firmware Updaters for the TKL (`02A7`) and the TKL 8 kHz
(`02D0`), both firmware `1.04.00`. A reference for the protocol: firmware code is not
carried over into the project. Addresses are for these versions. The firmware is built
from the same code as the V3 Pro one ([V3 Pro firmware](../huntsman-v3-pro/firmware.md));
here only what was checked and what differs.

## Image

`DeviceUpdater.resources`:

| Key | TKL | TKL 8 kHz |
|---|---|---|
| `VID` / `PID` | `1532` / `02A7` | `1532` / `02D0` |
| `BLPID` / `BLVER` | `110E` / `2.0` | `110E` / `2.0` |
| `DevFWVer` / `FlashFWVer` | `1.04.00` / `1.04.00` | `1.04.00` / `1.04.00` |
| `RYFWFileName` | `Talia_T1_TKL_7203_App_FW_20240428_v1.4.0_3CF72FF3.enc` | `Talia_T1_TKL_Refresh_5528_App_FW_v1.4.0_91871BFB.enc` |

`DevFWLine*` is Intel HEX of the main MCU from `0x20000000`; `FlashFWSector0..36` is
the encrypted image of the second chip, as on the V3 Pro.

Scatter-load table: TKL `0x2001E474`, TKL 8 kHz `0x20022134`. The decompressor is the
same Keil LZ at `0x20000568`.

## Same as the V3 Pro

- USB: 6 interfaces with the same HID descriptors (61, 237, 79, 22, 327, 30 bytes);
  control is a 90-byte feature report, the vendor reports `04`, `05`, `07`, `0B` are
  47 bytes each.
- Actuation step table and RT table: `1638·{1, 4, 8, …}` and `1638·{1…10}`, the same
  u16 scale, 1638 = 0.1 mm. TKL `0x2001B258` / `0x2001DC44`, TKL 8 kHz `0x2001EB88` /
  `0x20021810`.

## Key matrix

Sensor matrix is 7 × 14, the same in both (TKL `0x2001CEB0`, TKL 8 kHz `0x20020A7C`),
fwIDs as on the V2 and V3 Pro:

```
110 112 113 114 115 116 117 118 119 120 121 122   1   2
  3   4   5   6   7   8   9  10  11  12 123  13  14  15
 75  80  85  28  29  76  81  86  16  17  18  19  20  21
 22  23  24  25  26   0  30  31  32  33  34  35  36  37
 38  39  40   0  44  45  46  47  48  49  50  51  52  53
 54   0  58 127  60 150  61 154 151  62  59 129  64   0
 27  41  42  43  55  56  57  83  79  84  89   0  87   0
```

The ANSI board uses 84 of them: the full-size layout without the numpad and without
PrtSc / ScrLk / Pause (`124`–`126`). The Synapse module for `02A7` has no such keys
either. The rest are ISO/JIS keys and `87`, which the Synapse module does not map.

## Not analyzed

- Class dispatcher and the command set: assumed the same as on the V3 Pro.
- What the 8 kHz version adds: extra HID descriptors in its image, polling rate
  commands.
- Lighting matrix size.
