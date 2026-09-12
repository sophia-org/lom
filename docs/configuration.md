# Configuration and ironbar migration

Lom accepts KDL **v2** documents with `version 1` naming Lom's configuration
schema. Both configuration and theme are explicit command-line files in this
tranche. No legacy format importer, XDG discovery or live file watcher exists yet.

The complete starting point is `examples/minimal/`. Module names are required,
nonempty and unique. A document contains exactly one named panel, with optional
`start`, `center` and `end` groups. Modules remain in configured group order.

```kdl
version 1
panel "main" {
    position "top"
    height 24
    margin 0 0 0 0
    popup-gap 5
    start {
        workspaces "views" {
            name-map {
                name "1" "web"
            }
            hidden "scratch"
            all-monitors #false
        }
    }
    center {
        label "greeting" {
            text "Привет — Lom"
        }
    }
    end {
        clock "time" {
            format "%d/%m/%Y %H:%M"
            format-popup "%H:%M:%S"
            show-week-numbers #true
        }
    }
}
```

## Supported settings

| Ironbar concept | Lom KDL | Behavior/default |
| --- | --- | --- |
| `position` | `position "top"` | top/bottom/left/right; default bottom |
| `height` | `height 24` | Logical thickness, 1–512; default 42 |
| margin object | `margin TOP RIGHT BOTTOM LEFT` | Signed logical values −512…512; default zero |
| `popup_gap` | `popup-gap 5` | Desired logical gap, 0–512; default 5 |
| module arrays | `start`, `center`, `end` blocks | Ordered named module nodes; default empty |
| label content | `label "id" { text "..."; }` | Plain UTF-8; no Pango markup or dynamic script |
| workspace `name_map` | `name-map { name "old" "new"; }` | Display names only; action identity does not change |
| `hidden`, `all_monitors` | `hidden "name"`, `all-monitors #true` | Names omitted; current output only by default |
| workspace sort | implicit | Numeric displayed labels first, then lexical labels; stable ID tie-break |
| clock formats | `format`, `format-popup` | Chrono/strftime; defaults `%d/%m/%Y %H:%M`, `%H:%M:%S` |
| `show_week_numbers` | `show-week-numbers #true` | ISO week numbers; default false; Monday-first calendar |
| battery `show_if` script | `visible-when "available"` | Hide absent observation; never executes a command |
| `focused`, `battery`, `sys_info`, `tray` | `focused`, `battery`, `sys-info`, `tray` | Text presentations from explicit fixtures only |

The current clock consumes explicit offset-bearing time observations. Locale
and timezone discovery, real-time scheduling and locale-sensitive calendars are
not integrated yet; the calendar uses Chrono's English month/day presentation.
Opening resets the calendar to the observed month. Ticks preserve navigation.
Navigation across year/leap-day boundaries and dismissal are reducer-tested.

KDL position, margins and popup gap express **desired placement**, retained in
the model for a future Engine allocation request. The local panel PNG represents
one allocation's content; it does not draw a screen or simulate reservation,
margin placement or native popout anchoring. `--width` means physical long-axis
extent (height for a vertical panel); thickness is `ceil(height × scale)`.

Unknown or unsupported settings are errors, not ignored compatibility promises.
This includes scripts, favorites, profiles, automatic hiding, tooltips, images,
custom widgets, CSS, layer-shell `layer`, connector selectors, reservations and
service actions. Native output facts do not currently supply ironbar's connector
names, so monitor mapping cannot be inferred from a name. `icon_size`, battery
formatting and per-service options remain work for their actual module adapters.
Documents are bounded to 256 KiB and panels to 64 modules. These are **local
parser/preview limits**, not advertised Sophia capability limits.

## KDL themes

```kdl
version 1
theme "minimal" {
    defaults {
        background "#1c1c1c"
        foreground "#ffffff"
        selected "#2d2d2d"
        active "#6699cc"
        urgent "#8f0a0a"
        font-size 13
        gap 13
        padding 7
    }
    module "clock" {
        bold #true
    }
    instance "time" {
        foreground "#aaddff"
    }
}
```

Resolution is defaults → module kind → configured instance, independent of
source order. Duplicate fields/overrides, unknown kinds, invalid colors and
instance names absent from the panel are rejected. Colors use `#rrggbb` only.
Font sizes are 6–96 logical pixels, gaps/padding 0–128. `bold` is a KDL boolean.
The regular and bold DejaVu Sans Mono faces are explicitly bundled and registered;
preview layout does not discover system fonts. The supplied fixture's musical
note and diamond are glyphs from that font, not fetched tray icons.

`selected` colors an active workspace's background, `active` its bottom strip
and the calendar's today cell; visible workspaces have a foreground-colored
strip; urgent fill takes precedence over the selected fill. Square corners and
no decorative border/shadow match Minimal. This tranche does not yet implement
GTK's hover/pressed selector cascade, transitions, arbitrary font selection or
all of ironbar's typography options. Fixture styling is not evidence of an
admitted pointer/hover stream.

## Explicit fixture input

`preview` requires a separate fixture file. This input never executes a command,
queries `/sys`, opens a bus or substitutes for admission.

```kdl
version 1
output 1
active-output 2
time "2026-09-12T18:35:42-04:00"
workspace 1 1 "1" {
    active #true
    action 11
}
value "focused" "Example title"
```

`output` and `time` are required. `active-output` is optional and may name an
output with no indicators. Workspace arguments are positive indicator ID,
positive output ID and name. Optional child fields are `active`, `visible`,
`urgent` and positive `action`; no action means no activation control. IDs are
unique and entries are bounded to 256. `value` supplies plain text to a configured
focused/battery/sys-info/tray instance; unknown or duplicate names fail.
Missing data is shown as unavailable unless `visible-when "available"` requests
hiding. Workspace and time fixture observations use the same reducer as other
observations; the model carries an explicit fixture marker.

A diagnostic calendar image is rendered separately using local popout intent.
Neither opening it nor issuing an activation effect executes a native action.
