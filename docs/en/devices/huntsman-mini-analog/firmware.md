# Huntsman Mini Analog firmware

Analysis of the official Firmware Updater (firmware `1.03.00`). A reference for the
protocol: firmware code is not carried over into the project. Addresses are for this
version. The firmware is built from the same code as the V2 Analog one
([V2 Analog firmware](../huntsman-v2-analog/firmware.md)); here only what was checked
and what differs.

## Image

`DeviceUpdater.resources`:

| Key | Value |
|---|---|
| `VID` / `PID` | `1532` / `0282` |
| `BLPID` / `BLVER` | `110E` / `2.0` |
| `DevFWVer` | `1.03.00` |
| `FlashFWUpdate` | `0`: no second chip |

`DevFWLine0..5179` is Intel HEX, `0x08007000`–`0x0801B373`, 82,804 bytes, not
encrypted. As on the V2 Analog, the bootloader takes `0x08000000`–`0x08006FFF`.

## Same as the V2 Analog

- USB: 5 interfaces with the same HID descriptors (61, 212, 79, 22, 327 bytes), the
  configuration descriptor is at `0x0801936C`; control is a 90-byte feature report,
  the depth stream is report `07`.
- Threshold clamp: the tail of the function at `0x08016D4C` matches the V2 one
  byte for byte: press `15..250`, release no less than `5` and no more than press
  − 10.

## Key table

`0x08019E34`: 68 fwIDs, as on the V2 and V3 Pro:

```
110   2   3   4   5   6   7   8   9  10  11  12  13  15
 16  17  18  19  20  21  22  23  24  25  26  27  28  29
 30  31  32  33  34  35  36  37  38  39  40  41  42  43
 44  45  46  47  48  49  50  51  52  53  54  55  56  57
 58 127  60 150  61 154 151  62  59 129  64  14
```

The set is the same as in the V3 Pro Mini matrix: 61 ANSI keys with Esc (`110`) in
place of the tilde, the rest are ISO/JIS keys.

## Not analyzed

- Class dispatcher, the command set and the profile layout: assumed the same as on
  the V2 Analog.
- Lighting matrix size.
