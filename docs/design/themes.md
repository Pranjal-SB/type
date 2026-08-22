---
type: reference
status: living
area: design
verified: 2026-08-22
---

# Themes

A theme is a TOML file. Six ship inside the binary, any number can live in the config
directory, and every one of them — shipped or not — is measured against the same rubric
before it is allowed to be a theme.

Spec: [`architecture.md`](architecture.md) §4 "Clean: one visual system applied uniformly",
§7 terminal capability detection.

## The format

```toml
name = "TYPE Slate"
kind = "dark"          # or "light"

[palette]
base00 = "#10141b"     # named colours, any names you like
accent = "#4f8cc9"

[ui]
bg = "base00"          # a palette name, or a "#rrggbb" literal
border_focused = "accent"

[syntax]               # optional, parsed and validated, nothing reads it until M2.6
"function" = { fg = "accent" }
```

Three sections and two required scalars. `[palette]` names colours; `[ui]` assigns them to
the editor's 25 slots. A `[ui]` value is either a `[palette]` key or a `#rrggbb` literal —
there is no third form, and a name that resolves to neither is an error naming the line.

**`kind` is declared, not inferred.** It picks one of the contrast floors, and the audit
checks that the declaration agrees with the background's luminance. A `kind = "dark"` theme
with a pale page is rejected rather than quietly measured against the wrong floor.

**Every key is optional.** A file that mentions nothing still loads — each unset key keeps
the shipped default. That is what makes "copy a theme, change four lines" work, and it is
why `inherits` was never built: every theme already inherits the default.

A misspelled key gets a did-you-mean when the edit distance is small enough to be a typo,
and no suggestion when it isn't. `forgeground` suggests `fg`; `banana` gets told it is not a
key and left alone.

## The 25 slots

| Group | Keys |
|---|---|
| Page | `fg` `bg` `cursor_line_bg` |
| Gutter | `gutter_fg` `gutter_bg` `line_number_fg` `line_number_current_fg` |
| Selection | `selection_fg` `selection_bg` `selection_primary_bg` |
| Brackets | `bracket_match_fg` `bracket_match_bg` |
| Chrome | `chrome_bg` `border` `border_focused` |
| Status bar | `status_bar_bg` `status_bar_fg` `status_bar_inactive_fg` `status_bar_accent` |
| Tree | `tree_directory_fg` `tree_file_fg` |
| Diagnostics | `diagnostic_error` `diagnostic_warning` `diagnostic_info` `diagnostic_hint` |

`chrome_bg` is the raised surface the sidebar and status bar share. The editor keeps `bg`.
Two levels, not three — `cursor_line_bg` is the third tint and a sidebar in that exact
colour collides with it.

`selection_primary_bg` exists because with thirty cursors something has to say which one
every motion is relative to.

The four `diagnostic_*` slots are painted by nothing until M3. They are in the format now
because adding them later is a breaking change to every theme in the wild, which is the same
argument that put `[syntax]` in ahead of the highlighter. This is the *only* sanctioned
reason to ship a field nothing reads, and it costs a doc comment naming the milestone that
takes ownership.

## The rubric

`typ_core::audit(&colors, kind)` returns the list of everything wrong with a palette. Empty
means it passes. It is public because a theme author needs to run the same check the project
runs — a rubric only the project can execute is a rubric community themes ignore.

Every ratio is WCAG 2.1 computed from actual channel values.

| Rule | Floor | Why |
|---|---|---|
| `fg on bg` | ≥ 7.0 | AAA. The pair you stare at all day. |
| `fg on cursor_line_bg` | ≥ 7.0 dark, ≥ 6.5 light | See below. |
| `cursor_line_bg vs bg` | ≠ identical, < 1.5 | Above 1.5 it is a stripe, not a hint. |
| `line_number_fg on bg` | ≥ 3.0 | WCAG's non-body floor. Below it the gutter is texture. |
| `fg` over `line_number_fg` | further from `bg` | Numbers must be quieter than the code. |
| `line_number_current_fg` vs `line_number_fg` | ≠ identical, further from `bg` | A number *closer* to the page reads as disabled. |
| `selection_fg` on both selection grounds | ≥ 4.5 | |
| `selection_primary_bg vs selection_bg` | ≠ identical, ≥ 1.3 | |
| `bracket_match_fg on bracket_match_bg` | ≥ 4.5 | |
| `border_focused` vs `border` | further from `bg` | Focus is gained attention, not lost. |
| `status_bar_fg` | ≥ 4.5 | |
| `status_bar_inactive_fg` | ≥ 3.0, quieter than active | It carries content, not decoration. |
| `tree_directory_fg` / `tree_file_fg` on `chrome_bg` | ≥ 4.5 | Measured on the surface it draws on, not on `bg`. |
| `tree_directory_fg vs tree_file_fg` | ≠ identical | |
| `chrome_bg vs bg` | ≠ identical | The surface has to be a surface. |
| every `diagnostic_*` on `bg` | ≥ 4.5 | Read individually, so legible alone. |
| `diagnostic_error vs diagnostic_warning` | ≥ 1.8 | Deuteranopia. See below. |
| every slot | truecolor | An ANSI-16 name inherits whatever the terminal defines, which cannot be measured or tuned. |

