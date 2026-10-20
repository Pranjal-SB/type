# Changelog

Versions map onto milestones: `0.<milestone>.<patch milestone>`. See
[`docs/design/architecture.md`](docs/design/architecture.md) §9.

## [0.4.5] - 2026-10-21 (M4.5 Keybindings, shell slice 5 shipped)

The keybindings surface from `docs/design/interface.md` §8 4d and §8b, as a tab beside the
settings, writing `keys.toml` with its comments kept.

### Added
- **The keybindings tab.** `Ctrl+K K`, or `>keybindings` in the palette. Every binding, and every
  action with none, grouped under the menu's rules: what it does, the key as the status bar spells
  it, the layer and whether the row is yours or shipped. Typing filters by action or key;
  `:modified`, `:conflict`, `:unbound` and `:kitty` filter as their names say, and `Ctrl+R` then a
  key filters to what that key does.
- **Recording a key.** `Enter` on a row, or a click on its key, opens a float that shows the keys
  as you press them (a prefix such as `Ctrl+K` waits for one more), says whether every terminal
  sends the chord, it needs the kitty protocol, or the terminal owns it (`Ctrl+I`, `M`, `[`, `H`),
  and names what the chord runs now. `Enter` replaces: the row's old chord is unbound and whatever
  the new one ran loses it. `Esc` writes nothing. `Delete` unbinds, `r` resets the action to its
  shipped keys, `Ctrl+Z` undoes, `Ctrl+O` opens the file; the footer clicks all of them.
- **Live.** A change reloads the keymap the editor runs on, so the `Ctrl+K` menu, the idle hint
  and the palette move with it at once.
- **The keyboard view.** `Ctrl+T` in the tab draws the same bindings on a keyboard under a strip
  of `ctrl`, `ctrl+shift`, `alt`, `ctrl+k ▸` and the F-keys. Bound keys say what they run, free
  keys are outlined, terminal-owned keys are taken, kitty-only keys are marked.
- `config::write_binding` changes one row of `keys.toml` and keeps every other byte, however the
  file spells the chord.

### Not in this release
- Panel layers (`controls.md` §3), so every binding is global and "keep both in different
  layers" waits for a second layer. Giving a free key on the keyboard view an action, which is
  done from that action's row.

## [0.4.4] - 2026-10-20 (M4.4 Settings, half of shell slice 5)

The settings surface from `docs/design/interface.md` §8 4c and §8b, as a tab in the editor area,
writing `config.toml` with its comments kept. Keybindings, the other half of slice 5, are M4.5.

### Added
- **The settings tab.** `Ctrl+K ,`, or `>settings` in the palette. A search row, then every
  setting grouped under labelled rules (`editor › indentation`), two lines a row: the name and
  its control, then what it does, its default and where the value came from. `●` marks a value
  that differs from its default. `@section` and `:modified` filter, as in the picker; at 100
  columns and wider the sections stand in a column of their own, and clicking one filters to it.
- **Changing a value, by key or click.** `←` `→` step through a choice and `Enter` lists them,
  `Space` flips a box, `Enter` edits a number (an out-of-range one is refused in red and nothing
  is written; `Esc` reverts). `r` resets to the default, `Ctrl+Z` undoes the last change, `Ctrl+O`
  opens `config.toml` itself; the footer has the same three to click. Theme, whitespace, indent
  width, layout, adapt and session apply as you change them; colour depth and language servers
  say they wait for the next start before you change them.
- **Writes keep your file.** One value changes and every other byte stays, comments and order
  included, through a temp file and a rename. A `config.toml` that does not parse is refused
  rather than replaced.
- **Tabs that are not files.** A view registered with `typ-registry` opens through `OpenWith` as
  a tab: saving, the language servers, parsing, the watch and the session all skip it.

### Changed
- `toml_edit` joins the dependencies, for the comment-keeping writes. It shares `toml`'s parser
  and writer.

### Not in this release
- A project-level `.typ/config.toml` and the user/project toggle, the colour swatch and chip list
  controls (no setting needs them yet), live preview while arrowing through a choice list, and
  warp labels inside the settings tab.

## [0.4.3] - 2026-10-20 (M4.3 Menus and warp, shell slice 4 shipped)

Interface §0.1 made checkable: anything you can click has a key, anything you can press can be
clicked, and the build fails when an action is missing either. From `docs/design/interface.md`
§5 and §8. That finishes slice 4.

### Added
- **Warp labels.** `Ctrl+K J` puts a two-letter label on every target on screen, the region's
  letter (`s` side, `t` tabs, `b` bar, `f` a float) then its place in screen order, over a
  stepped-back body. Typing a label clicks that target, through the same path a mouse takes;
  `Esc` cancels and a label that names nothing says so. The editor's text has no labels yet.
- **The context menu.** `Shift+F10`, `Ctrl+K .`, the menu key where the terminal reports it, or a
  right-click open the actions for what is under it in a float, by group and with their keys: a
  tab's, the text's, a tree row's, a pane's. A right-click on a background tab or a tree row makes
  it current first, so "close the tab" closes the one you clicked. Type to filter, arrows and
  `Enter`, `Esc`, or click.
- **The menu bar.** `F10` lists every action group over the top row; `←` `→` move, `↓` or `Enter`
  drops a group's actions down with their keys, `Esc` steps back. A click on a group opens it, on
  a row runs it, anywhere else closes it. Hidden, it takes no row, and shown it reflows nothing.
- **A parity test.** Every action needs a keyboard path and a mouse path or the build says which
  is missing.

### Changed
- **Right-click is the context menu everywhere.** On a selection it used to copy; copy is now the
  menu's row, or `Ctrl+C`.
- **The `Ctrl+K` menu's `esc cancels` is on the status bar**, which kept the menu inside 80x24
  with two more rows in it. While any menu has the keys, the left of the status bar says which.

