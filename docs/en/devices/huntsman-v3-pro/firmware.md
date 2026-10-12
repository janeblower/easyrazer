# Huntsman V3 Pro firmware

Analysis of the official Firmware Updater `v1.03.00_r1` (firmware `1.03.00`, the
same one Synapse calls `1.3.0.0`). This is a reference for the protocol: firmware
code is not carried over into the project. Addresses are for this version.
Conclusions without direct verification are marked "probably". Commands are in
[protocol.md](protocol.md).

## Image

The updater is the same as for V2 (.NET, unpacks with 7-Zip), parameters are in
`DeviceUpdater.resources`:

| Key | Value |
|---|---|
| `VID` / `PID` | `1532` / `02A6` |
| `BLPID` / `BLVER` | `110E` / `2.0` — bootloader, as on V2 |
| `DevFWVer` / `FlashFWVer` | `1.03.00` / `1.03.00` |
| `FeatureReportLen` | `91` |
| `RYFWFileName` | `Talia_T1_7203_App_FW_20240426_v1.3.0_0D55D9FF.enc` |

There are two images:

- `DevFWLine0..8292` — Intel HEX of the main MCU, unencrypted: `0x20000000`–`0x200205FF`,
  132 608 bytes.
- `FlashFWSector0..36` — 37 408 bytes, encrypted (entropy ~6.2 bits/byte),
  flashed via `Ry_Online_Update_Dll`. Probably a second chip; not needed for the
  protocol. `update/update_config.ini` refers to PID `02B0` and a different
  `.enc` — looks like a leftover from the V3 Pro Mini.

Analysis in Ghidra 12 headless: `ARM:LE:32:Cortex`, raw binary with base
`0x20000000`, plus uninitialized blocks SRAMX `0x04000000`, RAM
`0x20020600`, peripherals `0x40000000` and SCS `0xE0000000`.

## Microcontroller

NXP LPC55xx, Cortex-M33; probably LPC55S6x with 640 KB flash:

- strings from NXP MCUXpresso SDK 2.8.2 (`fsl_gpio.c`, `fsl_spi.h`, `fsl_utick.c`),
  nine FLEXCOMMs, CTIMER, UTICK, WWDT, GINT;
- startup writes `MSPLIM`, a register that exists only in ARMv8-M;
- flash page is 512 bytes; the firmware uses flash up to `0x87FFF`, which is more
  than 512 KB;
- stack is `0x04008000`, the end of the 32 KB SRAMX;
- the code is linked to RAM, not flash: probably the bootloader copies it there.

## Startup

- Reset (`0x20000214`) → `MSPLIM = 0x04006000` → `SystemInit` (`0x20004084`) →
  `__main` (`0x20000200`): stack, scatter-load (`0x20000544`) → `main`
  (`0x2000EF3C`).
- Scatter-load table is at `0x2001E958`, 4 entries `{src, dst, size, handler}`:
  `0x2001EA00` → `0x20020800` (`0x5C00`) and `0x2001FE00` → `0x04000000` (`0xA00`)
  are decompressed, `0x20026400` (`0x74A4`) and `0x04000A00` (`0x27D0`) are zeroed.
- Decompressor `0x20000568` is Keil LZ. Token `t`, then:
  - `n = t & 3`, if `0` — the next byte; then `n − 1` literals;
  - `len = t >> 4`, if `0` — the next byte (it comes before the literals); `len = 0`
    means no match;
  - back offset is the next byte plus `(t & 0x0C) << 6`, and when
    `t & 0x0C = 0x0C` — plus one more byte `<< 8` instead; `len + 2` bytes are copied.

After decompression the USB descriptors, the profile pointer and other data
become visible, which are not in the image in plain form.

## USB

Device descriptor is `0x20026040`, configuration is `0x20025F80`, 6 HID interfaces;
report descriptor table is `0x2001B340`.

