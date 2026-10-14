# Huntsman V3 analog line protocol

[Русская версия](../../../ru/devices/huntsman-v3-analog/protocol.md)

Checked on hardware on the V3 Pro Mini only (firmware 2.00, [#45](https://github.com/janeblower/easyrazer/issues/45)), see its
section. Sources: the Synapse 4 module for `678` (`1532:02A6`),
firmware 1.03.00 ([firmware.md](firmware.md)) and the OpenRazer driver. Where
there is a single source, it is noted. Packet format — [in the overview](../README.md#control-packet).

## Models

| Model | VID:PID | Keys | Firmware | On hardware |
|---|---|---|---|---|
| Huntsman V3 Pro | `1532:02A6` | full size, 104 | 1.03.00 | not checked |
| Huntsman V3 Pro TKL | `1532:02A7` | TKL without PrtSc / ScrLk / Pause, 84 | 1.04.00 | not checked |
| Huntsman V3 Pro TKL 8KHz | `1532:02D0` | as the V3 Pro TKL | 1.04.00 | not checked |
| Huntsman V3 Pro Mini | `1532:02B0` | 60%, 61 | 2.01.00 | checked on 2.00 |
| Huntsman V3 TKL 8KHz | `1532:02E5` | TKL, 87 | 1.02.00 | not checked |
| Huntsman V3 Pro 8KHz | `1532:02CF` | as the V3 Pro | 1.03.00 | not checked |
| Huntsman Signature Edition | `1532:02D8` | as the V3 Pro TKL | 1.02.00 | not checked |
| Huntsman V3 Pro Low-profile TKL 8KHz | `1532:02E6` | as the V3 Pro TKL | 1.01.01 | not checked |

The sections below are about the V3 Pro; the other models differ only as described in
their sections.

## HID interfaces

Per the firmware descriptors.

| Interface | Purpose |
|---|---|
| MI_00 | Standard keyboard (boot); no gamepad or consumer, unlike V2 |
| MI_01 | Report `01` NKRO, `02` consumer, `03` system control; vendor `04`, `05`, `07`, `0B`, 47 bytes each. `0B` is the depth stream |
| MI_02 | Mouse: 5 buttons, wheel, X/Y |
| **MI_03** | **Control**: feature report without ID, 90 bytes |
| MI_04 | HID LampArray, feature ID `01`–`06` |
| MI_05 | Vendor, 64 bytes in/out, purpose unknown |

## Device

- Control channel is interface 3, transaction id `1F` (same in Synapse and
  OpenRazer).
- Mode `00:04` / `00:84`: `00` hardware, `03` driver, `01` is a flag, probably
  bootloader entry, as on V2. On switching to driver mode the firmware makes
  profile 1 active and saves that to flash if another one was active
  ([details](firmware.md#mode-switch)); on the V3 Pro Mini `05:84` goes from `02` to `01`
  and stays `01` after leaving driver mode and after a replug. Snap Tap (`02:21`) is
  disabled in driver mode.
- Profile `0` is RAM only for every command, as on V2.
- Profiles: `0` is the working copy in RAM, `1`–`6` are persistent. Synapse uses 5
  slots (`OBMSlots: 5`) and stores profile names in the firmware, including the first.

## Depth stream (driver mode)

MI_01, report `0B`: up to 15 entries `fwID depth:u16`, BE, the rest is zeros. Same
scale as the thresholds, `0..0xFFFF`; below `0x666` (0.1 mm) the firmware reports `0`.
The report is sent on change, a released key is removed from it. V2 uses report
`07` and a 1-byte depth.

## Actuation

The threshold is a u16 **big-endian**, 1638 = 0.1 mm, `65520` = 4.0 mm. That is how
Synapse computes it (`unit: 1638`, `min: 1638`, `max: 65520`, default `19656` = 1.2 mm,
`minBreak: 256`), and the firmware has steps of 1638: actuation 0.1…3.6 mm, RT 0.1…1.0 mm.

The first argument byte of `02:19`, `02:1A`, `02:99`, `02:9A`, `02:9C` is the **value
mode**: `00` means u16, otherwise 1 byte (the high byte of the u16, low byte `00`).
Synapse always sends `00`.

| Command | Arguments |
|---|---|
| `02:19` set actuation | `mode profile {fwID H:u16 L:u16}×≤15` |
| `02:99` get | `mode profile fwID×≤15`, response in the same format as set |
| `02:1A` set RT | `mode profile {fwID on up:u16 down:u16}×≤13` |
| `02:9A` get | `mode profile fwID×≤13`, response as for set |
| `02:20` / `02:A0` | continuous RT: `profile on` / `profile` → `profile on` |
| `02:9C` | `mode {fwID value:u16}…` — key sensor value (Synapse: Get Multi Analog Key ADC Data) |

- `fwID = FF` means all keys at once; `00` gives error `03`.
- H and L are as on V2: L is press, H is release (order in the packet: H, then L).
- In `02:1A`, zero up or down leaves the stored value unchanged. `on = 01` on at
  least one key enables RT for the whole profile.
- Profile `0` changes RAM only; `1`–`6` are written to the profile and, if it is
  active, applied immediately.

The old `02:12` / `02:92` (V2 format: `profile fwID layer thrL thrH fnId
fnSize fnData…`) work, but the threshold in them is the high byte of the u16, i.e.
`0..255` ↔ 0–4 mm in steps of ≈ 0.016 mm, not 1.5–3.6 mm as on V2.

## Snap Tap

`02:21` — `profile on fwID1 fwID2`, one key pair; `02:A1` — `profile` →
`profile on fwID1 fwID2`. Synapse calls it kill switch (`killSwitch`).

Per the firmware: pressing one key of the pair releases the other; when it is
released, the other one, if still held, is pressed again. Hardware mode only. `02:27`
(Snap Tap Ctrl from the Synapse table) has no handler.

## Key assignments

- `02:0D` / `02:8D` — as on V2: `profile fwID layer fnId fnSize fnData…`.
- `02:1E` set / `02:9E` get — in batches: `profile {fwID layer fnId fnSize data[5]}…`,
  9 bytes per key. Synapse reads assignments via `02:9E`, 8 keys at a time.
- `02:12` / `02:92` — assignment together with threshold, see above.

## Lighting

Class `0F` matches V2: `0F:02` effect (LED `05`; the firmware replaces `00` with
`05`), `0F:03` custom layout (`store led row first last RGB…`, LED `05` or `00`),
`0F:04` / `0F:84` brightness, `0F:82` effect. The custom layout matrix is 6 × 22
(rows 0–5, columns 0–21), on V2 it is 8 × 22; OpenRazer agrees.
Effects `00`–`0A`: one more than on V2 (`09` is fire), what `0A` is remains
unknown.

## Class `02` commands per the firmware

Set: `02`, `03`, `07`, `0D`, `0F`, `11`, `12`, `13`, `15`, `19`, `1A`, `1E`, `1F`,
`20`, `21`. Get: `82`, `83`, `87`, `8D`, `8F`, `91`, `92`, `93`, `95`, `99`, `9A`,
`9C`, `9E`, `9F`, `A0`, `A1`. Everything else returns `05`.

`02:1F` / `02:9F` — a single flag (Synapse: Analog Key Adjustment Mode Ctrl), what it
changes is unknown.

## Keys

Sensor matrix is 8 × 14, 111 keys. fwIDs are the same as on V2, except `152` / `153`
(Hanja / Hangeul): the table in `crates/core/src/keymap.rs` fits. Per photos the key
layout is the same as on V2 (ANSI, 104 keys, three buttons and a dial above the
numpad), so `layout.rs` fits too. Lighting differs: no case edge and wrist rest
rows, whether the buttons and the dial are lit is unknown.

## Not verified

- Everything not checked on the V3 Pro Mini.
- Whether `02:92` after `00:04` reads what was written to profile 0.
- Profile names (`05:88`): the app shows none on the V3 Pro Mini, the reply is not seen yet.
- Reports `04`, `05`, `07` on MI_01 and interface MI_05.
- fwIDs of the dial and the three buttons, and whether they are lit.
- Effect `0A`, flag `02:1F`, the meaning of bytes `+1`, `+2` in threshold entries.

## Huntsman V3 Pro TKL and TKL 8KHz

Per the firmware the protocol is that of the V3 Pro, with these differences.

- PID `02A7`; `02D0` for the 8 kHz version.
- Tenkeyless board: sensor matrix 7 × 14, 84 keys in ANSI. No numpad and no
  PrtSc / ScrLk / Pause.
- Lighting matrix size is unknown, so the custom effect is not offered.

## Huntsman V3 Pro Mini

Per the firmware the protocol is that of the V3 Pro, with these differences.

- PID `02B0`.
- 60% board: sensor matrix 5 × 14, 61 keys in ANSI. No F-row, arrows, navigation
  block, numpad, media buttons or dial.
- Lighting matrix size is unknown, so the custom effect is not offered.
- Checked on hardware, firmware 2.00: `02:92` / `02:12` thresholds, RT `02:1A`, Snap Tap
  `02:21` and lighting `0F:02` / `0F:04` on profile 0; the depth stream `0B` and the
  mode switch above. `05:81` lists six slots: `06 01 02 03 04 05 06`.

## Huntsman V3 TKL 8KHz

The Synapse module for `741` (`1532:02E5`) sends the same commands as the one for the V3
Pro TKL, with the same u16 threshold scale (`unit: 1638`, `max: 65520`). The firmware is
a separate code base, so this is less certain than for the models above.

- PID `02E5`.
- Full TKL: sensor matrix 7 × 15, 87 keys in ANSI, with PrtSc / ScrLk / Pause.
- No HID LampArray interface (`isWDLSupported: false` in Synapse); the other interfaces
  match the V3 Pro, control on interface 3, depth stream in report `0B`.
- Lighting matrix size is unknown, so the custom effect is not offered.

## Huntsman V3 Pro 8KHz

Per the firmware 1.03.00 and the Synapse module for `719` the protocol is that of the V3 Pro:
the same sensor matrix, interfaces and LampArray. PID `02CF`. Synapse allows thresholds up to
`65535` instead of `65520`.

## Huntsman Signature Edition

Per the firmware 1.02.00 and the Synapse module for `728` the protocol is that of the V3 Pro
TKL: the same 7 × 14 matrix, the interfaces and LampArray of the V3 Pro. PID `02D8`.

## Huntsman V3 Pro Low-profile TKL 8KHz

Firmware 1.01.01 is from the same code base as the V3 TKL 8KHz one; the Synapse module for
`742` sends the same commands as the V3 Pro TKL one.

- PID `02E6`.
- Low-profile switches with 2.8 mm of travel: the same u16 scale, 1638 = 0.1 mm, but
  Synapse stops at `45864` = 2.8 mm.
- Keys of the V3 Pro TKL: no numpad and no PrtSc / ScrLk / Pause.
- No HID LampArray interface (`isWDLSupported: false`), as on the V3 TKL 8KHz.
- Lighting matrix size is unknown, so the custom effect is not offered.
