# Keyboard models

[Русская версия](../../ru/devices/README.md)

Protocol and firmware, per keyboard line: models that differ only in form factor share
one document. This page covers what the lines share and how they differ.

| Line | Models | MCU | EasyRazer | Documents |
|---|---|---|---|---|
| Huntsman V2 Analog | V2 Analog (`0266`), Mini Analog (`0282`) | STM32L4+ | V2 Analog supported; Mini Analog in development, RAM writes only, not tested on hardware | [protocol](huntsman-v2-analog/protocol.md), [firmware](huntsman-v2-analog/firmware.md) |
| Huntsman V3, analog | V3 Pro (`02A6`), V3 Pro TKL (`02A7`), V3 Pro TKL 8KHz (`02D0`), V3 Pro Mini (`02B0`), V3 TKL 8KHz (`02E5`), V3 Pro 8KHz (`02CF`), Signature Edition (`02D8`), V3 Pro Low-profile TKL 8KHz (`02E6`) | NXP LPC55xx, probably LPC55S6x | in development, RAM writes only, not tested on hardware | [protocol](huntsman-v3-analog/protocol.md), [firmware](huntsman-v3-analog/firmware.md) |

The V3 Pro has not been verified on hardware: everything about it comes from the
Synapse 4 module and firmware 1.03.00. The Synapse modules for the V2 Analog,
Huntsman Mini Analog (`0282`), V3 Pro, V3 Pro TKL (`02A7`) and V3 Pro Mini
(`02B0`) are built from one branch, and the V2 and V3 Pro share one command
table. The TKL firmware 1.04.00 and the Mini firmware 2.01.00 are V3 Pro builds with
another key matrix; the Mini Analog firmware 1.03.00 is a V2 Analog build with a 60%
key table. The V3 Pro 8KHz and Signature Edition firmware are V3 Pro builds too. The V3 TKL
8KHz and V3 Pro Low-profile TKL 8KHz firmware are a separate code base, but their
Synapse modules send the same commands as the V3 Pro TKL one.

## Control packet

Feature report of 91 bytes: report ID `00` + 90 bytes. The format is the same on
both models (for the V3 Pro, per the firmware).

```
0      status: request 00; response 02 ok, 01 busy, 03 fail, 04 timeout, 05 not supported
1      transaction id (the device accepts any; we use 1F)
2-3    remaining packets
4      protocol type (0)
5      data size — actual payload length
6      command class
7      command id; get = set | 0x80
8-87   args[80]
88     crc = XOR of bytes 2..87
89     reserved
```

Exchange: `SetFeature` → `GetFeature`. The interface number and handling details
are in the model's protocol doc.

## How they differ

| | V2 Analog | V3 Pro |
|---|---|---|
| Threshold in the protocol | 1 byte, `0..255` ↔ 1.5–3.6 mm | u16 BE, 1638 = 0.1 mm, up to 4.0 mm |
| Actuation | in the key assignment, `02:12` / `02:92` | `02:19` / `02:99`, up to 15 keys per packet; `02:12` also works, threshold is the high byte of the u16 |
| Rapid Trigger | host only, in driver mode | in firmware: `02:1A` / `02:9A`, `02:20` |
| Snap Tap | host only | in firmware, one key pair: `02:21` |
| Depth stream (driver mode) | report `07`, 1-byte depth | report `0B`, u16 BE depth |
| Saving to flash | deferred, on a timer | immediate, on every command to profile 1–6 |
| Sensor matrix | 8 × 16 | 8 × 14, same fwIDs except `152` / `153` |
| Lighting | `0F:02/03/04`, LED `05`, effects `00`–`09`, 8 × 22 matrix | same, plus effect `0A`, 6 × 22 matrix |

## Where to get model data without hardware

- **Synapse 4 module**: `apps.razer.com/synapse/products/<PID in decimal>/mw/`.
  `index.html` → `main.*.js` — the device description (`productId`, `analogSpecs`,
  capability flags); it also holds the webpack chunk map. Chunk `3159` is the
  command table, `rzDevice25` builds packets, `obmEngineKeyboard` shows which
  commands are called for the model. A protocol reference; do not copy the code.
- **Firmware Updater** from the Razer site — a .NET program, unpacks with 7-Zip.
  `DeviceUpdater.resources` holds the parameters (`PID`, `BLPID`, `DevFWVer`) and
  the image: lines `DevFWLine0..N` are Intel HEX records. If the image is
  unencrypted, it can be analyzed in Ghidra. Initialized data is usually compressed
  (Keil): it has to be unpacked using the scatter-load table, otherwise
  descriptors and pointers are not visible.
- **OpenRazer** (`driver/razerkbd_driver.c`, `daemon/.../hardware/keyboards.py`):
  PID, interface and transaction id, effect set, lighting matrix size.
