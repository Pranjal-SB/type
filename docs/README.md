---
type: index
status: living
verified: 2026-08-26
---

# TYPE documentation

Kept true against the tree. If one disagrees with the code, the document is wrong and gets
fixed. Each carries a `verified` date saying when that was last checked.

| Document | What it is |
|---|---|
| [`design/architecture.md`](design/architecture.md) | The spec. Goals, invariants, budgets, the panel contract, milestones. Deviating from it needs a stated reason. |
| [`design/gap-analysis.md`](design/gap-analysis.md) | Known defects, and how TYPE measures against other editors. Re-run at each milestone. |
| [`design/themes.md`](design/themes.md) | The theme format, the 25 slots, and the contrast rubric every palette is measured against. |
| [`design/visual.md`](design/visual.md) | **Draft, not built.** What TYPE looks like: one rule instead of boxes, how focus is shown without one, and why the renderer is not what caps it. |
| [`design/controls.md`](design/controls.md) | **Half built.** The keyboard model: two chord tiers, prefix resolution and its hint, layered keymaps. The tier analysis decided how the palette and tab switching are bound; the prefix mechanism is not built. |
| [`design/landscape.md`](design/landscape.md) | Who else is in this niche and whether the bet is a good one. Asks a harder question than the gap analysis: not what is missing, but whether it matters. |
| [`design/lsp.md`](design/lsp.md) | **Research, ahead of the code.** What the field uses to talk to language servers and why, the position-encoding problem, and the nine decisions M3 was planned against. |
| [`releasing.md`](releasing.md) | Cutting a release: the close-out, the tag, and the order the ten crates publish in. |

The roadmap lives in the [README](../README.md). Per-milestone task lists are working documents
and are not published.

[`design/m0-findings.md`](design/m0-findings.md) is kept for its numbers — the M0 feel spike's
code was deleted once it had answered its question, its measurements were not.
