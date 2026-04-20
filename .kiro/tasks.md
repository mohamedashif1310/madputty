# madputty tasks

This is the single source of truth for work on madputty. Both Kiro IDE and Kiro CLI read
and write to this file. Coordinate through git commits — commit status flips immediately
so the other side sees them on the next pull.

## Legend

- **Status**: `[ ]` todo | `[~]` in-progress | `[x]` done | `[!]` blocked
- **Owner**: `(ide)` or `(cli)`
- Optional `files:` hint when touching shared files to prevent step-on-toes

## Protocol reminders

- Before starting: `git pull` (or refresh), re-read this file + `decisions.md`.
- When picking up a `[ ]` task: flip to `[~]`, set owner, commit that flip FIRST before doing work.
- When finishing: flip to `[x]`, add a one-line note with files touched, commit.
- If blocked: flip to `[!]` and say what's needed — especially if the other side needs to unblock it.
- Design decisions that affect the other side: append to `.kiro/decisions.md` before committing the work.
- Default split: IDE = multi-file refactors, design, UI edits, rich-diff docs. CLI = build/test loops,
  grep-glob sweeps, benchmarks, git chores, parallel subagent work.

## Active specs

- `.kiro/specs/serial-terminal/` — core picocom-style serial terminal (done)
- `.kiro/specs/kiro-cli-log-analysis/` — split-pane AI analysis via kiro-cli (in design)

## Tasks

### Spec: kiro-cli-log-analysis

- [~] (ide) Design doc for kiro-cli-log-analysis — write `.kiro/specs/kiro-cli-log-analysis/design.md` covering architecture, module layout, task graph, split-pane UI approach, non-blocking guarantees, redaction pipeline. files: `.kiro/specs/kiro-cli-log-analysis/design.md`
- [ ] (ide) Tasks doc for kiro-cli-log-analysis — write `tasks.md` after design is approved by user. files: `.kiro/specs/kiro-cli-log-analysis/tasks.md`
- [ ] (ide) Implement `src/ai/` module (redaction, kiro-cli invoker, rolling buffer, response formatter) — after tasks.md approved. files: `src/ai/*.rs`, `Cargo.toml`
- [ ] (ide) Implement split-pane terminal renderer — scroll region + AI pane drawing via crossterm. files: `src/ui/split_pane.rs`, `src/session.rs`
- [ ] (ide) Wire hotkeys Ctrl+A A, Ctrl+A Q, Ctrl+A L into keymap + session. files: `src/io/keymap.rs`, `src/session.rs`
- [ ] (ide) Add `--ai-watch`, `--ai-timeout-seconds`, `--no-redact`, `--no-ai` flags + `kiro-login`/`kiro-status` subcommands. files: `src/cli.rs`, `src/main.rs`
- [ ] (ide) AI response persistence — write `~/.madputty/ai-responses/<session_id>.md`. files: `src/ai/response_log.rs`
- [ ] (cli) Add `regex = "1"` to Cargo.toml and run `cargo check --all-features` to confirm clean build after ai module lands. files: `Cargo.toml`, `Cargo.lock`
- [ ] (cli) Run `cargo clippy -- -D warnings` across the repo after each ai module commit; file findings back. files: n/a (read-only analysis)
- [ ] (cli) Property-test the redaction engine with proptest — idempotence, leak prevention. files: `tests/redaction_properties.rs`
- [ ] (cli) Integration test for non-blocking log pump — spawn madputty in plain mode, assert bytes keep flowing while a mock slow AI task runs. files: `tests/integration/non_blocking_pump.rs`
- [ ] (cli) Benchmark split-pane redraw cost at 921600 baud to confirm no visible lag. files: `benches/redraw.rs`

### Cross-cutting / hygiene

- [x] (cli) Configure `.gitignore` for Rust target dir, editor junk, local creds. files: `.gitignore` — expanded to cover Rust/Cargo, editors, OS junk, secrets (pem/key/env), logs, criterion, madputty runtime dirs (`/ai-responses/`, `/session-logs/`, `/.madputty/`), and `.kiro/cache` + `.kiro/sessions`. Cargo.lock policy intentionally deferred to IDE (see decisions.md).
- [!] (cli) Run `cargo fmt --all` once and commit a baseline. files: whole repo (format-only) — blocked on baseline commit of project sources
- [!] (cli) Verify `cargo test --workspace` passes from a clean checkout. files: none (verification) — blocked on baseline commit of project sources
- [~] (cli) Commit baseline project sources (src/, Cargo.toml, Cargo.lock, README.md, PROJECT_OVERVIEW.md, LICENSE, CONTRIBUTING.md, .github/, .kiro/specs/) — prerequisite for fmt/test tasks. IDE should decide Cargo.lock policy (track for binary, ignore for lib). files: repo-wide
- [ ] (ide) Update `README.md` with new `--ai-*` flags, hotkey table, and kiro-cli setup section. files: `README.md`
- [ ] (ide) Update `PROJECT_OVERVIEW.md` with kiro-cli integration section (extension points, ADR pointers). files: `PROJECT_OVERVIEW.md`
- [ ] (ide) Add an ADR-style entry in `decisions.md` for "why split-pane over full TUI" once design lands. files: `.kiro/decisions.md`

### Nice-to-have / deferred (not claimed)

- [ ] (any) Windows MSI installer via `cargo-wix` — low priority, revisit after AI feature ships. files: `wix/*`
- [ ] (any) CI workflow (`.github/workflows/ci.yml`) for check/test/clippy/fmt — only if repo gets pushed anywhere; hold until decision is made. files: `.github/workflows/*`
- [ ] (any) Serial session replay — feed a `.log` back through madputty as if it were a device (useful for testing the colorizer + AI). files: `src/replay.rs` (new)
- [ ] (any) Hex dump mode for binary protocols. files: `src/io/colorizer.rs`

## Notes

- This repo just got `git init`'d. No remotes yet. If/when a remote is added, both sides should pull before picking tasks.
- Both sides MUST commit status flips immediately to minimize races on claim.
- If you see two `[~]` entries for the same task after a pull, the second claimer's earlier commit must be dropped (force-pull the first claim and back off).
