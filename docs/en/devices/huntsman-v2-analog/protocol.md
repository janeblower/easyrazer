# Huntsman V2 Analog line protocol

[Русская версия](../../../ru/devices/huntsman-v2-analog/protocol.md)

Verified on a live device (`1532:0266`, firmware 1.06 — the same one that was analyzed) or read from
the Synapse 4 and firmware 1.06 code ([firmware.md](firmware.md)). In the "Firmware commands"
section, items are unverified by default; in the other sections, unverified items are marked.

## Models

| Model | VID:PID | Keys | Firmware | On hardware |
|---|---|---|---|---|
| Huntsman V2 Analog | `1532:0266` | full size, 104 | 1.06.00 | checked |
| Huntsman Mini Analog | `1532:0282` | 60%, 61 | 1.03.00 | not checked |

The sections below are about the V2 Analog; the Mini Analog differs only as described
in [its section](#huntsman-mini-analog).

## HID interfaces

| Interface | Usage page / usage | Purpose |
|---|---|---|
| MI_00 | 0x01 / 0x06 | Regular keyboard (boot), the OS grabs it exclusively |
| MI_00 Col02 | 0x01 / 0x04 | Gamepad mode (521-byte output report) |
| MI_00 Col03 | 0x0C / 0x01 | Consumer / system control |
| MI_01 Col01 | 0x01 / 0x06 | NKRO keyboard |
| MI_01 Col04 | vendor | Razer codes: `04 <code>…` — held, `04 00` — released; service `11` codes arrive in hardware mode too |
| MI_01 Col06 | vendor | Depth stream in driver mode, report `07` |
| MI_02 | 0x01 / 0x02 | Mouse (macros; volume wheel in driver mode) |
| **MI_03** | 0x0C / 0x01 | **Control**: 91-byte feature report |
| MI_04 Col01 | 0x59 / 0x01 | HID LampArray (Windows Dynamic Lighting) |

## Control packet (MI_03)

The packet format is shared across models — [see the overview](../README.md#control-packet).
On the V2, the firmware accepts only packets exactly 90 bytes long and executes the command
immediately, in the `SET_REPORT` handler; the response is ready for the next `GetFeature`. It does
not check the CRC of incoming packets. It never returns status `01` busy; a 5–20 ms pause and a
retry are only needed in case of a failure.

## Mode and profiles

- `00:84` / `00:04` — mode: `00` hardware (the keyboard types by itself), `03` driver
  (regular key presses are not sent, the depth stream is sent instead). After power-on — `00`.
  ⚠️ `01` — the device disappears and enumerates as `1532:110E`; it comes back after
  reconnecting.
- `00:81` firmware, `00:82` serial number.
- Esc + Caps Lock + Space while plugging in toggles demo
  mode (persisted): a wave that reacts to key press depth; commands `03` and `0F`
  have no effect in it ([firmware.md](firmware.md#device-modes)).
- `05:84` active profile (`01`), `05:81` list (`01 01`), `05:8A` → `05` (maximum number of profiles).
- **Profile 0 is a working copy in RAM**: on power-on it is filled from the saved profile,
  writing to it does not touch flash, and it is lost after reconnecting. `02:12` to profile 0
  with layer 1 changes only Hypershift; writing Normal in hardware mode did not touch Hypershift
  of the same key (Menu, verified).
- **A mode change (`00:04`) also overwrites profile 0 from the active one**: after
  switching driver ↔ hardware, temporary assignments and the effect are gone (verified).
  Temporary changes must be sent again after RT is turned on and off.
- **`05:04 id` and Fn+Menu also overwrite profile 0** from the new active one; the active
  profile is saved to flash and survives reconnecting (verified). There is no profile change
  event, only `05:84`; an Fn press shows up in MI_01 Col04 as `04 01` / `04 00`.
- The Hypershift layer of profile 0 is written separately (`02:0D 00 fwID 01 …`), Normal and flash
  are not touched: Menu → `02 00 73` turns Fn+Menu into F24 without a profile change (verified).
- A free service code (`11 20`) types nothing in hardware mode and arrives in
  MI_01 Col04 as a set of held codes: `04 01` (Fn), `04 01 20`, `04 01`, `04 00` (verified).
- `11 21` — the Snap Tap toggle code in EasyRazer, written the same way.
- An inactive profile can be read in full: `02:8F`, `02:92`, `0F:82` with its number
  (thresholds of all keys — ~2.5 s).

## Actuation

The V2 has no separate actuation and RT commands; the threshold is stored in the key assignment:

- `02:92` get `[profile, fwID, layer]` (layer 0 — Normal, 1 — Hypershift), size 80.
- Response: `[profile, fwID, layer, thrL, thrH, fnId, fnSize, fnData…]`.
  Example for key A: `01 1F 00 00 00 02 02 00 04` — fnId `02` key, `00 04` modifier + HID usage.
- `02:12` set — same format. In profile ≥1 it writes to flash.
- `mm = thr/255·2.1 + 1.5`.
- `thrL` — the actuation point, the firmware clamps it to `15..250`. `thrH` — the release
  point: no less than `5` and no more than `thrL − 10`. So `0/0` gives actuation at
  15 and release at 5, and `0/230` gives the same. In hardware mode the actuation point is
  never closer than 1.62 mm or farther than 3.56 mm. Verified: with `thrL = 250` the key
  triggers only almost at the bottom.
- Synapse writes `0/0` to all keys and determines actuation itself in driver mode.
- **In driver mode profile 0 accepts only thresholds**: the firmware silently drops an fnId
  change (response ok). In hardware mode everything is written to profile 0. Writing fnId to a
  flash profile in driver mode succeeds, but it is not copied to profile 0 (verified).
- `02:0D` / `02:8D` — the same assignment without thresholds: `[profile, fwID, layer, fnId,
  fnSize, fnData…]`.

fnId — per the firmware key press handler. `02:0D` and `02:0F` check the fnId and the data
length and respond `03` on error; `02:12` writes the assignment without checking.

| fnId | Data | Action |
|---|---|---|
| `00` | any | disabled |
| `01` | `btn` | mouse: `01`–`08` buttons, `09` / `0A` wheel up / down (verified `03`, `04` back, `05` forward, `0A`) |
| `02` | `mod usage` | key |
| `03` | `id16 n` | macro n times; a press during playback is ignored |
| `04` | `id16` | macro in a loop while the key is held; after release the current pass finishes |
| `05` | `id16` | macro in a loop until the next press, also finishes the pass |
| `06` | 1, 2 or 5 bytes depending on `data[0]` | accepted, does nothing on press |
| `07` | `mode n` | profile: `01` / `02` next / previous, `04` / `05` the same, cycling, `03 n` — profile n |
| `08` | 1 byte | accepted, does nothing on press |
| `09` | `code` | system control: `81` power, `82` sleep, `83` wake; factory Fn+Pause — `82` |
| `0A` | `usage16` | consumer (media), BE: `00 E9` — volume up (verified) |
| `0B` | `btn` | mouse button double click (verified) |
| `0C` | `on` | Hypershift while the key is held |
| `0D` | `mod usage rate16` | key turbo (verified `00 04 00 32`) |
| `0E` | `btn rate16` | mouse button turbo |
| `0F` | `id16` | step macro: each press plays the next 2 events |
| `11` | `fn` | service, per factory assignments: `01` Fn, `03` game mode (Fn+F10), `04` Fn+F9 — the M indicator blinks for ~1 s, `09` / `08` brightness − / + (Fn+F11 / F12); the rest — Razer code to the host |

`10` and `12`+ fail the check. In driver mode the firmware itself handles only
`0C` (Hypershift) and `11 01` (Fn) from the Hypershift layer; the host does the rest. Verified for
writing to profile 0: `00`, `02`, `03`, `04`, `05`, `0A`, `0F`. Factory Fn+Menu is `07 04`, next
profile, cycling. The firmware has no on-the-fly macro recording: Fn+F9 only blinks M.

`fwID` — AT-101 positional codes (`~`=1, `Q`=17, `A`=31, Space=61), table —
`crates/core/src/keymap.rs`.

Not supported (`05`): `02:99`, `02:9A` (V3 multi-actuation and RT), `02:9F`,
`02:A0`, `02:A7`–`02:AA`. How they work on the V3 Pro — in
[its protocol](../huntsman-v3-analog/protocol.md#actuation).

## Depth stream (driver mode)

- MI_01 Col06, report `07 (fwID depth)… 00`; an empty report — everything released.
- ~1 kHz, only on change, up to 11 keys per report, noise ±1–2. Depth is the average of
  8 of the last 10 samples, excluding the maximum and minimum.
- Depth 0..255 = 1.5..3.6 mm — the same scale as the threshold; 12 units = 0.1 mm.
- Release hysteresis of ≥ 12 units is needed, otherwise a key near the threshold chatters.
- `SendInput` gives no autorepeat — the host repeats by itself.
- Fn layer and media are on the host: Col04 `04 52..55` — four media keys, `04 01` — Fn.
  Fn itself also goes into the depth stream under fwID 59 (`RIGHT_GUI`); the other
  Fn-layer keys (F9, F10, Menu) come as regular report `07` pairs.
- The volume wheel arrives as a mouse wheel (MI_02, 8 bytes, wheel — byte 3).
  Per the firmware: the encoder is polled at `0x08017DB4` on TIM6 and looks only at the
  mode; it has no assignment or settings — it cannot be switched to consumer.
  Synapse remaps it to VolumeUp/Down, probably with a filter driver.
- On the host the wheel can be told apart via Raw Input: by the time `WH_MOUSE_LL` is called
  for a wheel click, its `WM_INPUT` is already in the thread queue (verified), so the hook knows
  the device and can swallow the click.
- `02:2A` (report mode) is not supported (`05`); in hardware mode there is no stream.

Synapse on the V2 does RT and Snap Tap on the host: `profile.appEngine = {mappings,
actuations[{keyId, make, break}], rapidTriggers{fwID: {makeSensitivity,
breakSensitivity}}}` is passed to `mapping_engine.dll` (report `07`, `SendInput`,
optionally a filter driver). There is no hardware RT.

## Lighting

### Effects `0F:02`

Arguments `profile led effect…`, `led` = `05`. `profile` `00` — working copy in RAM,
`01`–`05` — a profile in flash (after reconnecting, the effect of the active
profile returns). Read with `0F:82 profile 05`.

| Effect | Bytes after `profile 05` |
|---|---|
| Off | `00 00 00 00` |
| Static | `01 00 00 01 RGB` |
| Breathing 1 / 2 / random | `02 01 00 01 RGB` / `02 02 00 02 RGB RGB` / `02 00 00 00` |
| Spectrum | `03 00 00 00` |
| Wave | `04 dir speed 00`, dir `01`/`02`, the lower the speed, the faster |
| Reactive: Key | `05 00 speed 01 RGB` (speed does not work) |
| Reactive: Ripple | `06 00 speed 01 RGB` |
| Starlight | `07 00 speed 00` / `… 01 RGB` / `… 02 RGB RGB`, speed 1–3 |
| Custom layout | `08 00 00 00` |
| Fire | `09 00 00 00` — not in Synapse and not in the `0F:81` list (verified). No parameters: the firmware does not read the remaining bytes, speed and palette are fixed |

Brightness — `0F:04 profile 05 VV`, read with `0F:84`. Two colors do not work for reactive.

### Custom layout `0F:03`

- `00 led row start stop RGB×n`, size `5+3n`, `led` — `00` or `05` (the firmware does not read
  the leading `00` byte); 8×23 grid (rows 0–7, columns 0–22).
  Display — `0F:02 00 05 08 00 00 00`.
- **RAM only**: the firmware does not save effect `08` to profile ≥ `01`,
  although it responds ok.
- Row 0 — the F row (media buttons — one cell 18, the wheel — 21), 1–5 — keys,
  6 — case edge lighting, 7 — wrist rest. Key columns — `crates/core/src/layout.rs`.

### LampArray and Dynamic Lighting

- MI_04 Col01: 148 lamps (107 keys, 40 accent, 1 logo), 30 frames/s. Per the
  firmware, other layouts have 149 or 153 lamps.
  Feature reports: `1` attributes, `2`/`3` lamp, `4` multi-update, `5` range-update,
  `6` AutonomousMode.
- Writing lamps from the host (reports `4`, `5`) switches the effect to `08` "Custom layout".
  After `0F:10 01` the firmware ignores these writes (verified).
- **`AutonomousMode = 0`** — the host controls the lamps, `0F:02` has no effect.
  `0 → 1` turns on the default effect from settings (spectrum, `03`), not the profile
  effect — the effect has to be sent again.
- Feature report `7` on the same interface is a second entry point to the control channel:
  a packet in the same format goes to the same dispatcher (not verified on hardware).
- DL with a static color writes once; with animation it sends frames continuously.
- Enabling DL: `HKCU\Software\Microsoft\Lighting\AmbientLightingEnabled` (DWORD),
  the service applies the change immediately.

## Macros

A macro body is stored in shared storage under its own id and is not tied to keys;
a key references it with a `02:12` assignment with fnId `03`–`05`, `0F`. The firmware
plays macros only in hardware mode.

- `06:08` `id16 size32` — allocate space. `06:09` `id16 offset32 len data…` —
  data, up to 72 bytes per packet. Read with `06:89` in the same format. The firmware buffers
  `06:09` and writes to flash 100 ms after the last packet, so
  `06:89` right after a write may return old data. `06:0D` (name in
  Synapse) does not exist on the V2; we keep names on the host.
- **The body is written only to flash**, there is no RAM variant; only the binding
  (profile 0) can be in RAM. Storage — internal flash `0x08090000`–`0x080FFFFF`;
  the firmware reads events directly from there during playback and only from this
  range ([firmware.md](firmware.md)).
- `06:80` count, `06:81` / `06:8B` id list, `06:88 id16` size, `06:03 id16`
  delete.
- `06:8E` memory: `FFFF, total32, maxFree32, 0` — usable size of all blocks and
  the largest free block, both computed on connect. On an empty keyboard
  `6FFF0` and `6FA78` (457,336 bytes).
- `06:86` recomputes on every request: `count16, 0000, maxFree32, free32` —
  the number of macros, the largest free block and all free space. Verified:
  after writing a 4-byte macro, `06:86` immediately showed `6FA60`, `06:8E` — the old
  `6FA78`. More convenient for the UI than `06:8E`.
- A macro of `d` bytes takes a block of `8 + ceil8(d + 6)`: 8 bytes — block tags, 6 —
  body header. Verified with d = 2, 3, 12, 73.
- Deletion frees the block immediately and merges it with adjacent free ones; the space can be
  reused immediately. Only `06:8E` shows old values until reconnecting.
- The macro index is 100 entries of 14 bytes, it grows by 50 on overflow;
  there is no separate limit on the number of macros.
- `06:0A 00 00 00 02` — format: deletes all macros, response `00 00 02 02`,
  free space comes back immediately (verified). `06:0A` with the arguments that
  Synapse sends responds `05`.
- Up to 20 macros play at once. The default delay between events —
  `06:07` / `06:87` (`01`), in polling intervals.
- ⚠️ Each flash write rewrites a whole 4 KB page: frequent writes
  wear out the flash.

Events, 2–5 bytes:

| Bytes | Event |
|---|---|
| `01 usage` / `02 usage` | key pressed / released (HID usage) |
| `03`/`04`, `05`/`06` | power / media pressed / released |
| `08 mask` | mouse buttons |
| `0A 01` / `0A FF` | wheel up / down |
| `11 t8`, `12 t16`, `13 t24`, `14 t32` | delay, BE, in polling intervals (at 1000 Hz — ms) |
| `15 x16 y16` | cursor movement |

Example: `01 04 11 32 02 04 01 05 11 32 02 05` types `ab`.

## Firmware commands

The full list from the firmware 1.06 handlers ([firmware.md](firmware.md)). Unless
"verified" is stated, it was not tested on hardware. ⚠️ — writes to flash what
Synapse does not touch; do not send.

### `00` — device

| cmd | Arguments and meaning |
|---|---|
| `00:81` / `00:87` | firmware version, 2 / 4 bytes: `01 06`, `01 06 00 00` (verified) |
| `00:02` / `00:82` | serial number, 22 bytes; ⚠️ `02` overwrites it |
| `00:83` | always `00 60` |
| `00:04` / `00:84` | mode: `00` hardware, `02` factory, `03` driver, `01` — reboot into the bootloader (verified `00`, `01`, `03`). Mode `08` exists in the code but does not work in this version |
| `00:05` / `00:85` | polling interval, ms: allowed `1`–`5`, `8`, `10` (`1` — 1000 Hz); anything else — `03` (verified `02`, `06`). Saved to flash |
| `00:06` / `00:86` | two bytes in config |
| `00:0B` | `00` — reboot after writing all data, the keyboard comes back in ~2.5 s (verified); `01 00` — settings to factory defaults, ⚠️ `01 01` — config as well (calibration, serial number) |
| `00:3D` / `00:BD` | `idx(0/1)` + 14 bytes in config. `00:BD 01` → `07E7 0005 0016 000A 0029 001E` — probably the manufacturing date (2023-05-22 10:41:30), `00` — zeros |
| `00:B5 03` / `00:B9` | byte from `0x20000000` |

### `02` — keys

`profile` `00` — working copy in RAM, `01`–`05` — flash; a write to the active
profile is copied to `00` as well.

| cmd | Arguments |
|---|---|
| `02:02` / `02:82` | 4 bytes in settings |
| `02:03` / `02:83` | `profile fwID fnId size data` — assignment without a layer |
| `02:0D` / `02:8D` | `profile fwID layer fnId size data` (verified) |
| `02:12` / `02:92` | `profile fwID layer thrL thrH fnId size data` (verified) |
| `02:11` / `02:91` | thresholds only: `profile fwID _ thrL thrH` |
| `02:0F` | batch: `profile layer`, then 8-byte entries `fwID fnId size data[5]` |
| `02:8F` | `profile layer startFwID` → up to 9 entries of 8 bytes |
| `02:13` / `02:93` | `first last` + 2 bytes per key (of 3 in settings), shared by all profiles; meaning unknown |
| `02:07` / `02:87` | `profile flag` — profile flag, resets held keys on change; meaning unknown |

### `03` — indicators

LED: `07` — M indicator, `08` — game mode indicator (crosshair), both single-color;
`18` — RGB. List — `03:89` → `03 07 01 08 01 18 03`. Verified: `08` works in
any mode; `07` obeys `03:00`/`03:02`/`03:04` only in driver mode, in hardware
mode M is lit only by Fn+F9.

| cmd | Arguments |
|---|---|
| `03:00` / `03:80` | `00 led on` — on/off |
| `03:01` / `03:81` | `00 18 RGB` — color |
| `03:02` / `03:82` | `00 07 mode`: `0` — steady, `1` — blinking |
| `03:04` / `03:84` | `00 07 on off` — blink durations, in 100 ms units; default `05 05` |

### `05` — profiles

`05:80` count, `05:81` list, `05:84` active, `05:8A` → `05` maximum,
`05:02` create, `05:03` delete, `05:04 id` make active, `05:08` / `05:88`
`id off16 len16 data` — profile name in UTF-16BE (profile 1 has `default`; read and write verified),
`05:0B` — purpose not determined.

- `05:02 id` (size 1) creates a profile but does not clear its page: it keeps whatever
  was there before deletion, including the name and Synapse data. After creation the profile
  must be written in full (verified).
- `05:03 id` only removes the profile from the list, the data remains. Deleting the active one
  leaves `05:84` on the deleted id — switch to another one first (verified).

### `06` — macros

In addition to what is described in ["Macros"](#macros): `06:02 id` — create an index entry, `06:04` /
`06:84` `id n` + `n` words, `06:07` / `06:87` — delay between events,
`06:0A` / `06:8A` `id x n` (`n = 2` — format), `06:0C` / `06:8C` `id off16 total16 data` —
macro blob (probably the name), `06:8B off` — id list, paged. `06:85`,
`06:8D` → `05`.

### `0F` — lighting

| cmd | Arguments |
|---|---|
| `0F:80` | `05 19 03 0B 17` — capabilities; `17` = 23 columns |
| `0F:81` | `led` (`00`/`05`) → effects `01`–`08` |
| `0F:02` / `0F:82` | `profile led effect p1 p2 n RGB×n`, `n ≤ 10` (verified) |
| `0F:03` | `00 led row start stop RGB×n` (verified) |
| `0F:04` / `0F:84` | `profile led VV` — brightness (verified) |
| `0F:06` / `0F:86` | `profile flag` |
| `0F:10` / `0F:90` | `01` — the firmware ignores lamp writes via LampArray; effect `08` is then replaced by the default effect (verified). Saved to flash |

### `FE` / `F0` — factory mode

Work only in device mode `02` (`00:04 02`), otherwise `05`. `A7`, `A8`, `28`
require calibration to be enabled (`FE:20 01`), otherwise `03`. Everything except `FE:28` and
`FE:83` is verified; mode `02` is reset by `00:04 00` or by reconnecting.

| cmd | Arguments |
|---|---|
| `FE:20` / `FE:A0` | enable / read calibration mode |
| `FE:A1` | `80` if calibration is enabled |
| `FE:A7` | entries `fwID 00 00` → raw 16-bit sensor value of each key, BE. The value drops on press: Q at rest ≈ `0E53`, bottomed out ≈ `0240`; noise ~20 |
| `FE:A8` | `fwID 00 00 00 00` → calibration pair, 2 × u16 BE: Q `0448 0D0F`, A `03E6 0D0F`. Probably the bottom of travel (depth 255) and the start (depth 0, shared by all keys). More than one entry per request — `03` |
| `FE:28` | ⚠️ writes calibration pairs and saves to flash |
| `FE:83` | `ch < 8` — sensor multiplexer test |

## What else Synapse can do for this model

From the Synapse 4 screens — a reference for the roadmap:

- **Actuation**: actuation, RT (separate up/down), Snap Tap (A/D pair,
  Fn+LShift toggle).
- **Customize**: Standard / Hypershift layers; game mode (block Win, Menu,
  Alt+Tab, Alt+F4, "in game only"); WASD → gamepad stick, Q/E → triggers;
  typing mode (Fn+Ctrl+Esc → default profile).
- **Lighting**: brightness, turning off when the display is off / idle.
- **Macros**: delay, key, mouse, nested macro, launch program,
  command, text, loop.

## Synapse commands the V2 does not have

Synapse tables contain commands for other models. The V2 firmware has no handlers for them,
the response is `05`: `02:1B` Mod-Tap, `02:1E`, `02:27` / `02:2C` Snap Tap, `02:28`,
`02:29`, `02:2D`, `02:2E`, `02:31`, `02:33`–`02:37`, `02:B2` keymaps, `02:A4`,
`02:A8`, `02:9C`, `FE:2D`, `FE:AD`, `FE:AE`, `00:08`, `00:5A`, `00:5D`, `05:05`–`05:07`,
`05:09`, `05:0A`, `05:0C`. Snap Tap, game mode and RT on the V2 are host-only; the firmware only toggles
the game mode indicator (`11 03`).

## Not verified on hardware

Everything else in this file marked "verified" has been confirmed on the device. Here is
what is known only from the firmware.

Needs key presses:

- macro playback modes: `03` ignores a press during playback, `04` and
  `05` finish the pass, `0F` plays 2 events at a time; up to 20 macros at once;
- macro events: mouse `08` (EasyRazer writes the mask as in a HID mouse report: `01` left,
  `02` right, `04` middle, `08` back, `10` forward), wheel `0A`, cursor `15`, media `05`/`06`;
- fnId `0E` (mouse turbo), `0C` (Hypershift), `11 03` (game mode), `07` (needs ≥ 2
  profiles);
- release point `thrH` and the lower actuation limit of 15;
- Hypershift `thrL`: a key with Fn triggers at the Hypershift point (W at 3.4 mm, Fn+W slowly), not at the regular one;
- `11 21` in hardware mode: Fn+Z on Snap Tap types nothing, MI_01 Col04 receives `04 01 21`;
- converting the raw sensor value to depth: `FE:A7` against the depth stream for a single
  press;
- report rate with `00:05` ≠ `01`;
- demo mode on a charger, without a computer.

Needs work on `razer-probe`:

- feature report `7` on MI_04 as an entry point to the control channel;
- the `06:09` flash write delay — write and read `06:89` in a single process.

Meaning not determined: `00:06`, `00:83`, the `00:3D` / `00:BD 00` block, `02:07`, `02:13`,
`05:0B`, `0F:06`, fnId `06` and `08`.

Deliberately not tested: `00:02`, `00:0B 01 xx`, `FE:28`, `FE:83`, mode `01`, fnId `09` and
events `03`/`04` (power, sleep). Cannot be tested: layouts with 149 and 153 lamps
(another keyboard is needed), the bootloader (not in the image).

## Linux

HID — `/dev/hidraw*` + a udev rule (conflicts with openrazer, if installed);
capture — `evdev` + `EVIOCGRAB`, emulation — `uinput`. Linux has no LampArray support —
lighting only via `0F:xx`.

## Huntsman Mini Analog

Per the firmware 1.03.00 the protocol is that of the V2 Analog, with these differences.

- PID `0282`.
- 60% board: 68 keys in the firmware table, 61 of them in ANSI, the same set as the
  [V3 Pro Mini](../huntsman-v3-analog/protocol.md#huntsman-v3-pro-mini). No F-row,
  arrows, navigation block, numpad, media buttons or dial.
- Lighting matrix size is unknown, so the custom effect is not offered.