### Not in this release
- Disabled rows: nothing knows yet whether an action can run, so every row runs and one with
  nothing to act on says so, as from the palette. `Alt`+letter for the menu bar: a bare `Alt`
  never reaches a terminal application.

## [0.4.2] - 2026-10-20 (M4.2 Layouts, shell slice 3 shipped)

Layout presets, a layout that follows the terminal's width, and sessions, from
`docs/design/interface.md` §7. That finishes slice 3.

### Added
- **Presets.** Focus folds the dock to its rule, Standard opens it. `Ctrl+K L`, `>layout` in the
  palette, or `Enter` or a click on the `▣` segment at the end of the status bar switch between
  them, and `layout = "focus"` or `"standard"` in `config.toml` picks the one to start in.
- **Adapt to width.** Below 100 columns Standard folds its dock and reopens it when the terminal
  grows. A layout picked by hand, or a dock opened with `Ctrl+B`, is left alone. `adapt = false`
  turns it off.
- **Sessions.** Quitting on a project writes its panes, ratios, tabs, cursors and top lines, the
  dock's width and state, and the preset to the state directory (`TYP_STATE_DIR` overrides it),
  and the next start on that project puts them back. A file that has gone is skipped and named;
  a session that does not parse starts clean with a warning. `typ <file>` reads and writes no
  session. `session = false` turns it off.

### Changed
- **Focus is the shipped default.** A first start shows the file and the dock's folded rule;
  `layout = "standard"` brings back the open tree.
- **Status segments drop by width.** The right half of the bar takes at most half of it; past
  that the line ending and indent go first, then the language, the progress label, the
  percentage and the preset. The position and the problem count never drop, and `F6`, a click and
  the paint all see the same segments.
- **The idle hint drops whole parts** on a narrow bar instead of ending on a separator, and
  `border_focused` is audited on `chrome_bg`: Slate's moved to its lifted blue, Dracula's to cyan.
- **The `Ctrl+K` menu fits 80x24.** Its groups pack into columns, and a run of digits bound to
  one action folds into one row (`1…9  focus pane n`); a rebind that breaks the run splits it.

### Not in this release
- The Wide preset waits for a right dock, Review for the review lane, and arrowing through
  presets does not preview them, since with two the switch is the preview.

## [0.4.1] - 2026-10-03 (M4.1 Docks, shell slice 3 continued)

The left dock from `docs/design/interface.md` §2 and §5, zoom, and focus that reaches every
region and comes back the way it went. Layout presets and sessions follow in v0.4.2.

### Added
- **The dock.** The tree lives in a left dock. `Ctrl+B` opens and focuses it when it is folded,
  focuses it when it is open, and folds it when it has focus, focus going back where it came
  from. Folded, it is one column: a rule with `▸` at its top, and a click on it opens the dock.
  Drag its rule, or `Ctrl+K =` with the dock focused and the arrows, to resize it between 12
  columns and half the screen; `=` puts it back.
- **Zoom.** `Ctrl+K Z` gives the focused region, the dock or a pane, the whole body, and again
  puts the layout back exactly. The `◰` at the right of the dock's heading and of every tab row
  does the same, and so does a double-click on the dock's heading. The status bar says `zoomed`
  while it is.
- **The status bar takes focus.** `F6` walks the dock, the panes in reading order, then the
  status bar. There `←` `→` move between the segments that do something and `Enter` does it:
  the position opens goto-line, the problem count jumps to the next problem. A click on a segment
  does the same thing. `F8` jumps to the next problem from anywhere.

### Changed
- **`Esc` walks back.** In the dock or the status bar, with nothing of its own to cancel, `Esc`
  returns to the region focus came from, up to eight deep, skipping a pane that has closed or a
  dock that has folded. In a pane it never moves focus: the editor is where `Esc` brings you
  back to, not somewhere it takes you from.
- **The idle hint says `F6 focus`**, read from the keymap, as are its `Ctrl+S` and `Ctrl+Q`. It
  said `Tab focus` long after Tab became indent.
- **The contrast audit measures chrome text on `chrome_bg`**: `status_bar_accent` and `fg` at the
  content floor, `status_bar_inactive_fg` at the quiet one. Slate's status-bar accent moved to
  its lifted blue (`accent_bright`) and Dracula's to cyan, both under the floor before.

## [0.4.0] - 2026-10-03 (M4 Panes, shell slice 3 begun)

The first part of the layout model in `docs/design/interface.md` §2, §5 and §7: lines between
regions instead of boxes around panels, and the editor area split into panes. Docks and layout
presets follow in v0.4.1 and v0.4.2.

