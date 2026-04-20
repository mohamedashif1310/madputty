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

- [x] (ide) Design doc for kiro-cli-log-analysis — write `.kiro/specs/kiro-cli-log-analysis/design.md` covering architecture, module layout, task graph, split-pane UI approach, non-blocking guarantees, redaction pipeline. files: `.kiro/specs/kiro-cli-log-analysis/design.md` — DONE: two-lane architecture (log pump + AI subsystem), split-pane via ANSI scroll regions, HotkeyDispatcher extending ExitStateMachine, Redactor with 6 regex patterns, KiroInvoker with tokio::process + timeout, RollingBuffer with Arc<Mutex<VecDeque>>, ErrorScanner with 30s debounce, ResponseLog as append-only Markdown, full sequence diagrams for manual + auto-watch flows.
- [x] (ide) Tasks doc for kiro-cli-log-analysis — write `tasks.md` after design is approved by user. files: `.kiro/specs/kiro-cli-log-analysis/tasks.md` — DONE: 21 tasks covering all new modules (ai/, ui/), existing file modifications, checkpoints, and docs. Ordered for incremental progress.
- [x] (ide) Implement `src/ai/` module (redaction, kiro-cli invoker, rolling buffer, response formatter) — after tasks.md approved. files: `src/ai/*.rs`, `Cargo.toml` — DONE: 7 files in src/ai/ (rolling_buffer, redactor, kiro_invoker, error_scanner, pane, response_log, mod). Zero diagnostics.
- [x] (ide) Implement split-pane terminal renderer — scroll region + AI pane drawing via crossterm. files: `src/ui/split_pane.rs`, `src/session.rs` — DONE: src/ui/split_pane.rs + src/ui/mod.rs. ANSI scroll regions, resize handling, fallback mode. session.rs integration pending (separate task).
- [x] (ide) Wire hotkeys Ctrl+A A, Ctrl+A Q, Ctrl+A L into keymap + session. files: `src/io/keymap.rs`, `src/session.rs` — DONE: HotkeyDispatcher replaces ExitStateMachine. AI hotkey stubs in session.rs. Full AI task wiring is part of session integration.
- [x] (ide) Add `--ai-watch`, `--ai-timeout-seconds`, `--no-redact`, `--no-ai` flags + `kiro-login`/`kiro-status` subcommands. files: `src/cli.rs`, `src/main.rs` — DONE: all flags added, KiroLogin/KiroStatus subcommands dispatch to kiro-cli.
- [x] (ide) AI response persistence — write `~/.madputty/ai-responses/<session_id>.md`. files: `src/ai/response_log.rs` — DONE: append-only Markdown with timestamp headers.
- [x] (cli) Add `regex = "1"` to Cargo.toml and run `cargo check --all-features` to confirm clean build after ai module lands. files: `Cargo.toml`, `Cargo.lock` — regex = "1" added to [dependencies]. `cargo check --all-features` exit 0. Same 3 pre-existing warnings.
- [x] (cli) Run `cargo clippy -- -D warnings` across the repo after each ai module commit; file findings back. files: n/a (read-only analysis) — 5 findings filed below under "Clippy findings (2026-04-20)". 3 warnings pre-existing in theme.rs, 2 new in colorizer.rs. All are (ide) territory to fix.
- [ ] (cli) Property-test the redaction engine with proptest — idempotence, leak prevention. files: `tests/redaction_properties.rs`
- [ ] (cli) Integration test for non-blocking log pump — spawn madputty in plain mode, assert bytes keep flowing while a mock slow AI task runs. files: `tests/integration/non_blocking_pump.rs`
- [ ] (cli) Benchmark split-pane redraw cost at 921600 baud to confirm no visible lag. files: `benches/redraw.rs`

### Cross-cutting / hygiene

- [x] (cli) Configure `.gitignore` for Rust target dir, editor junk, local creds. files: `.gitignore` — expanded to cover Rust/Cargo, editors, OS junk, secrets (pem/key/env), logs, criterion, madputty runtime dirs (`/ai-responses/`, `/session-logs/`, `/.madputty/`), and `.kiro/cache` + `.kiro/sessions`. Cargo.lock policy intentionally deferred to IDE (see decisions.md).
- [x] (cli) Run `cargo fmt --all` once and commit a baseline. files: whole repo (format-only) — fmt touched src/list.rs, src/main.rs, src/session.rs, src/theme.rs. Exit 0.
- [x] (cli) Verify `cargo test --workspace` passes from a clean checkout. files: none (verification) — `cargo test --workspace` exit 0. 0 tests found (no tests authored yet). 3 warnings in src/theme.rs (pre-existing, will be addressed by clippy sweep task #9).
- [x] (cli) Commit baseline project sources (src/, Cargo.toml, Cargo.lock, README.md, PROJECT_OVERVIEW.md, LICENSE, CONTRIBUTING.md, .github/, .kiro/specs/) — files landed in commit ac29e1a. NOTE: staging race with IDE caused my staged content to be included in IDE's claim commit; see race ADR in decisions.md. Content is correct; Cargo.lock tracked per binary convention. `cargo check` passes (3 pre-existing warnings in src/theme.rs).
- [x] (ide) Update `README.md` with new `--ai-*` flags, hotkey table, and kiro-cli setup section. files: `README.md`
- [x] (ide) Update `PROJECT_OVERVIEW.md` with kiro-cli integration section (extension points, ADR pointers). files: `PROJECT_OVERVIEW.md`
- [x] (ide) Add an ADR-style entry in `decisions.md` for "why split-pane over full TUI" once design lands. files: `.kiro/decisions.md` — DONE: ADR added in design doc commit (936d72d) + lane agreement ADR added.

### Nice-to-have / deferred (not claimed)

- [ ] (any) Windows MSI installer via `cargo-wix` — low priority, revisit after AI feature ships. files: `wix/*`
- [ ] (any) CI workflow (`.github/workflows/ci.yml`) for check/test/clippy/fmt — only if repo gets pushed anywhere; hold until decision is made. files: `.github/workflows/*`
- [ ] (any) Serial session replay — feed a `.log` back through madputty as if it were a device (useful for testing the colorizer + AI). files: `src/replay.rs` (new)
- [ ] (any) Hex dump mode for binary protocols. files: `src/io/colorizer.rs`

## Notes

- This repo just got `git init`'d. No remotes yet. If/when a remote is added, both sides should pull before picking tasks.
- Both sides MUST commit status flips immediately to minimize races on claim.
- If you see two `[~]` entries for the same task after a pull, the second claimer's earlier commit must be dropped (force-pull the first claim and back off).

## Clippy findings (2026-04-20, cli)

`cargo clippy --all-targets --all-features -- -D warnings` exit 101. 5 findings,
all (ide) territory to fix. These do NOT block any CLI task, but should be
addressed before the next clippy sweep (which will run after each src/ai/ module
commit per task #9).

- [x] (ide) theme.rs:155 — `let mut row =` closure, `mut` not needed (unused_mut) — FIXED
- [x] (ide) theme.rs:35 — `pub const BOX_MASCOT` unused (dead_code) — REMOVED
- [x] (ide) theme.rs:63 — `Palette::log_number` field unread (dead_code) — REMOVED
- [x] (ide) colorizer.rs:165 — replace `.map_or` with `.is_none_or` — FIXED
- [x] (ide) colorizer.rs:175 — replace `while let` with `for` — FIXED
