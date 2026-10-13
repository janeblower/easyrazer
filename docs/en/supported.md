# Supported keyboards

✅ supported, 🧪 implemented but not tested on hardware, — not supported.

| Model | Actuation | Rapid Trigger | Snap Tap | Remapping | Macros | Hypershift | Lighting |
|---|---|---|---|---|---|---|---|
| Huntsman V2 Analog (`1532:0266`) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Huntsman Mini Analog (`1532:0282`) | 🧪 | 🧪 | 🧪 | 🧪 | — | 🧪 | 🧪 |
| Huntsman V3 Pro (`1532:02A6`) | 🧪 | 🧪 | 🧪 | — | — | — | 🧪 |
| Huntsman V3 Pro TKL (`1532:02A7`, `1532:02D0`) | 🧪 | 🧪 | 🧪 | — | — | — | 🧪 |
| Huntsman V3 Pro Mini (`1532:02B0`) | 🧪 | 🧪 | 🧪 | — | — | — | 🧪 |
| Huntsman V3 TKL 8KHz (`1532:02E5`) | 🧪 | 🧪 | 🧪 | — | — | — | 🧪 |

## Huntsman V2 Analog

Supported, tested on hardware. Rapid Trigger and Snap Tap are computed by the app
(driver mode); some macro modes are untested.

## Huntsman Mini Analog

Same as the V2 Analog, with a 60% key map; no custom lighting effect. Not tested on
hardware: read only by default, writes to the keyboard's RAM after allowing it in
Settings, never to its memory, so macros and profile slots are not available.

## Huntsman V3 Pro

In development, not tested on hardware. Read only by default; writes to the keyboard's
RAM after allowing it in Settings, never to its memory. In HW mode the firmware runs
Rapid Trigger and one Snap Tap pair. A tester with the keyboard is needed.

## Huntsman V3 Pro TKL

Same as the V3 Pro, without the numpad and PrtSc / ScrLk / Pause; the 8 kHz version
is the same. No custom lighting effect.

## Huntsman V3 Pro Mini

Same as the V3 Pro, with a 60% key map; no custom lighting effect.

## Huntsman V3 TKL 8KHz

Same as the V3 Pro, with a full 87-key TKL map; no custom lighting effect and no
Windows Dynamic Lighting. Its firmware is a separate code base, so it is less certain
than the V3 Pro models.

Protocol and firmware notes for each model: [devices](devices/README.md).