### Added
- **Panes.** `ctrl+k \` splits the current pane to the right and `ctrl+k -` splits it downward.
  The new pane opens empty with the file picker up. `ctrl+k x` closes the current pane, asking
  first if a tab in it has unsaved work, as closing those tabs would; the last pane never closes.
  A file is open in one pane at a time: opening it again focuses the pane that has it.
- **Moving between panes.** `ctrl+k 1` to `9` focuses a pane by its number, which leads its tab
  row and counts in reading order. `alt+arrow` focuses the pane beside the current one,
  preferring the one level with the cursor. A click anywhere in a pane focuses it, and the wheel
  scrolls the pane under the pointer without moving focus.
- **Resizing.** Drag the rule between two panes, or press `ctrl+k =` and use the arrows; `=`
  evens the split and `enter` or `esc` leaves. No pane goes under 10 columns or 3 rows.
- **One watch for the workspace.** Outside writes anywhere under the project are noticed by a
  single recursive watch, built and filtered off the render thread by the same ignore rules the
  file picker walks with. A file outside the project, or one it ignores, still gets a watch of its
  own.

### Changed
- **Lines, not boxes.** No panel draws a frame any more. A single rule divides the tree from
  the editor, the tree's heading is a labelled rule (`── project ──`, accent and bold while it
  has focus), and every pane is divided from its neighbours by one rule. The editor gains the
  column and two rows its box used.
- **The tab row is always shown**, one tab included: with the box gone, the tab is the
  editor's label and where focus shows.
- **A tab switch does no file watching.** It used to rebuild the file's OS watch on the render
  thread.

## [0.3.3] - 2026-09-30 (M3.3, shell slice 2)

The second slice of `docs/design/interface.md`: `ctrl+k` as a door to everything a terminal
cannot give its own chord, built on the mechanism `controls.md` §2 has described since v0.2.9.
Closes gaps 52 and 53.

### Added
- **The `ctrl+k` menu.** Pressing `ctrl+k` docks a menu above the status bar with every binding
  under it, in columns by group, each row its key and what it does. The second key runs a row
  directly; arrows and `enter` run the highlighted one; a click runs the row under it and a
  click elsewhere cancels; `esc` cancels. It is generated from the keymap, so a rebind changes
  it. On a short terminal it takes at most a third of the rows and scrolls.
- **The teaching line.** After a row runs from the menu, the status bar shows its chord, such
  as `ctrl+k f · search the project`, for two seconds. It goes with the first event after that
  and never sets a timer, so an idle editor is not woken for it.
- **Sequence bindings.** `keys.toml` can bind `"ctrl+k x"`, each step canonicalised like any
  chord. A key bound alone and as a prefix is an error naming both; three steps is an error.
- **Default `ctrl+k` rows** for what has no chord every terminal sends: `f` project search, `p`
  command palette, `g` goto line, `w` close tab, `d` goto definition, `h` hover, `r` restart the
  language servers. The direct chords stay.
- **`open_menu`**, so the palette opens the menu where a terminal or multiplexer eats `ctrl+k`.
- **`shift+f6`** walks focus backwards; `focus_previous` names it.
- **A startup warning** for a `keys.toml` binding the terminal cannot send without the kitty
  keyboard protocol, such as `ctrl+shift+k` outside Windows.

### Changed
- **Every action has a description and a group.** The command palette shows and matches the
  description ("add the next match of the selection") rather than the config name
  (`select_next_occurrence`), which `keys.toml` still uses.
- **`esc` goes back.** In a panel with nothing of its own to cancel, `esc` returns focus to the
  panel it came from; in the editor it still collapses the selection and never moves focus.

## [0.3.2] - 2026-09-29 (M3.2, shell slice 1)

The first slice of `docs/design/interface.md`: the contrast audit's fixes as theme slots, focus
shown by receding, and the picker and hover box drawn as floats.

### Added
- **Three theme slots**, each with an audit rule checked at truecolor and at 256 colours:
  `receded_fg` (text in a panel without focus), `float_border` (the border of a float that has
  the keys) and `scrim` (what a modal float dims the screen to).
- **Focus recedes.** The panel without focus draws its text in `receded_fg` with no syntax
  colour; its selection and cursor line stay. The focused panel's title and the active tab are
  accent and bold.
- **Floats.** The picker and the hover box are rounded boxes with their name and `esc` cut into
  the top border and a clear cell of page around them. The picker dims everything behind it
  except the status bar; clicking its `esc` closes it. The hover box sits a row clear of the
  cursor and dims nothing.

### Changed
- **`border` holds 3:1 against both surfaces.** It was 1.3 in Slate. The name stays, so theme
  files keep working; every shipped theme has a new value.
- **The indent guide is decoration.** It no longer takes the gutter's floor; it has to be
  visible and quieter than a rule. Slate's is `#3b4557`.

### Fixed
- The picker and the hover box no longer show the editor's text through their blank rows.

## [0.3.1] - 2026-09-29 (the v0.3.0 audit)

An audit of the whole tree found 80 defects. 73 are fixed here, and the first release to carry
M3's language server support is this one: 0.3.0 was never tagged.

### Fixed
- **Two ways to lose unsaved work.** A confirmed tab close armed the tab that took its place, so
  the next `Ctrl+W` discarded it without asking. `quit` and `close_tab` from the palette could
  never finish.
- **A file name could drive your terminal.** Control characters in a path or a search result
  reached the terminal raw, so a cloned repo could write to your clipboard over OSC 52.
- **The editor no longer exits** when a file it has open is replaced by a binary, when Enter hits
  a binary in the tree, or when a worker thread panics.
- **Multiple cursors:** a caret at the start of a selection could edit the wrong range, merging
  lost direction, and every motion was quadratic in the cursor count. 4000 cursors move in
  under a millisecond and Ctrl+Shift+L on 4000 hits takes about 4 ms.
- **Saving** opens its temp file exclusively, never makes it world-readable, and removes it when
  the save fails.
- **Language servers:** answers are matched to the server that sent them, a reopened file is
  announced as opened, progress requests are answered, a `ContentModified` reply is asked again,
  and a refused request says so.
- **Wide characters** are measured in cells in the status bar, hover box and picker.
- Undo back to the saved text reads as clean, a no-op edit keeps the redo stack, a mis-spelled
  chord in `keys.toml` is an error, an unreadable config file is a warning, and a background tab
  notices when its file changes on disk.

### Changed
- A closed terminal input ends the session with exit code 1 rather than hanging.
- `PanelEvent::Quit` is gone. Quitting is an app action.

## [0.3.0] - 2026-08-29 (M3, code intelligence)

TYPE talks to language servers. Diagnostics appear as you type, `F12` goes to a definition,
`Alt+H` says what a thing is, and the status bar says what the server is busy with. rust-analyzer
and taplo are configured without a config file.

