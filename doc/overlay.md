# WattSeal Overlay

> **This is a fork addition.** The overlay is developed in this repository and is not part of
> upstream WattSeal or maintained by it. Report it — here. Upstream has no widget, so a report about
> the overlay sent there can only be bounced, and a crash that happens only while the overlay runs
> looks like a dashboard bug to anyone who does not know the two are separate programs.

A small always-on-top window that shows live power metrics, so you can keep an eye on consumption
while the dashboard is closed or hidden behind a full-screen game.

## What this is, and what it deliberately is not

`overlay/` builds **one program**, `wattseal-overlay`, that:

- reads the collector's `power_monitoring.db` **read-only** and shows what it finds;
- runs against the **official WattSeal** — it does not need a WattSeal built from this repository;
- **links none of WattSeal's code**. Not its types, not its database layer, not its language enum.

That last one is the design, not an accident. A program that shares a build with WattSeal has to be
rebased whenever WattSeal is, and has to be re-released whenever WattSeal is; a program that only
reads the data does not. The price is written down in
[What it reads, and how](#what-it-reads-and-how) — where independence has a cost — and nowhere else.

### Running it

1. Unpack the release next to WattSeal, so both are in one folder.
2. Make sure WattSeal is running: the overlay reads what the collector writes, and shows a
   placeholder until there is something to read.
3. Start `wattseal-overlay`. It opens on its own.

To have it start with Windows, put a shortcut to the executable in your startup folder — the usual
place is `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup`.

It never writes to the database. A slow or wedged overlay cannot take the dashboard down with it.

## Using the widget

Drag it to move it. Right-click (or, once pinned, use the settings panel) for the menu:

| Menu item      | What it does                                    |
|----------------|-------------------------------------------------|
| `Settings`     | Open the settings panel                          |
| `Pin` / `Unpin`| Lock the position, and on Windows ignore the mouse |
| `Hide overlay` | Quit                                            |

Only one overlay runs at a time. Nothing can see another copy's handle, so the widget takes an
exclusive lock on `overlay_config.json.lock`, beside its config file, and a second copy leaves right
away. A previous version used to stack a second widget on top of the first, both writing the same
file.

### Sizing

The window is always exactly as wide as its content, within the bounds you set:

- **Vertical** (the default) is one metric per line; **horizontal** is a single line.
- Three text sizes and three densities change the padding and the type size.
- `Width` is a **cap**, not a setting. The window takes what the content needs and never more, so a
  value with three short labels does not leave a wide strip of empty window. The cap is what stops a
  long list from running off the side of a small screen.
- The width always lands on a multiple of the platform's character grid, because a window that is
  not pixel-aligned renders soft on some GPUs.

A reading is never cut in half to fit. If the content does not fit the cap, the widget says so.

## What it reads, and how

The overlay has exactly one data source: the collector's `power_monitoring.db`, opened beside the
executable. Reading it is the whole of its coupling to WattSeal, so two rules keep that coupling
survivable.

**It never writes.** The handle is opened `SQLITE_OPEN_READ_ONLY` and issues no pragma that would
modify the file — no migration, no `CREATE TABLE`, no journal-mode change. A missing file is a state
to report, not an empty database to create. A widget that "only reads" is still a second writer
competing for the lock on a file it does not own.

**It reads by column name.** With no shared structs, every query names the tables and columns it
wants: `timestamp`, `duration_ms`, `total_energy_uj`. Column *names* survive a migration; column
*order* does not, and a value read by position after a migration is a plausible number that means
something else. A table or column that is not there is **left out** — so an unfamiliar database
produces fewer numbers, never a wrong one.

### The schema generation

`PRAGMA user_version` is the only thing in the file that says what the tables look like. It is read
on **every tick**, because the collector can migrate underneath a running widget.

> **The schema generation this build reads is 3.**
>
> This is the single number shared with WattSeal's side that is not data, and the one thing that has
> to be kept up to date when upstream migrates. **It is not a rebase** — nothing here has to compile
> against WattSeal — so the cost of a bump is a widget that hides its labels until this line is
> changed, not a merge conflict.

**This is the one place where independence has a price tag.** A shared build would fail to compile
when WattSeal moved this number. With no shared code there is no compiler to notice, so:

- the number is written in the line above, and
- `the_documented_generation_is_the_one_this_build_reads` checks this paragraph against
  `SUPPORTED_GENERATION` in `source.rs`.

Move to a new database generation and both are bumped; the test is what stops one being done without
the other.

### When the generation is not one this build knows

The widget degrades instead of guessing:

| | |
|---|---|
| What it shows | The readings it can confirm — it names the columns it needs, so it keeps working as long as `timestamp`, `duration_ms` and `total_energy_uj` are still there |
| What it drops | Labels, the settings panel, the `Top apps` rows (a process row needs a name from a second table), and the language and theme it would otherwise follow from `ui_settings` |
| Why no labels | It **does** know which metric each number is — it named the table in its own query, so a figure under `cpu_data` really is the CPU. What it cannot know is the **language** and **theme**, because those are the dashboard's vocabulary. Showing no words keeps the widget neutral; showing words would mean words in a choice you did not make |
| What still works | Moving, pinning, quitting — none of which reads the database |
| What it never does | Show a number it cannot confirm, or a number cut in half |

The ordinary "the collector has not started yet" case is reported separately, so the widget waits
with its full presentation rather than flickering into the degraded one on every cold start.

## Appearance it follows

The overlay reads the **language** and the **theme** out of the same `ui_settings` row the dashboard
writes. Changing either in the dashboard is enough — no restart, and no second place to configure
it. Until the dashboard has saved anything, English and the dark scheme are used.

> The language list and the theme names are **copies**, not imports. What keeps the two sides in
> agreement is the **code in the `ui_settings` row**, which is data: the dashboard writes `ZH`, the
> overlay reads `ZH`, and neither needs to know the other exists. The cost is that a language or a
> theme added on one side and not the other is a silent mismatch rather than a compile error — which
> is why the widget treats an unrecognised value as English and the default theme instead of
> guessing.

`ui_settings.theme` stores the theme's **English display name** (`Swimming`), not an internal
identifier. The overlay resolves that name to a light or dark scheme.

There is deliberately no theme picker in the settings panel: the dashboard owns that choice.

## Language

The overlay offers the same five languages as the dashboard — English, German, French, Chinese,
Romanian — and follows whichever one is selected there.

## Transparency

| Mode      | What it does                                                   |
|-----------|----------------------------------------------------------------|
| `Auto`    | Layered window on Windows, per-pixel surface alpha elsewhere   |
| `Layered` | Force the Win32 layered path                                   |
| `Off`     | Fully opaque                                                   |

On Windows the default is the **layered window** (`WS_EX_LAYERED` plus
`SetLayeredWindowAttributes`, see `winlayer.rs`), and that choice is not cosmetic. Many GPUs only
expose an `Opaque` composite mode for their swapchain; on such a machine a window created with
per-pixel alpha renders fully opaque no matter what the application draws, because the compositor
ignores the alpha channel it is handed. The layered path composites at the DWM level instead, so it
works with any GPU and any rendering backend.

The price of that path is that it applies **one alpha to the whole window** — the card and its text
fade together, and no per-element opacity can be expressed. That is why there is a single `Opacity`
setting and no separate text opacity: on a machine whose surface offers no alpha mode, the two
cannot be decoupled, and offering a second slider would only suggest a control that does nothing.
Contrast is tuned with `Bg color` and `Text color`, which change the *hue*, not the alpha.

The same limit decides the **drop shadow**. A shadow is per-pixel alpha, so the layered path — one
alpha for everything — flattens it into a dark ring around the card, and an opaque window has
nothing behind it to fade into either. The card therefore draws its shadow only in a mode that
carries per-pixel alpha. In a mode that cannot, a line under the toggle says so — whether or not the
box is ticked, because an unticked box that cannot be ticked is exactly the case worth explaining.

> Set `WATTSEAL_OVERLAY_LOG=1` to have the overlay write its renderer diagnostics to
> `overlay.log` next to the executable: selected adapter, surface format and the alpha modes the
> surface actually accepted. It is opt-in, so a normal run never writes to disk, and it is the
> fastest way to find out which transparency path a given machine took.

## Pin mode

`Pin` does two things, and only the first one exists on every platform:

1. **The position is locked.** A pinned widget ignores dragging. This is plain application logic and
   works everywhere.
2. **The mouse passes through.** The window gets `WS_EX_TRANSPARENT | WS_EX_NOACTIVATE`, so every
   click — including the right-click that would reopen the menu — lands on whatever is underneath the
   widget, and the widget can never steal focus.

The second part is Windows-only and is controlled by `Pin makes it click-through` in the settings
panel (see `winlayer::click_through_supported`). macOS would need `setIgnoresMouseEvents` and X11 an
input shape; Wayland has no protocol for it at all. Where click-through is unavailable the settings
panel says so rather than offering a toggle that cannot do anything.

Because a click-through widget no longer receives mouse events, it cannot be released by clicking
it. Two ways out exist:

| Path | How |
|---|---|
| Config file | Set `pin_mode` to `false` — **the reliable one**, because it works with no tray and no dashboard |
| Restart | Quitting clears the pin; the widget comes back unlocked |

Locking the position is deliberate: the widget cannot be grabbed by accident while it is meant to
be out of the way.

## Configuration

Everything is persisted to `overlay_config.json`, next to the executable. Missing or unknown keys
fall back to the defaults, so the file is safe to edit by hand.

If a hand-edit leaves the file in a state that will not parse, the next save moves it aside as
`overlay_config.json.unreadable` before writing the defaults over it — so a stray comma costs you a
rename rather than an afternoon of tuning. Delete the copy once you have looked at it.

The defaults below are the ones a **newly created file** starts with. There is one wrinkle:
`overlay_requested` defaults to `true` when the key is *absent from an existing file*, because a file
written before that key existed predates the widget having an on/off state at all. It is `false` in a
new file because "no file yet" and "the overlay is running" have to answer the same way, or pinning
the widget on a fresh install would record something that was never opened.

| Key                  | Default | Meaning                                                              |
|----------------------|---------|----------------------------------------------------------------------|
| `opacity`            | `0.80`  | Window alpha (whole window on the layered path)                       |
| `bg_color`           | `auto`  | Card color swatch, `auto` follows the dashboard's theme               |
| `text_color`         | `auto`  | Text color swatch, `auto` follows the dashboard's theme               |
| `transparency`       | `auto`  | `auto` / `layered` / `off`                                            |
| `shadow`             | `true`  | Drop shadow, drawn only where the surface carries per-pixel alpha      |
| `layout`             | `vertical` | `vertical` (one metric per line) or `horizontal` (single line)     |
| `density`            | `compact` | Padding and spacing: `ultra` / `compact` / `normal`                 |
| `font_size`          | `small` | `small` / `medium` / `large`                                          |
| `show_labels`        | `true`  | Show the metric name next to its value                                |
| `show_units`         | `true`  | Show `W` after the value                                              |
| `abbreviated`        | `false` | Shorten labels (`Total` → `T`, `CPU` → `C`)                           |
| `decimals`           | `1`     | Decimals shown on the values                                          |
| `refresh_secs`       | `1`     | How often the metrics are re-read                                      |
| `always_on_top`      | `true`  | Keep the widget above other windows                                    |
| `width`              | `140.0` | Widest the widget may get; the window stays narrower when the content needs less |
| `overlay_requested`  | `false` | Whether the overlay should be running                                 |
| `pin_mode`           | `false` | Position locked, and click-through if the next key allows it           |
| `pin_click_through`  | `true`  | Whether pinning also makes the window ignore the mouse                |
| `blur`               | `false` | Ask the window system to blur what is behind the card (config file only) |
| `position`           | unset   | Last window position, restored on the next launch                      |
| `metrics`            | total, cpu, gpu, ram, top_apps | Which metrics are shown, in order                      |
| `top_apps`           | `3`     | How many apps the `Top apps` metric lists (clamped to 1–8)             |
| `language`           | unset   | Override the dashboard's language; unset follows it                     |
| `theme`              | unset   | Override the dashboard's scheme (`dark` / `light`); unset follows it    |
| `launch_wattseal`    | `true`  | Start WattSeal when there is no recent sample to read                   |
| `escape_hotkey`      | unset   | The shortcut that releases a pinned widget; unset means `ctrl+alt+o`     |

Deleting the file restores every default.

### Language and theme

`language` and `theme` default to **unset**, which means *follow the dashboard*: the widget reads
`ui_settings` on every tick and changes with it, with no restart. Setting either to a value here
takes it over — your choice wins, and the dashboard stops affecting it.

That override exists because the dashboard is optional. The widget used to have neither key nor a
picker, on the reasoning that the dashboard owned the choice — true while the dashboard was part of
the product, and false the moment this became a standalone program. A user who never opens WattSeal
would have been stuck in English with nothing to click and no error: not a setting, a wall. There is
now a picker for both, in the settings panel.

The panel shows `Automatic` while they are unset and says so in words underneath, because a language
that changed silently because a dashboard happened to open looks exactly like a bug.

### The escape shortcut

The shortcut that toggles the pin is **yours to change**. In the settings panel, under the
click-through toggle, the button shows the current one; press it and then press the combination you
want. `Esc` cancels. Anything with `Ctrl` or `Alt` held plus a letter, a digit or `F1`–`F12` is
accepted; a bare key is refused, because taking a plain letter from every other program on the machine
is not a shortcut, it is a keylogger.

It is configurable because a fixed one cannot be trusted: on the machine this was written on,
`Ctrl+Alt+O` was already owned by another program, and so were fifteen of twenty other plausible
choices. `RegisterHotKey` reports that as a plain failure, so a hard-coded default is an escape hatch
that may not exist.

The panel says whether the shortcut was actually accepted. When it was not, hold `Ctrl`+`Alt` instead —
that route needs no registration and cannot be taken.

| If… | Then |
|---|---|
| the shortcut does nothing | something else has that combination — bind another, or hold `Ctrl`+`Alt` |
| you want it in a file | `"escape_hotkey": "ctrl+alt+p"` — any case, any modifier order |
| you want every setting back | delete `overlay_config.json` |
| you are on Linux or macOS | there is no click-through at all (see above), so the right-click menu always works |

### When the widget has nothing to show

A widget showing nothing, with no reason on screen, is a widget nobody can act on.
So every state where there is nothing to read says so — in your language, and
differently for each situation, because each needs a different action:

| You see | It means | What to do |
|---|---|---|
| `WattSeal not found — put it next to this file` | No `WattSeal.exe` beside the widget | Put it there |
| `Starting WattSeal …` | It was started and has not written yet | Wait a second or two |
| `WattSeal is not running — launching is off in the settings` | `launch_wattseal` is `false` | Start it yourself, or set it back |
| `WattSeal has stopped writing readings` | A database exists; the collector stopped | Restart WattSeal |
| `Newer database — numbers kept, labels unavailable` | A newer schema than this build | Update the widget |

**A stopped collector never shows its numbers.** A frozen reading looks exactly
like a live one, and the whole value of this widget is that the number is *now* —
so the sentence replaces the readings rather than joining them. That is the same
rule the reader already follows for a column it cannot find: fewer numbers,
never a wrong one.

The last row is the exception, because there the numbers are real — they are read
by column name — so that sentence is *added* above them instead.

**The `Width` setting does not apply to a sentence on its own.** The cap exists so
a long process name cannot push the readings off the screen, and a sentence with
no readings has nothing to protect. One sentence *beside* live numbers still
respects the cap, because there the numbers are what the setting is for.

### Starting WattSeal

With `launch_wattseal` on (the default) the widget starts WattSeal for you when the database has
nothing recent in it, so one icon is enough. It checks the **data**, not the process list: a database
whose newest sample is under three seconds old means somebody is already writing to it, so a second
copy is not started. That check matters because WattSeal has its own single-instance lock, and a
second copy exits with an error that would look like the widget had crashed.

Turn the setting off if you start WattSeal some other way — a service, a scheduled task — where
starting another would be wrong. Nothing here writes to the database to make the decision; it is
read the same way as everything else.

The child is left running when the widget closes. WattSeal is the user's program and is stopped far
less often than the widget is.

`overlay_config.json.lock` sits beside it and holds no settings at all: it exists only to be locked,
so that one widget runs at a time.

### Getting a pinned widget back

`pin_mode` locks the position. `pin_click_through` on top of it makes the window ignore the mouse —
so there is no menu, no settings panel, and no close button. **Restarting does not release it**: the
pin is written to the config file and comes straight back.

Three ways out, in the order to try them:

| | |
|---|---|
| **Hold `Ctrl` and `Alt`** | the widget stops ignoring the mouse for as long as you hold them — and stays reachable while its own menu is open, so you can let go before clicking it. Release, and it goes back to ignoring the mouse: being pinned is not undone, just paused |
| **Press the shortcut** | `Ctrl+Alt+O` unless you bound another; it toggles the pin outright |
| **Edit the file** | `"pin_mode": false` in `overlay_config.json`, or delete the file for the defaults |

The held-key route needs no shortcut registration, so unlike the shortcut it cannot be taken by
another program — which is why it is first in the list. The natural way to use it is **hold,
right-click, let go, then click the item you want**: the menu keeps the mouse until it closes, so
letting go of the keys does not take it away mid-click.

While it is stuck the widget **says so on its own card** — it cannot be clicked, but it can still be
read. The line disappears as soon as the pin is released.

### No console window

The release build is a **GUI** program on Windows: no black CMD window opens behind it, and none
stays there for as long as the widget runs. Debug builds keep one on purpose — that is the build you
run while working on it, and a panic you cannot see is worse than an ugly window.

That removes the only place a start-up failure used to be reported, so failures are written to
**`startup_error.txt`, next to the executable** — where someone who just double-clicked an icon is
standing. It is overwritten on every start, so it always describes the run that failed rather than
the last one that did. A successful run deletes it, so it is never there when there is nothing to
report.