| Interface | EP | Descriptor | What |
|---|---|---|---|
| 0 | `81`, 8 | `0x200260C0` | boot keyboard |
| 1 | `82`, 48 | `0x04000740` | ID `01` NKRO, `02` consumer, `03` system control, `04` `05` `07` `0B` — vendor, 47 bytes each |
| 2 | `83`, 8 | `0x20026140` | mouse: 5 buttons, wheel, 16-bit X/Y |
| 3 | `03` out | `0x04000540` | control: feature 90 bytes without ID |
| 4 | `84`, 2 | `0x040005C0` | HID LampArray, feature ID `01`–`06` |
| 5 | `85` / `05`, 64 | `0x04000880` | vendor, 64 bytes both ways, purpose unknown |

Unlike V2, interface 0 has no gamepad and consumer collections.

## Control channel

Packet is at `0x2002AF38`: status `+0`, size `+5` (`0x2002AF3D`), class `+6`, id `+7`,
args from `0x2002AF40`. The response is written to the same buffer.

| Address | What |
|---|---|
| `0x200125A0` | class dispatcher |
| `0x2001189C` | `00` |
| `0x2000FB78` | `02` — keys, actuation, RT, Snap Tap |
| `0x20011BDC` | `03` |
| `0x20012654` | `05` |
| `0x20011F74` | `06` |
| `0x20011338` | `0A`, `0F` — lighting |
| `0x200129A4` | other classes, including `FE` |

## Profiles