`landscape.md` has ranked the absence of LSP as the only *instant* bounce since it was written,
and everything below it on that list had already shipped. This is the milestone where the
architecture's central bet — that "all the frontier IDE features" is mostly three protocol
clients — either pays out or does not.

### Added
- **`typ-lsp`**, an eleventh crate at the bottom of the graph beside `typ-syntax` and `typ-find`.
  One reader thread and one writer thread per server, no async runtime. It speaks char offsets
  and never mentions graphemes; `typ-buffer` owns that conversion, because that is where grapheme
  logic already lives.
- **Documents sync to the server**, one notification per frame rather than one per keystroke. The
  event loop already batches input, so a ten-key burst is one `didChange`.
- **Diagnostics**, undercurled under the text in the severity's colour, marked in the gutter, and
  counted on the status bar. They shift forward through your edits rather than sitting under the
  wrong words until the server catches up.
- **Goto-definition** on `F12` and **hover** on `Alt+H`, both also in the command palette. A
  reply that arrives after the cursor has moved is discarded rather than acted on.
- **Indexing progress** in the status bar, so the first minute on a real project looks like work
  rather than like a broken editor.
- **A backend that draws styled underlines**, and owns the frame boundary that `run.rs` used to
  write by hand around every draw.
- **`[[language]]` in `config.toml`**, with rust-analyzer and taplo compiled in as defaults. An
  entry replaces a default of the same name; a language TYPE has never heard of is added.
- **`restart_language_servers`**, for after you install the thing it said was missing.

### Fixed
- **A form feed is no longer a line break.** ropey's `unicode_lines` is on by default and makes
  `U+000B`, `U+000C`, `U+0085`, `U+2028` and `U+2029` line breaks, per Unicode Annex #14. Every
  tool TYPE talks to counts a line feed and nothing else — rust-analyzer's line index, ripgrep,
  git — so a file containing a form feed put every diagnostic *and every project-search hit*
  below it on the wrong line. The search half had been wrong since v0.2.8.
- **The weekly perf tripwire runs all nine budget files**, having run two of seven. The five it
  skipped included the one holding the sub-100 ms cold start that the workflow's own header names
  as its reason for existing.

### Deliberately not in this release
- **Completion and signature help.** v0.3.1. The completion popup is the largest new UI surface in
  M3 and deserves its own release.
- **Pull diagnostics.** v0.3.1, moved there once the premise for including them turned out to be
  false: rust-analyzer sends its *native* diagnostics by push unless the client declares
  `textDocument.diagnostic`, so v0.3.0 already gets the fast half. Declaring that capability
  without a working pull path would turn it **off**, so the declaration and the implementation
  are one change and that change is a release.
- **Rename, code actions, formatting, references, document symbols.** v0.3.2 — all five apply
  workspace edits, which is one mechanism, so they ship together.
- **Several servers against one document.** One server per language is enough to prove the
  client; routing by capability and merging two sources of diagnostics is its own problem.
- **Incremental document sync.** Full sync cannot desync and is the smaller change. The cost was
  measured rather than assumed: the render thread pays 17 ns per notification, because it hands
  the writer thread a snapshot instead of a string.
- **Peek definition, and semantic tokens.** Additive later; neither blocks anything.

### Known
- Starting a server costs about 8 ms on the thread that asks for it, once per server. Invariant 7
  says subprocess work goes off-thread and this is subprocess work; it lands on a file-open frame
  rather than a keystroke, and the fix is an asynchronous `Client::start`. Gap analysis 61.
- Hover has no dwell-on-mouse, and a crashed server is restarted immediately rather than after a
  pause. Both want a timer the event loop does not have — it blocks on a bare `recv()` — which is
  the same thing pull diagnostics need, and all three arrive together in v0.3.1.

## [0.2.10] - 2026-08-26 (M2.9 loose ends)

A patch release for the bug tabs created and the documentation eight releases left behind.

### Fixed
- `typ a.rs b.rs` opens both. Everything after the first path was dropped without a word; the
  gap analysis had called it "honest until tabs exist, a real bug the moment they do" and tabs
  shipped without it. The first path stays active, the way `vim a b` does, and every path gets
  the same missing-parent check the first one always got.

### Documentation
- The crates.io front page said v0.2.1 and listed syntax highlighting, the clipboard and tabs
  as absent. All three had shipped.
- `controls.md` said "design, not built". Its tier analysis is why the command palette is
  reached by typing `>` rather than `Ctrl+Shift+P`; the prefix *mechanism* is still unbuilt, and
  it now says which half is which.
- `landscape.md` said the fuzzy finder does not exist. It shipped at v0.2.8.
- The docs index was missing `landscape.md` and undercounted the published crates.

### Known and deliberately not fixed
- A startup warning for Enhanced-tier bindings the terminal cannot deliver, which `controls.md`
  §1 requires, needs either a terminal query against a 100 ms cold-start budget or the raw input
  layer that does not exist yet. An environment heuristic would be wrong in both directions, and
  a warning that is wrong is worse than none. Gap analysis 52 and 53.

## [0.2.9] - 2026-08-26 (M2.9, workspace-lite)

M2.8 made moving between files the primary interaction and `App` held exactly one buffer, so
every `Ctrl+P` discarded the file you were on. Finding files got easy and keeping them got no
easier, which is a worse balance than before the picker existed. Now a file opens into a tab.

The command palette rode along because it was nearly free: every editing primitive was already a
named action and every binding already a table row, so the palette is a mode on the picker rather
than a feature.

### Added
- Opening a file adds a tab instead of replacing the buffer. Opening one that is already open
  switches to it, compared canonically so `./a.rs` and `a.rs` are one tab.
- A tab bar above the editor, drawn only when more than one file is open. The active tab takes
  the editor's own colours, which is what joins it to the pane underneath.
