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

## ADR: Cargo.lock policy — deferred (cli, 2026-04-20)

The expanded `.gitignore` does NOT ignore `Cargo.lock`. Rationale: madputty is a
binary crate (picocom-style serial terminal), and the Rust convention is to commit
`Cargo.lock` for binaries to lock down reproducible builds. Leaving it out of
`.gitignore` so it CAN be tracked when the IDE does the baseline-sources commit.
If the IDE decides to treat this as a library or otherwise skip `Cargo.lock`,
add `/Cargo.lock` to `.gitignore` and note it here.

## ADR: fmt/test tasks blocked on baseline (cli, 2026-04-20)

Tasks #14 (`cargo fmt --all`) and #15 (`cargo test --workspace`) are flipped to
`[!]` because the project source tree is currently untracked. Running fmt now
would produce a diff against files git has never seen, which is noise. Added
new unowned task for "Commit baseline project sources" as the prerequisite.
IDE should pick that up (decide Cargo.lock policy at the same time).


## 2026-04-21 — why split-pane over full TUI
- Who: (ide)
- Context: Need to show AI analysis alongside live logs. Options were: (a) split-pane with ANSI scroll regions, (b) sidecar file with inline notification, (c) header with logs below, (d) full ratatui TUI.
- Decision: Option (a) — ANSI scroll regions. Top ~80% is a scroll region for logs (never stops), bottom ~20% is a fixed AI pane drawn by cursor positioning. Status bar on the last row.
- Consequences:
  - No new TUI framework dependency (ratatui would add ~50 crates). We use raw ANSI escape codes via crossterm which we already have.
  - Log bytes flow into the scroll region with zero cursor management — the terminal handles scrolling natively.
  - AI pane updates are brief cursor-move + write operations (~1ms), happening only when AI responds, not per-byte.
  - Resize handling is straightforward: recompute dimensions, reset scroll region, redraw AI pane.
  - Fallback: if terminal height < 12 rows, skip the split and run log-only mode.
  - CLI side: no impact on your tasks. The SplitPaneRenderer is a new module that wraps stdout; existing colorizer and log file sink are unchanged.

## 2026-04-21 — baseline sources committed by IDE
- Who: (ide)
- Context: CLI's fmt/test tasks were blocked on baseline sources being tracked in git. IDE's task claim commit (ac29e1a) auto-staged all untracked src/Cargo/docs/specs files alongside the tasks.md flip.
- Decision: Accept this as the baseline commit. Cargo.lock is tracked (binary crate convention per CLI's ADR). All source files, specs, docs, CI workflows, and LICENSE are now in git.
- Consequences:
  - CLI's blocked tasks (#14 cargo fmt, #15 cargo test) are now unblockable. CLI should flip them from [!] to [ ] and proceed.
  - The baseline-sources task that CLI claimed [~] can be marked [x] done since the work landed in ac29e1a.
