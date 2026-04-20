# madputty decisions

Short ADR-style notes for design decisions that affect both Kiro IDE and Kiro CLI.
Append-only: oldest first, newest last. Each entry has a timestamp, who made it,
context, decision, and consequences.

Format:

```
## YYYY-MM-DD HH:MM — title
- Who: (ide) or (cli)
- Context: one or two sentences.
- Decision: what we chose.
- Consequences: what it means for the other side / for future work.
```

---

## 2026-04-20 — coordination protocol established
- Who: (ide)
- Context: Two Kiro agents (IDE + CLI) will work the same repo in parallel without
  shared memory. Coordination must flow through files + git commits.
- Decision: `.kiro/tasks.md` is the single source of truth for work. `.kiro/decisions.md`
  is append-only for cross-cutting choices. Status flips are committed immediately
  before any real work begins, so the commit graph resolves "who claimed first" races.
- Consequences:
  - Both sides pull before picking tasks and re-read both files.
  - Merge conflicts on tasks.md are expected but small — one-line edits, commit fast.
  - If a decision changes something the other side is mid-work on, add a note here
    BEFORE committing the change so the other side sees it on next pull.

## 2026-04-20 — repo was git-init'd today
- Who: (ide)
- Context: There was no `.git/` when coordination started.
- Decision: `git init -b main`. No remote configured. Commits stay local unless/until a remote is added.
- Consequences:
  - The "pull before picking" step is a no-op until a remote exists — `git status` and
    re-reading the files is sufficient for now.
  - When a remote is added, both sides must start `git fetch && git pull --ff-only` before each task cycle.

## 2026-04-20 — division of labor baseline
- Who: (ide)
- Context: Need a default split so tasks get picked up efficiently without constant negotiation.
- Decision: IDE takes multi-file refactors, architecture/design, UI edits, rich-diff docs, code review.
  CLI takes build/test/clippy/fmt loops, grep/glob sweeps, benchmarks, git housekeeping, parallel subagents.
  Per-task owner in `tasks.md` can override the default.
- Consequences: When unsure, assign to whoever is idle. Either side can reassign by flipping
  the owner field and committing — but do so BEFORE starting work to avoid overlap.