A profile is `0x11B3` bytes. Profile 0 (working copy) is in RAM, `0x200269A8`.
Profiles 1–6 are in flash, `0x49E00 + n·0x11B3`: commands read and write them right
there (see ["Flash writes"](#flash-writes)). Commands reject a number greater
than 6. Active profile is `0x20029002` (settings `+2`).

| Offset | Field |
|---|---|
| `+0x979` | actuation level (see below) |
| `+0x980` | thresholds: 135 × 7 bytes `fwID ? ? H:u16 L:u16` (LE in memory) |
| `+0xD31` | RT enabled |
| `+0xD32` | RT level |
| `+0xD33` | RT: 135 × 8 bytes `fwID on on up:u16 down:u16 01` |
| `+0x116B` | continuous RT (`02:20`) |
| `+0x116C` | Snap Tap: `on fwID1 fwID2` (`02:21`) |
| `+0x116F` | flag `02:07` |
| `+0x1172` | 4 bytes `02:15` |
| `+0x11B1` | CRC-16/CCITT-FALSE (init `FFFF`, polynomial `1021`) over `+0`…`+0x11B0` |

Working copies of the tables for the current profile: thresholds — `0x20027328`,
level — `0x20027321`; RT — `0x200276DB`, flag — `0x200276D9`, level — `0x200276DA`;
continuous RT — `0x20027B13`; Snap Tap — `0x20027B14`–`0x20027B16`. Device
mode (`00:04`) — `0x04000DE3`.

## Flash map

| Address | Contents |
|---|---|
| `0x49000` | config, `0x425` bytes; in RAM — `0x04002000`, serial at `+6` (`00:82`) |
| `0x49C00` | settings, `0x177` bytes; in RAM — `0x20029000`: `+2` active profile, `+5`–`+0x12` — probably a macro index |
| `0x49E00` | profiles 1–6, `0x11B3` bytes each |
| `0x53600` | 16 bytes, in RAM — `0x04000DA4` |
| `0x53800`–`0x543FF` | factory area: read by command `81` of the other-classes handler at a 32-bit offset |
| `0x54400`–`0x7F3FF` | probably macro storage: the player reads 5-byte events from here |

Reset (`0x2001A898`) erases `0x49C00`–`0x87FFF`: settings, profiles, macros and
whatever follows. Config is left untouched.

## Flash writes

- `0x2000C430(addr, src, n)` — range write: the 512-byte page is read into
  `0x04001000`, modified and reprogrammed (`0x2000C39C`), up to 3 retries.
  The 512-byte page is one more argument for LPC55xx.
- `0x2001A074` recomputes the profile CRC and writes it with the same function.
- There is no deferred saving like on V2: a command to profile 1–6 (`02:19`, `02:1A`,
  `02:20`, `02:21`, `02:07`, `02:15`) immediately erases at least two pages —
  data and CRC. A batch of 15 keys costs as much as a single key, so flash
  writes should go in large batches, and live changes to profile 0.
- Profile 0 stays in RAM: `02:12` goes to `0x20019BA0`, `02:1A` to `0x20019B24`
  with the "do not save" flag, `02:21` writes `0x20027B14` directly; none of them
  calls `0x2000C430`. Live changes work as on V2.
- Config and settings are saved with a delay, like the V2 profiles: `0x200161E8`
  sets a dirty bit in `0x2002AF28` (`1` settings, `2` config) and a countdown of
  `0x67` in `0x2002AF2A`; `0x200151C4` writes the page once it runs out.

## Mode switch

`00:04` (`0x2001189C`): switching from another mode to `03` with an active profile
other than 1 makes profile 1 active (`0x20029002 = 1`) and marks the settings for
saving, so `0x49C00` is erased about `0x67` ticks later. Once profile 1 is active,
further switches write nothing; leaving driver mode does not bring the old profile
back. So the profile the keyboard starts with without the app silently becomes 1, and
an app that sets another startup slot and enters driver mode again costs one erase
each time. Mode `01` sets only `0x2002686D` (probably a request to reboot into the
bootloader).

## Key matrix

- Sensor matrix is 8 × 14 (`0x2001D1D0`, fwID by position), 111 keys and one
  empty cell. Position is encoded in a byte: column in the high 4 bits, row + 1 in
  the low ones. Key state is 6 bytes, `0x04001288 + row·0x54 + column·6`.
- The fwID set is the same as in the V2 matrix, except `152` / `153` (Hanja / Hangeul):
  key names from `crates/core/src/keymap.rs` fit.
- Lighting is 6 × 22 (`0F:03` accepts rows 0–5 and columns 0–21), frame
  buffer is `0x20028C74`, 66 bytes per row. On V2 it is 8 × 22.

## Actuation

- Thresholds and RT are u16, 1638 = 0.1 mm. Actuation level table `0x2001B3DE`:
  `1638·{1, 4, 8, 12, …, 36}` (0.1, 0.4, 0.8 … 3.6 mm); RT — `0x2001E11E`:
  `1638·{1…10}` (0.1…1.0 mm).
- When writing all keys (`fwID = FF`) the firmware sets the profile "level" —
  the highest step number not exceeding L of the first key (for RT — down).
  What it is for is unknown; probably for indication.
- The old `02:12` / `02:92` work with the high byte of the u16.
- Driver mode (`03`), `0x20007BC4`: each key's depth is mapped to
  `0..0xFFFF`, below `0x666` (0.1 mm) counts as zero. Changed keys
  accumulate in report `0B` (`0x20027D88`): up to 15 entries `fwID depth:u16` (BE),
  a released key is removed from it. Sent on interface 1, 48 bytes.

## Snap Tap

Key event handling (`0x2000DA74`, before the press engine): if `02:21` is
enabled and the mode is not `03`, pressing a key of the pair releases the other,
releasing it presses the other again if it is held. Pair state is `0x200283CC`–
`0x200283CE` and `0x040019C8`–`0x040019CA`.

## Not analyzed

- Whether profile 0 is reloaded from profile 1 when driver mode makes it active, and on
  a switch back to hardware mode.

- Format of config, settings and macros; flash page endurance.
- Sensor polling, calibration and press decision.
- Reports `04`, `05`, `07` of interface 1 and interface 5.
- Bytes `+1`, `+2` of threshold entries, effect `0A`, flag `02:1F`.
- The second image (`.enc`).