- `Ctrl+PageDown`/`Ctrl+PageUp` and `Alt+.`/`Alt+,` move between tabs, `Alt+1`..`Alt+9` jump,
  `Ctrl+W` closes. Two spellings because `controls.md` §1 does not list the page keys among the
  universally deliverable keys and does list `Alt+punctuation`.
- Clicking a tab selects it, its `×` closes it, and a middle click closes the tab under the
  pointer.
- A command palette over every named action, reached by typing `>` in the file picker or with
  `Ctrl+Shift+P`. Each row shows the key that runs it.

### Changed
- Closing the active tab activates the tab used most recently, not a neighbour.
- The confirmation on opening a file over unsaved changes is gone, with the reason for it: a new
  tab discards nothing. The question moved to closing a tab, where the work is actually at risk.

### Fixed
- Project search reads **every** unsaved buffer from memory, not only the visible one. Typing in
  one file, switching tabs and searching for what you typed found nothing.
- Closing a tab from its `×` asked nothing before discarding unsaved work, while `Ctrl+W` always
  asked. Both go through one gate now, and the pending answer names which tab it is about.
- A parse result now goes to the tab that asked for it. Two freshly opened buffers are both at
  revision 0, so the second file opened was never parsed at all; and switching tabs mid-parse
  discarded the answer while leaving the revision recorded as requested, so nothing asked again.
- Rebinding quit silently broke its own confirmation — the dispatcher compared the literal chord
  `"ctrl+q"` rather than the action.

### Measured
- Tab switch with 20 open: 640 µs, against a 16 ms keystroke budget. A probe put `watch_file`
  plus its drop at 909 µs, so the cost is the OS file watcher, rebuilt on the render thread. It
  is recorded rather than tuned: the milestone that watches a workspace replaces the path.
- Palette open plus query: 95 µs. Ranked on the render thread deliberately — sixty static names
  are cheaper than a round trip to the worker.
- Tab bar with 200 tabs: 88 µs, against 2.9 µs with two.

## [0.2.8] - 2026-08-25 (M2.8, find)

You can reach any file in the project without knowing where it is, and find any string in it
without leaving the editor. A fuzzy picker floats over the body; a parallel, gitignore-aware walk
feeds it; ripgrep's searcher answers the second question.

### Added
- `Ctrl+P` opens a fuzzy file picker over the editor. Type to filter, and the characters that
  matched are highlighted.
- `Ctrl+Shift+F` searches the project's text. Results show `path:line` and the matching line;
  Enter opens the file at that line and column.
- Both work with the mouse: click a row to open it, wheel to scroll, click away to dismiss.
- The walk respects `.gitignore`, `.ignore`, nested ignore files and the global one — and applies
  them outside a git repository too, which `ignore` does not do by default.
- The file you have open is searched from memory rather than from disk, so a project search finds
  unsaved edits instead of reporting the previous version of the file.
- Binary files are skipped, and a very long line is truncated rather than shipped whole.
- Two new crates: `typ-find` (the walk, the ranking and the search) and `typ-picker` (the widget).

### Changed
- `PanelEvent::OpenFile`'s `line` and `col` are now honoured. They have been on the event since
  M1 and nothing read them; harmless while the only producer was the file tree, which always
  means the top of the file, and wrong the moment a search result names a line.
- Search, replace and goto-line moved out of `app.rs` into a module of their own. The file was
  948 lines and this milestone adds overlay ownership and worker plumbing to it.

### Fixed
- `typ-syntax` shipped in v0.2.7 with no `description`, `keywords` or `categories`. crates.io
  rejects a package without them, which was discovered during the release rather than before it.
  A test now checks every crate, and a second checks that every path dependency names the
  workspace version.

### Performance
Release build, best of five, on the development machine:

| | |
|---|---|
| Rank 50,000 paths, worst-case one-character query | 4.75 ms |
| Rank 50,000 paths, typical six-character query | 2.60 ms |
| Walk 10,000 files | 17.8 ms |
| Search 10,000 files | 10.0 ms |
| Cold start | 16.4 ms |
| Binary | 4.93 MB, from 4.88 MB |

The walk is parallel and that is not an optimisation: a serial walk of a 37,586-file tree
measured 2596 ms against 94.7 ms. Nothing walks at startup — the index is built when the picker
first opens.

## [0.2.7] - 2026-08-25 (M2.7, parse)

The editor understands the file it is showing. A tree-sitter grammar parses the buffer on a
worker thread and the result is painted through the theme, so code is parsed rather than
pattern-matched.

Scoped larger than it shipped. Terminal capability work — the kitty keyboard protocol, following
the terminal's own light and dark — was cut: it needs an escape parser that reads replies from
the terminal, which is a backend question still open, and it buys polish rather than capability.

### Added
- Syntax highlighting for Rust, TOML, JSON, YAML and Markdown, by tree-sitter.
- Grammars are compiled into the binary. No runtime directory to locate, no C compiler to
  install, nothing to fetch — the two failure modes the field's other approaches carry.
- Parsing runs on a worker thread and coalesces: a burst of typing costs one parse per parse,
  not one per keystroke, with no debounce timer and so no latency floor on the common case of
  typing one character and stopping.
- Injections. A fenced code block in a Markdown file is highlighted as the language its fence
  names, YAML and TOML frontmatter as themselves, and a paragraph's emphasis and inline code
  through Markdown's second grammar.
- A `[syntax]` table in all six shipped themes, thirteen capture names each. Names resolve
  through their dotted prefixes, so `keyword.control` finds `keyword` and a theme colours a
  grammar it was never written for.
- `WT_SESSION` is read as a truecolor claim. Windows Terminal does not set `COLORTERM`, so a
  stock install had been quantising every theme for nothing.
- Cold start and time-to-first-highlight are measured, alongside the frame and keystroke budgets
  that already were.

