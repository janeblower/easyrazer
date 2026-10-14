# User guide

[Русская версия](../ru/usage.md)

- [Getting started](#getting-started)
- [Apply vs. Write](#apply-vs-write)
- [HW and Driver modes](#hw-and-driver-modes)
- [Selecting keys](#selecting-keys)
- [Profiles](#profiles)
- [Keys](#keys): [Actuation](#actuation), [Rapid Trigger](#rapid-trigger), [Snap Tap](#snap-tap), [Bindings](#bindings), [Hypershift](#hypershift), [Macros](#macros)
- [Lighting](#lighting)
- [Settings and tray](#settings-and-tray)
- [Troubleshooting](#troubleshooting)

## Getting started

> [!IMPORTANT]
> Close Razer Synapse completely, including its tray icon. Synapse keeps the keyboard
> in its own mode and overwrites the settings. While it runs, EasyRazer writes nothing
> and shows a warning in the status bar.

1. Download `EasyRazer.exe` from the [latest release](https://github.com/janeblower/easyrazer/releases/latest).
   It is a single file with no installer.
2. Run it. On first launch the app offers to start with Windows — accept if you use
   anything the app holds itself: Rapid Trigger, Snap Tap, the custom lighting layout,
   and restoring settings after reconnect and sleep.

Settings and backups are kept in `%APPDATA%\EasyRazer`.

## Apply vs. Write

Every tab has two ways to send changes to the keyboard:

| | **Apply** | **Write** (💾) |
|---|---|---|
| Where it goes | the keyboard's RAM and EasyRazer's settings | the keyboard's flash memory |
| Works without the app | until reconnect | yes, on any computer |
| Flash wear | none | one write |

After a reconnect EasyRazer, if running, restores applied values itself.

> [!WARNING]
> Flash has limited rewrite endurance. Write the finished result, not every attempt.
> The confirmation before writing can be turned off in Settings.

Before the first write in a session, a backup of all keys is saved to
`%APPDATA%\EasyRazer\backup-*.txt`.

Models not yet tested on hardware (🧪 in [supported keyboards](supported.md)) get only
"Apply", and only after turning on "Write to keyboards not yet checked on hardware" in
Settings.

Rapid Trigger, Snap Tap and the custom colour layout are kept in the app and are never
written to the keyboard's flash.

## HW and Driver modes

The switch is on the right above the tabs. The mode is remembered and turned on at
launch. While Synapse runs, the switch is unavailable.

| | **HW** | **Driver** |
|---|---|---|
| Who types | the keyboard, as with no software | EasyRazer, from the reported key depth |
| Without the app running | works | the keyboard is silent |

What each mode offers depends on the keyboard line:

| | Huntsman V2 Analog, Mini Analog | Huntsman V3 line 🧪 |
|---|---|---|
| Actuation range | HW: 1.62–3.56 mm (firmware limit); Driver: 1.5–3.6 mm | 0.1–4.0 mm in both modes; Low-profile TKL: up to 2.8 mm |
| Rapid Trigger | Driver only, computed by the app | HW: run by the firmware; Driver: computed by the app |
| Snap Tap | Driver only, any groups | HW: run by the firmware, one pair; Driver: any groups |

On the V2 line Driver mode is needed only for Rapid Trigger and Snap Tap. On the V3 line
HW mode covers both, with the Snap Tap limit described [below](#snap-tap).

In Driver mode:

- In windows run as administrator, on the lock screen and in UAC prompts the keyboard
  switches to HW for that time: Windows does not let keystrokes from a regular app
  through there. On the V2 line Rapid Trigger and Snap Tap stop there; on the V3 line
  the firmware keeps Rapid Trigger and one Snap Tap pair. Ctrl+Alt+Del does not work.
- The app returns the keyboard to HW on exit, on a crash and when Synapse starts. If the
  process is killed forcibly (Task Manager), the keyboard is silent until the cable is
  reconnected.
- Key repeat comes from the Windows keyboard settings.
- V2 line: EasyRazer handles the Fn layer from the profile's [Hypershift](#hypershift)
  layer; media keys and the volume wheel work as usual. The keyboard sends the volume
  wheel as a mouse wheel and EasyRazer turns it into volume. Games that read the mouse
  via Raw Input still see it as scrolling.
- V3 line: entering Driver mode makes the firmware switch the startup profile (⏻) to
  slot 1.

Why the modes exist: [how it works](how-it-works.md#hw-mode-and-driver-mode).

## Selecting keys

The same on the "Keys" and "Lighting" tabs (custom layout):

- click selects one key, Ctrl+click adds or removes one;
- drag a frame with the mouse to select an area;
- "Select all", "Clear selection".

If the selected keys have different values, the slider is empty and the label says
"mixed". A slider changes only its own parameter; the rest stay as they were on each key.

## Profiles

The list on the right above the tabs, next to HW / Driver. Any number of profiles; each
holds actuation points, bindings, lighting and Rapid Trigger.

- Picking a profile in the list writes nothing to keyboard memory: the profile works while
  EasyRazer runs.
- **Fn+Menu** switches to the next profile in the list while the app runs. Without the
  app, Fn+Menu cycles the profiles in keyboard memory, as the firmware intends.

| Mark | Meaning |
|---|---|
| `1/5`…`5/5` | written to one of the five keyboard slots, works without the app |
| "app only" | exists only in the app |
| ⏻ | turns on when the keyboard is connected without the app |
| "not written" | changed since it was written to its slot |

- To change the ⏻ profile: ⋯ → "Start without the app"; this is one write to keyboard memory.
- ⋯ on a profile row: rename, duplicate, write to a free slot, free the slot (the profile
  stays in the app), delete.
- "Save" (💾) writes to the profile's slot only what differs from what is written there.

## Keys

Two sections on one keyboard map: "Actuation" and "Bindings". The selection is
shared. "Apply" and "Save" send the changes of both sections at once.

### Actuation

Actuation point of each key, step 0.1 mm; the lower the value, the more sensitive the
key. The range depends on the model: 1.5–3.6 mm on the V2 line, 0.1–4.0 mm on the V3 line
(up to 2.8 mm on the Low-profile TKL).

- In Driver mode the slider fills green to the depth of the first pressed key — handy
  for fitting the point to your travel. In HW mode the keyboard does not report depth.
- V2 line, HW mode: the slider marks the limits 1.62 and 3.56 mm. A point beyond them can
  be chosen, but the keyboard will actuate at the limit — then the mark lights up.
- "Write" stores the values in the slot of the loaded profile; for a profile without a
  slot, in a free slot.
- "● not applied: N" — how many keys are changed but not yet applied.
- A red key — the keyboard did not confirm the value; details in the tooltip.
- The Fn key itself is not configurable. On models with [Hypershift](#hypershift) the
  actuation point of the Fn layer is the "Separate for Hypershift" switch under the slider.

### Rapid Trigger

The key actuates again as soon as it starts going down after a lift, and releases as
soon as it starts going up — without waiting for the actuation point.

- "Off / RT / CRT" and sensitivity 0.1–1.0 mm: how far the key has to move
  to actuate or release. RT works only below the actuation point. CRT (Continuous Rapid
  Trigger) works along the whole travel once the key has actuated, until it is fully up.
  On the V3 line in HW mode the firmware has one CRT switch for the whole profile, so
  keys choose only Off / RT, and the separate "CRT" button turns it on for every RT key.
- "Separate press and release" — a separate slider for release.
- The sliders show the travel of the first pressed key since the last extreme point.
  When going down, the fill grows from the top to the slider; when going up, from the
  slider upward; once it reaches the end, the key has actuated or released. With separate
  settings the slider of whatever happens next fills, and the arrow at the actuation
  slider shows which way the key has to move next.
- Keys with RT are marked "RT" or "CRT" on the map.

Where RT runs depends on the model:

- **V2 line** — only in Driver mode: these keyboards have no hardware RT, so the app
  computes it. In HW mode the RT settings are kept but have no effect.
- **V3 line** — in both modes: in HW mode the firmware runs it, in Driver mode the app.

> [!NOTE]
> Either way RT is stored in EasyRazer, not in the keyboard's flash. On the V3 line the
> app sends it to the keyboard's RAM with the loaded profile, so without the app it lasts
> until reconnect.

### Snap Tap

Of the keys in a group only one works, even if several are held: for example, A and D
do not cancel each other while strafing.

- Select keys on the map and press "Group the selected". A group can have any number of
  keys, and there can be any number of groups; a key belongs to only one group.
- Each group has a rule: "Last" — the last pressed one works, "Neutral" — none works,
  "Deeper" — the one pressed deeper works.
- Group keys are marked on the map with a coloured strip on top.
- Fn+LShift turns Snap Tap on and off without opening the window; it can be moved to
  another combination in [Hypershift](#hypershift).
- Snap Tap is stored in EasyRazer, not in the keyboard's flash.
- **V2 line** — works only in Driver mode.
- **V3 line** — in Driver mode any groups work. In HW mode the firmware runs a single
  pair: the first two keys of the first group, and only with the "Last" rule; the rest is
  kept but has no effect.

> [!CAUTION]
> Some games (for example, CS2 on Valve servers) forbid Snap Tap — turn it off there.

### Bindings

Huntsman V2 Analog and Mini Analog.

A binding is changed on one key at a time: pick it with a click. What it does in the
normal layer:

- **Key** — any keyboard key, optionally with Ctrl, Shift, Alt, Win; the key field has
  search by name;
- **Mouse** — left, right, middle button, "Back", "Forward", wheel up and down (wheel —
  one step per press);
- **Media** — play / pause, tracks, stop, sound, volume;
- **Macro** — a macro from the [Macros](#macros) section: several times in a row, while
  the key is held or until pressed again;
- **Disabled** — does nothing;
- **Default** — restore the factory binding.

The map labels keys with a non-default binding. In Driver mode bindings are executed by
EasyRazer, in HW by the firmware; those written to memory work in both modes.

### Hypershift

Huntsman V2 Analog and Mini Analog.

The "Hypershift · Fn+K" card sets the action for each combination with Fn: key, mouse,
media, macro, system (brightness, sleep, game mode, M indicator, next profile, Snap Tap)
or disabled.

Defaults: Fn+F11/F12 — brightness, Fn+Pause — sleep, Fn+F10 — game mode (blocks Win),
Fn+F9 blinks M, Fn+Menu — next profile, Fn+LShift — Snap Tap.

- "Next profile" and Snap Tap can be on only one combination: if you pick a taken one,
  the app offers to move it.
- On the "Actuation" section the "Separate for Hypershift" switch gives the Fn layer its
  own actuation point (blue slider); on keys with a separate point it shows as a blue line
  above the normal value.
- With Fn held in Driver mode the Hypershift point works, without Rapid Trigger and
  Snap Tap.

### Macros

Huntsman V2 Analog only.

A macro is a sequence of presses and releases of keys, mouse buttons and pauses.
**Record** captures keyboard presses together with the pauses between them (the "With
pauses" checkbox); steps can be added with buttons, edited, reordered and deleted.

- **Apply** — the macro is saved in EasyRazer. In Driver mode the app plays it right away.
- **Write** (💾) — the macro is written to keyboard memory. Only then does it play in HW
  mode and without the app. Unwritten macros are marked ●. Below the steps — how much
  memory the macro takes and how much is free.

In HW mode the keyboard plays only written macros: when applying a binding with an
unwritten macro, EasyRazer offers to write it; writing a binding writes the macro too.

- A macro bound to keys cannot be deleted — remove the bindings first.
- Pauses are counted in milliseconds at a 1000 Hz polling rate.

## Lighting

Built-in keyboard effects with live preview: off, static, breathing (1 colour, 2 colours,
random), spectrum, wave (direction, speed), reactive (key or ripple), starlight, fire,
custom layout. A shared brightness slider. The set depends on the model; see
[supported keyboards](supported.md).

- **Apply** — the effect turns on now and on every EasyRazer launch.
- **Write** (💾) — the effect is written to keyboard memory and works without the app.
- Reactive, ripple and spectrum have no speed. Fire has no settings at all: speed,
  height and palette are baked into the firmware; only brightness works.

### Custom layout

Huntsman V2 Analog only.

Each key and zone gets its own colour. Zones on the map: case trim (the frame around the
keys), media keys and wheel (above the numpad), wrist rest (the strip at the bottom).
Select keys and pick a colour — the keyboard shows it at once.

The custom layout **is not written to the keyboard** (the firmware cannot do it); it works
while EasyRazer runs, including minimized to the tray. The last layout is remembered.

### Windows Dynamic Lighting

If Windows "Dynamic Lighting" controls the lighting, keyboard effects are not visible —
the app warns and offers to turn it off. The switch is also on the tab. The V3 TKL 8KHz
and V3 Pro Low-profile TKL 8KHz do not support Dynamic Lighting.

## Settings and tray

| Setting | Options |
|---|---|
| Language | Russian, English |
| Start with Windows | starts minimized to the tray; also in the tray menu |
| On window close | ask, minimize to tray, quit the app |
| Watch for Synapse | when Synapse exits, the app restores its settings |
| Ask before saving to keyboard memory | on / off |

"Quit" in the tray menu always quits. Launching `easyrazer.exe` again opens the window
of the running copy.

## Troubleshooting

| Problem | Fix |
|---|---|
| The keyboard does not type | Reconnect the cable: it starts in HW mode. |
| "This Razer keyboard model is not supported" | See [supported keyboards](supported.md). |
| Settings are broken | Delete `%APPDATA%\EasyRazer\settings.json` to reset the app. |
| Need old actuation points back | Restoring from a backup is manual for now: the key values are in `%APPDATA%\EasyRazer\backup-*.txt`. |

Still stuck? [Open an issue](https://github.com/janeblower/easyrazer/issues) with your
keyboard model and what you did.
