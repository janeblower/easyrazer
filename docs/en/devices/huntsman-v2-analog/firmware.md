# Huntsman V2 Analog line firmware

Analysis of the official Firmware Updater `v1.06.00` (firmware `1.06.00`). This is a
reference for the protocol: no firmware code is carried over into the project.
Addresses are for this version. Conclusions without direct verification are marked
"probably". Control channel commands are described in [protocol.md](protocol.md).

## Models

| Model | VID:PID | Keys | Firmware | On hardware |
|---|---|---|---|---|
| Huntsman V2 Analog | `1532:0266` | full size, 104 | 1.06.00 | checked |
| Huntsman Mini Analog | `1532:0282` | 60%, 61 | 1.03.00 | not checked |

The sections below are about the V2 Analog; the Mini Analog firmware is in
[its section](#huntsman-mini-analog).

## Image

The updater is a .NET program. The image is in `DeviceUpdater.resources` (the exe
unpacks with 7-Zip): the `DevFWLine0..N` strings are Intel HEX records in numeric
order. The same place holds parameters:

| Key | Value |
|---|---|
| `VID` / `PID` | `1532` / `0266` |
| `BLPID` / `BLVER` | `110E` / `2.0` — bootloader |
| `DevFWVer` / `FlashFWVer` | `1.06.00` / `1.02.00` |
| `FeatureReportLen` | `91` |

The image is not encrypted: `0x08007000`–`0x0801D043`, 90,180 bytes. Only the
initialized data section is compressed (see ["Startup"](#startup)).

The analysis was done in Ghidra 12 headless: `ARM:LE:32:Cortex`, raw binary with base
`0x08007000`, plus uninitialized blocks for RAM `0x20000000`, peripherals
`0x40000000` and flash data `0x08080000`. Judging by the startup code, it was built
with the Keil compiler (ARM CC), peripherals via ST HAL. For some functions the
decompiler infers a wrong signature (CRC shows up as `void`); such places were checked
against the disassembly.

## Microcontroller

STM32L4+ with 1 MB flash in two banks and 320 KB RAM, probably STM32L4P5/L4Q5:

- ADC `0x50040000`, GPIO `0x48000000`, DMAMUX `0x40020800`, UID `0x1FFF7590`,
  USB OTG FS `0x50000000`;
- flash writes pick the bank by the DBANK bit in `FLASH_OPTR` and address `0x08080000`,
  page size is 4 KB; only the L4+ dual-bank mode works this way;
- the bootloader marker is written to `0x2004FFFC`, the last word of 320 KB RAM.

## Startup

- Reset (`0x080071D4`) → `SystemInit` (`0x0800D5D4`) → stack `0x2000EC68` →
  scatter-load (`0x0800725C`) → `main` (`0x08013CF4`).
- Scatter-load, driven by the table at `0x0801CF7C`, unpacks `0x240` bytes of data from
  `0x0801CF9C` to `0x20000000` (Keil LZ decompressor, `0x08007280`) and zeroes
  `0x20000240`–`0x2000EC67`. In total the firmware uses about 59 KB of RAM.
- `main`: peripheral init (including I2C1 and I2C2 — `0x0800CDB0`,
  `0x0800CE0C`), loading config, settings, profiles and the macro index
  (`0x08012C94`), then an infinite loop.
- Timing comes from `0x0800E5CC` — probably the USB SOF callback (1 ms): every
  `settings+0x5F` frames (polling interval, `00:05`) the macro player, turbo and report
  sending run. Macro delays are counted in these intervals.

## Flash map

| Address | Contents |
|---|---|
| `0x08000000`–`0x08006FFF` | bootloader (not in the image) |
| `0x08007000`–`0x0801D043` | firmware |
| `0x0801A724` | firmware version, 4 bytes |
| `0x0801A728` / `0x0801AD75` | factory config / settings |
| `0x0801AF90` | key combination for mode `0xAA` |
| `0x0801AFA0` | LampArray attributes |
| `0x0801BA00`, `0x0801BBA0` | "matrix ↔ fwID" tables |
| `0x0801BA80` | sensor multiplexer lines: 8 × {GPIO port, pin} |
| `0x0801BCAE` | default profile, `0xA9D` bytes (name `default`) |
| `0x0801C74B` | default key assignments, `0x7E0` bytes |
| `0x08080000` | config, `0x64D` bytes |
| `0x08081000` | settings, `0x1E4` bytes |
| `0x08088000`–`0x0808DFFF` | profiles 1–6, one 4 KB page each; commands reach 1–5 |
| `0x08090000`–`0x080FFFFF` | macro storage, `0x70000` bytes |

The config, settings and profile blocks share a layout: the first word is the block
size, the last two bytes are CRC-16/CCITT-FALSE (init `FFFF`, polynomial `1021`) over
everything else. On a wrong size or CRC the firmware takes the factory block and
writes it.

## RAM map

| Address | Contents |
|---|---|
| `0x20000070`–`0x2000007C` | class `03` indicators |
| `0x200000F0` | device mode (`00:04`) |
| `0x200000F3` | Num/Caps/Scroll LEDs and indicators |
| `0x200000F4` | save dirty bits |
| `0x2000011C` | "control packet in progress" |
| `0x20000145` | layout variant (`2`, `3` or other) |
| `0x20000165` | calibration mode (`FE:20`) |
| `0x2000017C` | mask of profiles to save |
| `0x20000190` | flash addresses of profile pages |
| `0x200001A8` | macro heap statistics |
| `0x20002CCC` | HID report buffers |
| `0x20002D5C` | config |
| `0x200033AC` | settings |
| `0x20003690` / `0x200036EC` | control packet: request / response |
| `0x200048BC` | key state, `[8][16]` of 32 bytes |
| `0x200058BC` | raw sensor values, `[8][16]` u16 |
| `0x20005ABC` | history and averaging for driver mode |
| `0x20007000` | flash page buffer, 4 KB |
| `0x20008000` | profile 0 — working copy of the active profile |
| `0x20008A9D` | profile template, followed by profiles 1–6, `0xA9D` bytes each |
| `0x2000D4E8` | 20 macro player slots of 26 bytes |
| `0x2000D6F0` | macro body write buffer, 4 KB (up to 2 KB used) |
| `0x2000E7DC` | `SET_REPORT` data |

## Config, settings, profile

**Config** (`0x64D`): `+4`, `+5` — `00:06`; `+6` — serial number, 22 bytes; sensor
calibration — 12 bytes per key (`row·0xC0 + col·0xC`), a u16 pair at `+0x33` / `+0x37`;
`+0x62F`, `+0x63D` — `00:3D` blocks.

**Settings** (`0x1E4`):

| Offset | Field |
|---|---|
| `+2` | active profile |
| `+3` | delay between macro events (`06:07`) |
| `+4`, `+8`, `+0xC`, `+0x10` | macro index: capacity, used, address, CRC |
| `+0x1E` | 4 bytes `02:02` |
| `+0x25` | default lighting effect, 34 bytes |
| `+0x5D` | `0xAA` — demo mode |
| `+0x5E` | `0F:10` |
| `+0x5F` | polling interval (`00:05`) |
| `+0x60` | 3 bytes per key (`row·0x30 + col·3`); `02:13` writes the second and third |

**Profile** (`0xA9D`):

| Offset | Field |
|---|---|
| `+0` | number |
| `+2` | exists |
| `+6` | name |
| `+0x104` | Normal layer assignments: 126 × 8 bytes `fwID fnId size data[5]` |
| `+0x4F4` | Hypershift layer assignments, same format |
| `+0x8E4` | thresholds: 126 × 3 bytes `fwID thrH thrL` |
| `+0xA5E` | flag `02:07` |
| `+0xA5F` | flag `0F:06` |
| `+0xA63` | lighting effect, 34 bytes |

In driver mode, when a profile is loaded, the assignments of profile 0 are replaced
with the defaults from `0x0801C74B`. So in this mode profile 0 only takes thresholds.

## Flash writes

- `0x08011D48` — write of an arbitrary range: the 4 KB page is read into
  `0x20007000`, modified, erased and programmed in double words, up to 3
  retries. Any write, even 4 bytes, costs a page erase.
- `0x08011F24` writes a whole page, without reading; profiles
  (`0x08016F48`) and `0x080177D4` go through it.
- Endurance per DS12903 Rev 2, table 65: at least 10,000 page erases at
  −40…+105 °C; after 10,000 cycles data is retained 15 years at 85 °C, after
  1,000 — 30 years. A page erase takes about 22 ms.
- There is no wear leveling: config, settings and each profile always sit on their
  own page. In the macro heap the index lies at the start of storage and is rewritten
  on every macro creation and deletion.
- There is no erase counter, neither in the chip nor in the firmware: there are no
  other paths to erasing besides these two functions, and neither of them counts
  anything. How many times a page has been erased cannot be found out.
- Commands do not write to flash themselves: `0x08017724(n)` sets a bit in `0x200000F4` and a timer.

| n | Bit | Action |
|---|---|---|
| 0 | `02` | config — immediately, no timer |
| 1 | `01` | settings — after the timer (`0x67` ticks) |
| 4 | `08` | reboot once everything else is written (`00:0B 00`) |
| 2, 5, 7 | `04`, `10`, `40` | not handled in the save loop |

- Profiles are saved by `0x08016F48` per the mask `0x2000017C`: 100 ticks after the
  last change, one profile per 40 ticks.

## Sensors and actuation

- TIM2 (IRQ 28) → `0x08017D0C`: selects one of 8 multiplexer lines
  (`0x0801BA80`), reads two ADCs of 8 channels each and puts 16 values into a row of
  `0x200058BC`. All 128 positions are polled over 8 interrupts.
- When all 8 lines are read, TIM6 (IRQ 54) starts processing at `0x0800F6E8`.
  For each position depth = `255 − map(raw)`, where `map` is a linear mapping by the key calibration (`+4`/`+8` → `+0xC`/`+0x10` in
  the key state).
- On hardware (`FE:A7`, key Q): rest ≈ `0x0E53`, bottomed out ≈ `0x0240`, calibration
  `0x0448` / `0x0D0F`. A raw value below `0x0448` gives depth 255, above `0x0D0F` —
  0; the span from rest to `0x0D0F` is a dead zone at the start of travel. A pressed Q
  shifts the neighboring sensor A by about 10% of travel.
- Thresholds from the profile (`0x08017554`): press = `thrL` within `15..250`,
  release = `thrH`, but no less than `5` and no more than "press − 10". The press
  decision is `0x08019274`.
- Driver mode (`03`): depth is the average of 8 of the last 10 samples without the
  maximum and minimum (`0x08015A20`). Only changed keys go into the report, up to 11
  `fwID depth` pairs.
- Mode `08` in the code is a stream of raw values or calibration ranges of a selected
  key group. The group number (`0x200000EC`) is never written and is always `0`,
  so in this version the stream does not run.

## Press engine

`0x08016148` receives a key event and performs the action by the fnId from the
assignment — table in [protocol.md](protocol.md#actuation). The fnId and data length
check is `0x08012EBC`; it is called by `02:0D` and `02:0F`, while `02:12` writes the
assignment without checking.

## Macros

**Storage** — a heap with boundary tags directly in flash `0x08090000`–`0x080FFFFF`:

- at the start and end of each block is the same word `(size << 3) | free`,
  `size` includes the 8 bytes of tags;
- allocation (`0x08011B08`) — first fit free block, the size is rounded up to a
  multiple of 8, the remainder of the block is split off as free;
- freeing (`0x080119EC`) merges the block with free neighbors;
- empty storage is one free block of `0x70000` (`0x080119B0`, also used when
  `06:0A` formats the storage);
- statistics `0x200001A8` (`total, free, maxFree, …`) are computed by walking the
  blocks at startup (`0x08018AB8`); at runtime only "free" changes. `06:8E`
  returns "total" and `maxFree`, `06:86` walks the blocks again.

**Index** — an array of 14-byte records in the heap: `id32`, metadata block address
(`06:04` / `06:0C`), body address (`06:08`), 2 bytes. On first startup — 100
records (`0x578` bytes); on overflow the index is reallocated 50 records larger.
Hence `6FFF0` in "total" on an empty keyboard: `0x578 + 0x6FA78`.

**Body write** (`0x08018FE0`): `06:09` accumulates consecutive chunks of one macro in
the buffer `0x2000D6F0`, up to 2 KB. The buffer goes to flash when a chunk does not
continue the previous one, a different id arrives, the buffer is full or there has
been no write for 100 ms (`0x0800E5CC`).

**Player** (`0x08013940`) — 20 slots of 26 bytes:

| Offset | Field |
|---|---|
| `+0` | active |
| `+1` | key |
| `+2` | mode |
| `+3` | counter |
| `+6` | delay |
| `+0xA` | position |
| `+0xE` | current event |
| `+0x12` | start |
| `+0x16` | length |

An event is read directly from flash, only if the pointer is inside storage. Macro
bodies are not copied to RAM.

| Mode | fnId | Behavior |
|---|---|---|
| 1 | `03` | n passes |
| 2 | `04` | loops; release switches to mode 1 with counter 0 — the current pass plays to the end |
| 3 | `05` | loops; a repeated press acts like release in mode 2 |
| 4 | `0F` | each press sets the counter to 2, each event decrements it |

After each event the delay is `settings+3` plus the delays from events `11`–`14`.

## Control channel

- `SET_REPORT` (`0x21`/`09`) is parsed by the function at `0x0800DD3C`, data
  arrives at `0x2000E7DC`:
  - Output report — Num/Caps/Scroll LEDs;
  - Feature on interface 3 — control packet, accepted only when
    `wLength == 90` and `0x2000011C` is free;
  - Feature on interface 4 — LampArray.
- The packet is copied to `0x20003690`, the dispatcher (`0x08015568`) runs immediately,
  in the USB interrupt, the response is copied to `0x200036EC` for `GET_REPORT`.
- The request CRC is not checked; the response gets the XOR of bytes 2..87. During
  processing the status is `01`, afterwards `02`; the host only sees `02`.

| Class | Handler | Condition |
|---|---|---|
| `00` | `0x08014B8C` | — |
| `02` | `0x08014190` | — |
| `03` | `0x08014F84` | not in mode `0xAA` |
| `05` | `0x08015620` | — |
| `06` | `0x08015178` | — |
| `0F` | `0x08014730` | not in mode `0xAA` |
| `F0`, `FE` | `0x08015794` | only in device mode `02` |

Unknown command — status `05`, argument error — `03`.

## USB

- Configuration descriptor — `0x0801A69C`, 134 bytes, 5 interfaces.
- HID report descriptors — `0x0801A3B0`–`0x0801A52C`: mouse, consumer, vendor,
  keyboard with report ID, NKRO, LampArray (`05 59`); no gamepad found among them.
- Strings: `Razer`, `Razer Huntsman V2 Analog`, `HID Config`, `HID Interface`.
- LampArray feature reports (`0x080164B0`):
  - `1`–`3` — attributes; the lamp count depends on the layout: 148, 149 or 153;
  - `4`, `5` — lamp update, switches the effect to `08`, ignored when
    `0F:10 01`;
  - `6` — AutonomousMode; enabling it restores the effect from `settings+0x25`;
  - `7` — control packet, passed to the same dispatcher.

## Lighting

- Effects accepted by `0F:02` (`0x08010284`): `00`–`09`. `09` is fire; Synapse does
  not have it. The check lets it through without parameters, and the frame (`0x08016884`)
  does not read a single effect byte: every 9 ticks the rows shift up with
  fading, the bottom row is filled with random "embers" `0x20..0x7F`, the color comes
  from a fixed palette. Only overall brightness is adjustable. Effect `0x89` from demo mode does not pass the check,
  and `0x08017EBC` starts it only in mode `0xAA`; it cannot be enabled in normal mode
  without custom firmware.
- Four drivers on I2C: `0xA0`, `0xB8`, `0xBE` on I2C1, `0xA6` on I2C2 (8-bit
  addresses). The firmware writes to `0xA6` only when PB9 is low — probably
  the wrist rest with a connection sensor.
- Write protocol: `0xFE` — unlock, `0xFD` — page; page 0 is
  on/off (24 bytes), page 1 is PWM (192 bytes). Probably an ISSI IS31FL3733
  or a compatible driver.

## Device modes

| Mode | Behavior |
|---|---|
| `00` | hardware: the keyboard sends key presses itself |
| `01` | interrupts disabled, `0xAAAAAAAA` to `0x2004FFFC`, `SYSRESETREQ`; the keyboard shows up as bootloader `1532:110E`, probably due to this marker |
| `02` | factory: unlocks classes `F0` / `FE` |
| `03` | driver: depth stream, the host handles presses |
| `08` | raw value stream code exists, but does not work in this version |

**Mode `0xAA`** (`settings+0x5D`) — demo. It toggles if Esc + Caps Lock + Space are
held when plugging in (`0x080116B0`, table `0x0801AF90`), and is saved to settings;
the same combination on the next plug-in turns it off.
Verified on hardware:

- lighting is a wave; on a press the wave stops and spreads from the key, farther
  the deeper the press (internal effect `0x89`, `0x080115D8`); with no presses
  the wave returns after ~10 s;
- classes `03` and `0F` reply ok, but do nothing and read back zeros;
- the keyboard types, assignments and macros work;
- key polling runs even without configured USB (`0x0800BAF4`), so the effect
  probably works from a charger without a computer too.

## Not analyzed

- Bootloader — not in the image.
- Purpose of the fields `02:07`, `02:13`, `0F:06`, `00:06`,
  `00:3D` and fnId `06`, `08`.
- USB device descriptor — probably built in code; not searched for.

## Huntsman Mini Analog

Firmware `1.03.00`, built from the same code as the V2 Analog one; here only what was
checked and what differs.

### Image

`DeviceUpdater.resources`:

| Key | Value |
|---|---|
| `VID` / `PID` | `1532` / `0282` |
| `BLPID` / `BLVER` | `110E` / `2.0` |
| `DevFWVer` | `1.03.00` |
| `FlashFWUpdate` | `0`: no second chip |

`DevFWLine0..5179` is Intel HEX, `0x08007000`–`0x0801B373`, 82,804 bytes, not
encrypted. As on the V2 Analog, the bootloader takes `0x08000000`–`0x08006FFF`.

### Same as the V2 Analog

- USB: 5 interfaces with the same HID descriptors (61, 212, 79, 22, 327 bytes), the
  configuration descriptor is at `0x0801936C`; control is a 90-byte feature report,
  the depth stream is report `07`.
- Threshold clamp: the tail of the function at `0x08016D4C` matches the V2 one
  byte for byte: press `15..250`, release no less than `5` and no more than press
  − 10.

### Key table

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

### Not analyzed

- Class dispatcher, the command set and the profile layout: assumed the same as on
  the V2 Analog.
- Lighting matrix size.
