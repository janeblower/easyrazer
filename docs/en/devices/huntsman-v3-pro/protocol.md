# Huntsman V3 Pro protocol

Not verified on hardware. Sources: the Synapse 4 module for `678` (`1532:02A6`),
firmware 1.03.00 ([firmware.md](firmware.md)) and the OpenRazer driver. Where
there is a single source, it is noted. Packet format — [in the overview](../README.md#control-packet).

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
  ([details](firmware.md#mode-switch)). Snap Tap (`02:21`) is disabled in driver mode.
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
(Hanja / Hangeul): the table in `crates/core/src/keymap.rs` fits. The geometry
(`layout.rs`) needs checking against photos: the wheel block in the top right
corner differs between the models.

## Not verified

- Everything in this file is without hardware.
- First on hardware: `05:84` before and after `00:04 03` with startup slot 2 — it
  should become `01`; whether that survives a replug. And whether `02:92` after
  `00:04` reads what was written to profile 0.
- Reports `04`, `05`, `07` on MI_01 and interface MI_05.
- fwIDs of the wheel and media buttons, and exact key geometry.
- Effect `0A`, flag `02:1F`, the meaning of bytes `+1`, `+2` in threshold entries.
