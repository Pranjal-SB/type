---
type: index
status: living
verified: 2026-08-15
---

# TYPE documentation

Two kinds of document live here, and they are maintained differently.

**Living documents** are kept true. If one disagrees with the tree, the document is wrong and
gets fixed. Each carries a `verified` date in its frontmatter saying when that was last
checked.

**Records** are frozen once their milestone ships. A plan document is the account of what was
attempted, what actually happened, and where the two diverged — the "Actual:" lines are the
point of it. Editing one after the fact to match what the code became would destroy the only
copy of that history.

## Living

| Document | What it is |
|---|---|
| [`design/architecture.md`](design/architecture.md) | The spec. Goals, invariants, budgets, the panel contract, milestones. Deviating from it needs a stated reason. |
| [`design/gap-analysis.md`](design/gap-analysis.md) | Known defects, how TYPE measures against other editors, and the install and first-launch design. Re-run at each milestone. |
| [`../README.md`](../README.md) | The public face. Status, keys, roadmap, versioning. |

## Records

| Document | Milestone | Version |
|---|---|---|
| [`plans/m0-m1-foundation.md`](plans/m0-m1-foundation.md) | M0 feel spike, M1 walking skeleton | v0.1.0 |
| [`plans/m2-editing.md`](plans/m2-editing.md) | M2 editing — selections, multi-cursor, search | v0.2.0 |
| [`plans/m2.1-correctness.md`](plans/m2.1-correctness.md) | M2.1 keystroke budgets, undo coalescing | v0.2.1 |
| [`../spikes/m0-feel/FINDINGS.md`](../spikes/m0-feel/FINDINGS.md) | M0 measurements — the spike's code is gone, its numbers are not | — |

## Conventions

- **Frontmatter, not wikilinks.** YAML frontmatter is read by Obsidian and hidden by GitHub, so
  it costs nothing on either surface. Obsidian's `[[wikilinks]]` render as literal text on
  GitHub, and these documents are read there — so links stay relative Markdown.
- **A living document states what it was verified against.** The drift this repository has
  actually suffered — a closed question still listed as open, a README describing a missing
  feature that shipped — was invisible precisely because nothing recorded when anyone last
  looked.
- **One plan per milestone**, tasks ordered, each ending in a commit. See any of the records for
  the shape.