### Emphasis is distance from the ground, not luminance

`|luminance(x) − luminance(bg)|`, never `luminance(x)` alone. "Brighter than" says the right
thing only on a dark page: on a pale one, emphasis moves *down* in luminance and recession
moves up. Four rules were written as bare luminance comparisons and all four rejected a
correct light palette. One substitution fixed all of them.

### Red against amber

`diagnostic_error vs diagnostic_warning ≥ 1.8` separates the two by lightness as well as
hue, so a red-green colour-blind reader can still tell an error from a warning. This is the
one diagnostic distinction that changes what somebody does about it.

Palettes designed for harmony fail this routinely. Measured across 97 published terminal
palettes, it is the single most-failed rule in the set — Rosé Pine misses by 0.03, Tokyo
Night by 0.48. A port that fails it gets its warning colour nudged inside its own hue, and
the file header says so.

### The light-ground problem

A saturated hue on a near-white page has a hard luminance ceiling. Catppuccin Latte's
published accents against its own base: two of eight clear 4.5. That is arithmetic, not
carelessness, and no published light palette in the field clears it because none is asked to.

It squeezes from the other side too. Latte's body text is 7.06 on base — a whisker over AAA —
so any current-line tint at all pushes `fg on cursor_line` under 7.0. Best achievable is
6.76. So `fg on cursor_line_bg` drops to 6.5 on a light ground, and the reason is recorded
here rather than in a commit nobody reads. Every other floor holds on both grounds.

The way out is not a lower floor, it is a different kind of accent. Alabaster clears
every rule but one with `error #704040` (8.21) and `warning #806850` (5.11) — desaturated
and darkened rather than fighting for saturation it cannot have. On a pale page, take the
hue down in lightness and saturation until it clears, and accept that it reads muted.

## Degrading to 256 colours

Themes are written in truecolor. `typ_app::capability::detect()` decides the depth once at
startup — `COLORTERM=truecolor|24bit`, or a `-direct` terminfo entry, otherwise 256. There
is no 16-colour path: there is no sane mapping onto the sixteen, and a terminal that cannot
manage 256 cannot manage the rest of the editor.

`downgrade_theme` maps each colour to the nearest cube or greyscale entry by perceptual
distance. **Quantising moves every colour, and it moves them by different amounts**, so a
palette that passes at truecolor can fail at 256 — two neighbouring ramp steps can collapse
onto the same cube entry and a surface stops being a surface. Every shipped theme is audited
at both depths for exactly this reason. Nobody else in the field checks it, so nobody else
knows whether their theme survives a terminal without truecolor.

`config.toml` gets a `color_depth` override. A multiplexer is deliberately not special-cased:
tmux 3.2 with `terminal-features ",*:RGB"` passes truecolor through correctly and nothing in
the environment distinguishes a configured tmux from an unconfigured one, so the claim is
believed and the escape hatch is a setting rather than a better guess.

## Loading

```toml
# config.toml
theme = "slate"
```

Lookup order:

1. `<config_dir>/themes/<name>.toml`
2. the embedded theme of that name

The config directory wins, which is what makes "copy a shipped theme and edit it" work and
is the only reason the embedded set is not a closed list. Themes are embedded rather than
installed beside the executable because `cargo install typ-editor` produces a binary with no
runtime directory to find, and the 100 ms cold-start budget has no appetite for a five-step
path search.

**A theme problem is never a startup failure.** A bad colour returns the shipped palette plus
a warning. An editor that refuses to open because of a colour is an editor you cannot use to
fix the colour.

## Writing one

```
cp <config_dir>/themes/slate.toml <config_dir>/themes/mine.toml
```

Edit `[palette]`, leave `[ui]` alone at first — every widget names a ramp step and never
mixes its own colour. A palette assembled colour-by-colour as each widget needed one is how
one visual system gets broken quietly: nothing is ever wrong, the greys just drift apart
until the editor looks assembled rather than designed.

Then check it:

```
cargo test -p typ-app --test theme_files
```

That enumerates every shipped theme at both depths. For a theme in your config directory,
call `typ_core::audit` directly — it takes `&ThemeColors` and a `Kind` and hands back the
list of what to fix, with the measured ratio in each line.
