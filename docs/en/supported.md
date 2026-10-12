# Supported keyboards

✅ supported, 🧪 implemented but not tested on hardware, — not supported.

| Model | Actuation | Rapid Trigger | Snap Tap | Remapping | Macros | Hypershift | Lighting |
|---|---|---|---|---|---|---|---|
| Huntsman V2 Analog (`1532:0266`) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Huntsman V3 Pro (`1532:02A6`) | 🧪 | 🧪 | 🧪 | — | — | — | 🧪 |
| Huntsman V3 Pro Mini (`1532:02B0`) | 🧪 | 🧪 | 🧪 | — | — | — | 🧪 |

## Huntsman V2 Analog

Supported, tested on hardware. Rapid Trigger and Snap Tap are computed by the app
(driver mode); some macro modes are untested.

## Huntsman V3 Pro

In development, not tested on hardware. Read only by default; writes to the keyboard's
RAM after allowing it in Settings, never to its memory. In HW mode the firmware runs
Rapid Trigger and one Snap Tap pair. A tester with the keyboard is needed.

## Huntsman V3 Pro Mini

Same as the V3 Pro, with a 60% key map; no custom lighting effect.

Protocol and firmware notes for each model: [devices](devices/README.md).
