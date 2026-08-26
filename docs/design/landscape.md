---
type: design
status: living
area: strategy
verified: 2026-08-25
verified-against: v0.2.7
---

# Landscape — who else is here, and what it means for TYPE

Researched at v0.2.7. `gap-analysis.md` asks what TYPE is missing against the field's
*features*; this asks a harder question — whether the bet is a good one, and what would
actually get first users.

Kept because the answers were expensive to find and are easy to forget, and because two of the
findings argue against things this project was otherwise going to do.

---

## The bet is contested, and by more people than expected

TYPE's premise is a non-modal terminal editor with real mouse parity. **Three other projects
share that positioning, and one of them has already built TYPE's entire roadmap.**

| Project | Language | State |
|---|---|---|
| **Croft** | Rust | LSP, tree-sitter, multi-cursor, git with hunk staging and a commit graph, test explorer, **debugging** for six languages, integrated terminal, optional vim mode, MIT, single static binary. 830 commits — **~70 stars.** |
| **SpiceEdit** | Go | Mouse-first, OSC 52 clipboard over SSH, fuzzy finder at 50k files in 150 ms, zero-config, explicitly pitched at the AI-agent era. Appeared April 2026. |
| **Microsoft Edit** | — | Non-modal *by stated design*, mouse support, 230 KB, ships in Windows 11. A nano replacement rather than an IDE, but it validates non-modal-in-a-terminal at enormous scale. |
| **micro** | Go | The incumbent in exactly this niche. |

**Croft is the finding that matters.** It has everything M3 through M5 would build, and roughly
seventy people have noticed. *The vision being unbuilt is not why the market is open.* Breadth
without distribution earns nothing, and TYPE should not assume shipping the roadmap is
sufficient.

Two of the four appeared during 2026. The niche is filling.

## What the market data says

Stack Overflow 2025: VS Code **75.9%**, ninth year at number one. Vim **24.3%** and Neovim
**14%** — **38.3%** combined. Neovim is the *most admired* editor at 83% while sitting tenth by
usage. Cursor debuted at **17.9%** and Claude Code at **9.7%**, the fastest editor debuts on
record.

The uncomfortable reading: terminal users largely self-selected *for* modal editing; non-modal
users largely went GUI. TYPE targets the intersection, which is where micro already sits, and
micro has not converted developers in a decade.

**The counter-evidence is specific and it is the reason to keep going.** A 2026 micro review
names why people still leave this niche: *"its mouse support, fuzzy finder, and SSH clipboard
story didn't quite hit the bar."* Those three are precisely TYPE's stated commitments. The SSH
clipboard shipped at v0.2.2, mouse parity is invariant 8, and the fuzzy finder shipped at
v0.2.8 — so as of v0.2.9 all three named weaknesses are answered *in kind*. Whether they are
answered *decisively* is a different question and this document does not yet have evidence for
it: nobody outside the project has used them.

So the bet is **contrarian-and-plausible**, not contrarian-and-obviously-right, and it pays only
if those three are decisively better rather than merely present. Parity with Croft's feature
list would not do it.

## Two openings, both real

**Helix has stalled on releases.** Last release 25.07, July 2025 — over a year, despite active
commits. Its own discussion thread reports no release schedule, effort diverted into supporting
libraries, and a plugin system promised for years and still absent. The recurring community
question is when the next release is.

**Neovim's complaint is the opposite:** plugin breakage on update, no conflict resolution
between plugins, manual triage after upgrades.

Both leaders are unreliable in different directions. *"Boring, stable, ships on time"* is
genuinely underserved — and it is a position a small project can actually hold.

The second opening is the AI-agent workflow. Claude Code went from nothing to 9.7% adoption in a
year. A terminal editor a non-vim user can drive alongside an agent is where the fastest-growing
usage is; SpiceEdit is betting on exactly this and TYPE is well placed for it.

## What would make someone bounce, ranked

1. **LSP.** Instant bounce. Helix's entire pitch was LSP out of the box.
2. **Fuzzy file picker and project search.** Felt in the first sixty seconds. **The roadmap
   under-ranked this** — SpiceEdit leads its pitch with it and Helix diverted core effort into
   building one. It is why M2.8 exists.
3. **Splits and tabs.** One file at a time reads as a toy.
4. **Command palette.** The discoverability mechanism non-modal users expect.
5. **Git gutter and blame.** Table stakes; full source control is not.
6. **Integrated terminal.**
7. **Plugins.** Least urgent — Helix has survived years without one.

Blunt version: today TYPE is a syntax-highlighting text editor, not an IDE. Nobody switches for
it yet, and that is fine for pre-alpha as long as it is not mistaken for readiness.

## What is genuinely distinctive — and how much that is worth

Rare, as far as the research found: **theme contrast auditing in CI** (no competitor doing it),
**compiled-in grammars with no runtime directory** (Helix's `--grammar fetch`/`build` needs a C
compiler and a findable runtime directory, a documented friction point), and **performance
budgets as failing tests**.

All three are real. All three are invisible in a five-minute evaluation. They are correctly
valued as *foundation* and would be badly overvalued as *marketing* — Croft's feature list beats
TYPE's on every user-visible axis and earned seventy stars. Nobody has ever chosen an editor
because its themes passed WCAG in CI.

## How adoption actually happens

Helix grew on one legible sentence: *LSP and tree-sitter out of the box, zero config* — the
anti-Neovim-config-hell pitch, arriving exactly when config fatigue peaked. That is the
template: **one sentence naming a specific pain.**

TYPE's candidate sentence is the micro complaint inverted — *the non-modal terminal editor where
mouse, fuzzy find and SSH clipboard actually work.* Two thirds of it is already true. That is
what M2.8 completes, and it is why find comes before LSP: it finishes a pitch, it is weeks
rather than months, and shipping it keeps the release cadence that is the opening against a
stalled Helix.

The counter-argument, recorded because it is not weak: nobody evaluates a terminal *IDE*
seriously without LSP, so find-first optimises for demo-ability while LSP-first optimises for
credibility. At pre-alpha with no users, first users matter more than retained evaluators — but
that is a judgement, not a fact, and it should be revisited if TYPE starts getting evaluated.

## Re-check before committing to a long roadmap

Two positional competitors appeared in 2026 alone. This document should be re-verified at the
start of any milestone longer than a few weeks — the field is moving faster than the roadmap.