### Fixed
- A parse belonging to a buffer that has since been closed is no longer applied to the one that
  replaced it. Opening a second file before the first finished parsing painted it in the
  previous file's colours until the next parse landed.
- Markdown paragraphs reach the inline grammar. The injection query shipped by the grammar crate
  is written for another editor's defaults, and under this highlighter it produced an empty
  range: the layer was created and parsed nothing, silently.

### Performance
- A highlighted frame deep in a 50k-line file: 864 µs, against a 16 ms budget and 511 µs for the
  same frame unhighlighted.
- Cold start over a real repository: 7.2 ms, against 100 ms.
- Loading six grammars and compiling their queries: 102 ms, once per process, on the worker —
  so it costs latency to the first highlight and nothing at startup.
- The binary grows from 1.19 MB to 4.87 MB, which is what five compiled-in languages cost.

## [0.2.6] - 2026-08-24 (M2.6, ship)

The milestone that makes the previous five reachable. v0.2.5 published one Linux archive and it
did not start on most Linux; getting the editor at all meant having a Rust toolchain. Linux is
static musl now on two architectures, there is a one-line installer for every platform, and a
release cannot publish itself until it has downloaded its own artifacts and run them.

Nothing in the editor changed, with one exception the measurement forced: the allocator on musl.

### Added
- **Static Linux builds, x86_64 and aarch64.** `x86_64-unknown-linux-musl` and
  `aarch64-unknown-linux-musl`, statically linked, with no glibc version to be too new for. One
  file covers Ubuntu, Debian, RHEL, Alpine, Void and NixOS regardless of age, and aarch64 covers
  Graviton, Raspberry Pi, Asahi and arm64 servers — a platform TYPE did not build for at all.
- **`install.sh` and `install.ps1`.** One line each, POSIX `sh` and Windows PowerShell 5.1. Both
  verify the published SHA-256 before anything is written outside a temporary directory, and
  neither asks for `sudo` or Administrator. Tested under dash, busybox ash, PowerShell 5.1 and 7.
- **Releases verify themselves before they publish.** Every archive is downloaded back off the
  release, checksummed, unpacked, executed, and asserted to report the version its tag claims.
  A release that fails stays a draft.
- **Build provenance** on every archive, so `gh attestation verify <file> --repo Pranjal-SB/type`
  proves it came out of this workflow.
- **`THIRD-PARTY-LICENSES.md` ships inside every archive** — the notices MIT and Apache-2.0
  require to travel with a binary containing their code. 106 crates, generated at package time.
- **Weekly perf runs.** The budgets in `tests/perf.rs` were enforced only when someone
  remembered to look, which had been true since v0.2.1.
- **`cargo binstall typ-editor`** fetches the release archive instead of compiling it. It was
  broken by default — binstall interpolates the crate name, `typ-editor`, and the archives are
  named for the binary — and on Linux it is pointed at the static build rather than the glibc
  one it would otherwise prefer.
- Dependabot on cargo and GitHub Actions, `SECURITY.md`, typo checking, zizmor on the workflows,
  and PSScriptAnalyzer on the PowerShell.

### Fixed
- **The published Linux binary did not start on most Linux.** v0.2.5 shipped one Linux archive,
  linked against glibc 2.39, which fails with `version 'GLIBC_2.39' not found` on Ubuntu 22.04,
  Debian 12, RHEL 9 and Amazon Linux 2023. The static musl build is now the one the installer
  and the README point at.

### Removed
- **v0.2.5's Linux archive was deleted from its release page**, with its `.sha256`, and the
  notes say why. It could not start on most Linux, and a missing asset is a better outcome than
  one that fails with a loader error. Immutable releases do not apply retroactively, so it could
  still be removed; releases published from here on cannot be corrected this way.

### Changed
- **mimalloc is the allocator on 64-bit musl.** musl's own `mallocng` cost `find_all` 4.11 ms →
  10.17 ms on a 50k-line file, against a 16 ms budget with the least headroom in the project.
  mimalloc returns it to 4.23 ms. Measured, best of five, on one host — and mimalloc rather than
  the jemalloc ripgrep uses because jemalloc's autotools build misdetects C11 atomics when it
  cross-compiles to aarch64-musl on an arm64 runner. The trade is resident memory: mimalloc uses
  more than either alternative, which for an editor is the cheaper side.

## [0.2.5] - 2026-08-23 (M2.5, colour)

The milestone that turns the palette into an artifact. A theme is a file, six of them ship, the
terminal's colour depth is detected and degraded to, and indentation stops being a number the
status bar asserts without measuring.

### Added
- **Themes are TOML files.** A named `[palette]` and a typed `[ui]` table of 27 slots. Six ship
  embedded — Slate, Catppuccin Mocha and Latte, Dracula, Rosé Pine, Tokyo Night Storm — and a
  file in `<config>/themes/<name>.toml` wins over the embedded copy of the same name. Every key
  is optional; an unset one keeps the shipped default. An unknown key is a load error with a
  did-you-mean rather than a silently wrong colour.
- **A contrast rubric, public as `typ_core::audit`.** Every shipped theme is checked at truecolor
  **and again after degradation to 256 colours** — the half nobody else checks. Quantising moves
  every colour by a different amount, and three of the ported palettes turned out to lose their
  second surface entirely at 8-bit while reading fine at 24.
- **Terminal colour-depth detection**, with `color_depth` in `config.toml` as the escape hatch.
  Nothing in the environment separates a tmux that forwards truecolor from one that mangles it,
  so that one is a setting rather than a cleverer guess.
- **Indent width is measured, not assumed.** VS Code's `guessIndentation` ported, including the
  alignment rule that stops `const a = b + c,` reading as a 6-wide indent, and a deterministic
  tie-break. The status bar now reports what was measured.
