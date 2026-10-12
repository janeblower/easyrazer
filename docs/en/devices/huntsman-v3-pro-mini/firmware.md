# Huntsman V3 Pro Mini firmware

Analysis of the official Firmware Updater (firmware `2.01.00`). A reference for the
protocol: firmware code is not carried over into the project. Addresses are for this
version. The firmware is built from the same code as the V3 Pro one
([V3 Pro firmware](../huntsman-v3-pro/firmware.md)); here only what was checked and
what differs.

## Image

`DeviceUpdater.resources`:

| Key | Value |
|---|---|
| `VID` / `PID` | `1532` / `02B0` |
| `BLPID` / `BLVER` | `110E` / `2.0` — bootloader, as on the V3 Pro |
| `DevFWVer` / `FlashFWVer` | `2.01.00` / `2.01.00` |
| `FeatureReportLen` | `91` |
| `RYFWFileName` | `Talia_T1_60%_7203_App_FW_v2.1.0_E888780F.enc` |

`DevFWLine0..8195` is Intel HEX of the main MCU, `0x20000000`–`0x2001FFFF`;
`FlashFWSector0..36` is the encrypted image of the second chip, as on the V3 Pro.

Scatter-load table is at `0x2001E584`: `0x2001E600` → `0x20020800` (`0x6400`) and
`0x2001FC00` → `0x04000000` (`0x600`) are decompressed, `0x20026C00` (`0x6108`) is
zeroed. The decompressor is the same Keil LZ at `0x20000568`.

## Same as the V3 Pro

- USB: 6 interfaces; control is a 90-byte feature report, LampArray is there, the
  vendor reports `04`, `05`, `07`, `0B` are 47 bytes each. Device descriptor is
  `0x04000340`.
- Actuation step table `0x2001B488` and RT table `0x2001DD7C`: `1638·{1, 4, 8, …}`
  and `1638·{1…10}`, so the same u16 scale, 1638 = 0.1 mm.

## Key matrix

Sensor matrix is 5 × 14 (`0x200269C0` after scatter-load), 68 keys and two empty
cells, fwIDs as on the V2 and V3 Pro:

```
110   2   3   4   5   6   7   8   9  16  17  18  19  20
 21  22  23  24  26  25  10  11  12  13  14  15   0  30
 31  32  33  34  35  36  37  38  44  45  46  47  48  49
 50  51  52  58 127  60 150  61 154 151  62  59  27  28
 29  39  40  41  42  43   0  53  54  55  56  57 129  64
```

The ANSI board uses 61 of them: the main block of the full-size layout with Esc
(`110`) in place of the tilde. The rest are ISO/JIS keys.

## Not analyzed

- Class dispatcher and the command set: assumed the same as on the V3 Pro.
- Lighting matrix size.