- **Whitespace rendering** — `none | trailing | selection | all`, defaulting to `selection`.
- **Indent guides**, including through blank lines, spaced by the detected width.

### Changed
- **Contrast floors depend on the ground a theme declares.** WCAG 2.1's ratio is not
  perceptually uniform across polarity: measured over 1,066 colour pairs from 97 published
  palettes, a dark ground returns about 2.5x the ratio of a light one at equal legibility. Under
  a single flat floor the rubric passed Slate's gutter at 3.35 and rejected Catppuccin Latte's at
  2.83 — while Latte's is roughly twice as legible. It was rejecting the better colour, for five
  dark themes against one light one.
- **Slate is retuned.** Its gutter and inactive status text moved most, which is the finding
  rather than a side effect: they had been tuned against a floor that was measuring the wrong
  thing.
- Panels draw as full boxes sharing one edge, and chrome sits on its own surface, so the sidebar
  and the editor stop reading as one space.

### Fixed
- The current line's tint now covers its line number instead of stopping at the gutter.

## [0.2.4] - 2026-08-16 (M2.4, live)

The milestone that makes the editor live and correct: able to be woken by something other than
a keypress, able to notice the file changing underneath it, and able to save without quietly
destroying what it did not write.

### Added
- **The event loop blocks on one channel** rather than on the terminal. A worker thread can
  now deliver a result without waiting for the user to press a key, which is the prerequisite
  for tree-sitter at v0.2.5 and LSP at v0.3.0. A detached thread pumps crossterm into the same
  channel.
- **File watching.** A file changed on disk while open was a data-loss bug: TYPE neither
  reloaded nor warned, and the next save overwrote the other writer. Now a clean buffer
  reloads silently, a dirty one warns and is left alone, and a deleted file leaves the buffer
  standing as the only copy. The watch is on the parent directory, because a rename-over
  destroys the inode a file watch is pinned to.

### Changed
- **A frame is drawn because state changed, not because the loop went round.** The loop drains
  everything queued behind the event it woke on and draws once. On a 50k-line file an idle
  wakeup costs 425 ns against a 513 µs frame, and a 30-event burst dispatches in 268 µs and
  draws a single frame.
- Scroll coalescing folds only consecutive wheel events over the same panel, in the batch. The
  version it replaces read ahead in the queue and dropped anything that was not a scroll, so a
  key pressed mid-flick vanished.

### Fixed
- **Saving preserves line endings, symlinks and mode bits.** CRLF is normalized to LF in the
  rope and written back on save, so a typed newline no longer puts an LF into a Windows file.
  A symlink is resolved and written through rather than replaced by a regular file. The
  original mode is carried onto the temp file before the rename, so an executable script stays
  executable and a `0600` file is never briefly world-readable.
- **The parent directory is fsynced after the rename.** A rename is not durable until the
  directory entry naming it is. None of ttt, TermIDE or Fresh does this.
- Resize is handled. It was harmless only while the loop repainted unconditionally; once
  redraw became damage-driven it was a frozen screen.

## [0.2.3] - 2026-08-16 (M2.3, polish)

The milestone that makes TYPE *look* like a finished program. M2.2 made it usable and did
nothing for how it reads; this is the answer to "feels very prototypey compared to ttt and
TermIDE". Every item closes a defect from the gap analysis's "Reads as unfinished" class, the
class the first audit had no rows for, because it compared feature lists and never asked
whether the thing looks finished.

### Added
- **A gutter, with line numbers.** Built as an ordered list of components rather than a
  line-number column: `LineNumbers`, `Spacer`, and `Diagnostics`/`Diff` reserving their cell
  and drawing nothing until M3 and M5 fill them in. Right-aligned, 1-based, width taken from
  the whole buffer so the text never shifts sideways at line 100. Relative numbering exists
  behind a field for the vim layer, off by default.
- **A truecolor theme.** `ThemeColors` goes from ten flat fields to twenty-four, modelled on
  Helix's `ui.*` scopes and drawn from one named ramp at one hue. Contrast is checked by test
  against WCAG ratios rather than by eye: body text holds 7:1, the gutter 3:1, every
  diagnostic 4.5:1, and error and warning are separated by lightness so they survive
  deuteranopia. The diagnostic colours ship unused so a theme file written at M2.5 does not
  get a breaking change at M3.
- **Current-line highlight, distinguishable primary selection, matching brackets.** Only
  empty selections tint their line. The bracket search is bounded by the viewport plus a
  margin and gives up rather than exceeding it, because it runs on the render path.
- **`Ctrl+D` selects the next occurrence**, `Ctrl+Shift+L` selects all of them. Case
  sensitive, unlike `Ctrl+F`: matching an identifier is a different job from finding prose.
- **`Ctrl+G` jumps to a line**, centring it rather than merely scrolling it into view.
- **Seven status segments** instead of three: name, filetype, line ending, indent, cursor
  count, position, percentage, each with an emphasis, so unsaved changes and a cursor count
  above one are accented rather than lost in a strip of even text.
- **A log file.** `TYP_LOG` names a path; unset, logging costs a branch. A TUI owns the
  screen, so `println!` debugging is unavailable by construction.
- **Line-ending detection.** The status bar stops claiming every file is LF. Preserving it on
  save remains M2.5's half of the job.
- **The tree colours directories apart from files**, so the shape of a project is readable
  without reading the names.

### Changed
- The tree's selected row uses the primary selection colour: it is the one thing being
  steered, the same job the editor's primary does.
- Perf tests take a mutex and the `find_all` budget takes best-of-five. Adding two render
  benchmarks made `InsertChar` read 32 µs against the 1.9 µs it actually costs. Cargo runs
  tests in parallel threads, so the older tests were being timed while a sibling saturated a
  core. It took a bisect against v0.2.2 to establish that the 20x was a phantom.

### Fixed
- The render path is measured for the first time. Architecture §4 budgets keystroke *to
  painted glyph*, and every perf test measured edits only. A frame deep in a 50k-line file
  costs 439 µs against 16 ms.

## [0.2.2] - 2026-08-15 (M2.2, usable)

The milestone that makes TYPE able to edit TYPE. Every item here closes a defect found by
reading the tree rather than by planning. None was named by any earlier plan, which is the
finding underneath the findings.

### Added
- **Clipboard.** Copy, cut and paste, keyboard and mouse. `Ctrl+C`/`X`/`V`, the `Insert` trio,
  right-click a selection to copy, middle-click to paste. Copying emits OSC 52, so a copy over
  SSH reaches the machine you are sitting at rather than the one you are logged into.
  Multi-cursor copy joins selections with newlines and pasting that many lines back gives one
  to each cursor.
- **Tab indents, Shift+Tab outdents.** At a caret Tab goes to the next tab stop; with a
  selection it shifts every line the selection touches and leaves the selection standing.
  Blank lines are skipped, and outdent takes a partial level to zero rather than to minus one.
- **`typ newfile.md`** opens an empty buffer that `save` creates. A missing parent directory is
  still an error, and still fails before the screen is taken.
- **Bracketed paste.** A paste is one edit and one undo step rather than one per character, and
  a chord inside pasted text is inserted rather than executed.
- Gap analysis: a defect audit of the tree and a measurement of TYPE against VS Code, Zed,
  Sublime, Helix, TermIDE and oh-my-pi, plus the install and first-launch design.

### Changed
- **Focus cycling moves from `Tab` to `F6`**, with `Ctrl+Tab` bound alongside. Tab had to be
  freed for indent. `F6` rather than `Ctrl+Tab` as the primary because without the kitty
  keyboard protocol a terminal cannot tell `Ctrl+Tab` from `Tab`. Tab now does nothing in the
  file tree until M4 names the tree's own actions; `F6` works from both panels.

### Fixed
- **Opening a file no longer discards unsaved changes.** `needs_close_confirmation` had one
  caller, so `Ctrl+Q` guarded your work and clicking a file in the tree threw it away.
- **The undo stack is capped at 1000 steps.** It had no cap and no eviction, so every version
  of the file was retained for as long as the editor was open.
- Architecture §10 no longer lists the config format as an open question. It is TOML, decided
  when `keys.toml` shipped.

## [0.2.1] - 2026-08-15 (M2.1, correctness)

### Fixed
- `InsertChar` cost 33.8 ms on a 50k-line file against a 16 ms budget. `TextBuffer::line_text`
  returned an owned `String` and three callers looped it over the whole buffer; a borrowing
  accessor replaced it. Undo and redo, and whole-buffer search, were the other two.
- Undo took back one character at a time. Consecutive edits of the same kind now coalesce, and
  anything that is not an edit ends the run — structural, not on a timer.
- Undo restores the selections the edit was made from rather than wherever clamping left them.

### Added
- Performance tests behind the budgets in architecture §4, run by hand with `--release
  --ignored`. A budget stated in prose with nothing measuring it is how the above shipped.

## [0.2.0] - 2026-08-15 (M2, editing is real)

### Added
- Selections as the only cursor model — a caret is an empty selection, and every editing path
  works for one cursor or thirty without branching.
- Multiple cursors: `Ctrl+Alt+↑/↓`, `Alt`+click.
- Word-wise motion and deletion, select-all, select-line, collapse-to-one.
- Mouse selection: drag to select, click twice to select a word.
- Horizontal scrolling that never splits a wide grapheme.
- Literal search and replace-all through a status-bar prompt, smart-case.
- Every editing primitive is a named `Action`; every key binding is a table row.
- `keys.toml` rebinding, from the platform config directory or `TYP_CONFIG_DIR`. A bad file
  warns and keeps the defaults rather than refusing to start.

## [0.1.0] - 2026-08-14 (M1, walking skeleton)

### Added
- Event loop, `Panel` trait, editor panel, file tree panel, status bar.
- `$EDITOR` invariants: `typ <file>` opens that file, blocks until closed, exits with an honest
  code, never detaches.
- Atomic save: write to a sibling temp file, fsync, rename over the target.
- The terminal's real cursor is drawn from the focused panel, so it blinks and reshapes like
  every other terminal program's.

[Unreleased]: https://github.com/Pranjal-SB/type/compare/v0.4.5...HEAD
[0.4.5]: https://github.com/Pranjal-SB/type/releases/tag/v0.4.5
[0.4.4]: https://github.com/Pranjal-SB/type/releases/tag/v0.4.4
[0.4.3]: https://github.com/Pranjal-SB/type/releases/tag/v0.4.3
[0.4.2]: https://github.com/Pranjal-SB/type/releases/tag/v0.4.2
[0.4.1]: https://github.com/Pranjal-SB/type/releases/tag/v0.4.1
[0.4.0]: https://github.com/Pranjal-SB/type/releases/tag/v0.4.0
[0.3.3]: https://github.com/Pranjal-SB/type/releases/tag/v0.3.3
[0.3.2]: https://github.com/Pranjal-SB/type/releases/tag/v0.3.2
[0.3.1]: https://github.com/Pranjal-SB/type/releases/tag/v0.3.1
[0.2.10]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.10
[0.2.9]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.9
[0.2.8]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.8
[0.2.7]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.7
[0.2.6]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.6
[0.2.5]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.5
[0.2.4]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.4
[0.2.3]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.3
[0.2.2]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.2
[0.2.1]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.1

<!-- 0.3.0, 0.2.0 and 0.1.0 have no link because they have no tag: tagging began at
     v0.2.1, when the versioning scheme was adopted, and 0.3.0 first shipped inside 0.3.1. -->

