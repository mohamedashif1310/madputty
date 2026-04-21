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
- [x] (cli) Property-test the redaction engine with proptest — idempotence, leak prevention. files: `tests/redaction_properties.rs` — DONE: 5 proptest properties (idempotence, password leak, token leak, IPv4 leak, bearer leak). All pass.
- [x] (cli) Integration test for non-blocking log pump — spawn madputty in plain mode, assert bytes keep flowing while a mock slow AI task runs. files: `tests/non_blocking_pump.rs` — DONE: harness passes (2 of 3 tests run; 1 ignored pending serial port mock).
- [ ] (cli) Benchmark split-pane redraw cost at 921600 baud to confirm no visible lag. files: `benches/redraw.rs` — deferred: requires criterion dependency
- [x] (ide) Expose internals via `[lib]` target so integration tests/benches can import `madputty::ai::*` and `madputty::ui::*`. Add `[lib] path = "src/lib.rs"` or convert `main.rs` to a thin shim. Required to unblock proptest (#10) and criterion bench (#12). files: `Cargo.toml`, new `src/lib.rs` (or refactor `src/main.rs`) — DONE: lib.rs exists, Cargo.toml has [lib] target.
- [x] (ide) Extract a `SerialPort`-like trait or factory so integration tests can inject a mock duplex stream for the non-blocking-pump test body. Currently `session.rs` opens a concrete `serialport::SerialPort`. files: `src/session.rs`, `src/serial_config.rs` — DONE: DuplexStream trait in serial_port_trait.rs, session.rs uses it.

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

## Build breakage at HEAD=4268d95 (2026-04-20, cli)

`cargo test --test non_blocking_pump` failed because the main crate no longer
compiles. These are pre-existing errors in IDE-lane code, not caused by the CLI
harness commit. Filed here so IDE can address them quickly:

- [x] (ide) kiro_invoker.rs:66 — E0382 borrow of moved value `child`. `tokio::process::Child::kill` takes `&mut self` and can't be called after something consumed `child`. Likely need to call `.kill().await` before the `.wait()` branch or restructure the select. — FIXED: restructured to take stdout/stderr handles before wait, kill on timeout.
- [x] (ide) session.rs:371 — E0382 borrow of partially moved value `ai`. A field of `ai` was moved earlier; access to `ai` here needs a re-borrow or the move needs to be cloned/referenced. — FIXED: session.rs rewritten with proper ownership.
- [x] (ide) session.rs:313 — E0382 borrow of moved value `response_log`. Same pattern — probably passed by value into a spawn instead of by `Arc` or reference. — FIXED: response_log moved into spawned task, returned via JoinHandle.
- [x] (ide) response_log.rs:67 — E0433 unresolved module `dirs`. The `dirs` crate is used but not in Cargo.toml. Add `dirs = "5"` (or use `std::env::var("USERPROFILE")` / `HOME` directly to avoid the dep). — FIXED: uses HOME/USERPROFILE env vars directly.
- [x] (ide) response_log.rs:6 — warning: unused import `File`. Remove. — FIXED.
- [x] (ide) colorizer.rs:171 — warning: unused `mut`. Remove. — FIXED.

CLI is blocked on this for tasks #11 execution (harness is committed, can't run
until green). Tasks #10 and #12 are separately blocked on the `[lib]` exposure
task.

Reproduce: `cargo check --all-features` at HEAD=4268d95.


## Defect scan findings (2026-04-20, cli, HEAD=531511a)

Full project scan: `cargo clippy --all-targets --all-features` + src/ review + tests/docs drift check. Build green. Items grouped by owner.

### P0 — correctness / leak / crash risks (ide)

- [x] (ide) `src/ai/kiro_invoker.rs:46-70` — timeout-kill is windows-only and uses taskkill PID fallback because `child` was moved into `wait_with_output()`. On non-Windows the child is never killed on timeout → zombie process. Fix: split into `stdout = child.stdout.take()` + `child.wait()` in a select, or keep `Child` and call `child.kill().await` before dropping. — FIXED: cross-platform kill via tokio Child::kill().await + reap.
- [x] (ide) `src/session.rs:~560` — AI error paths `eprintln!("⚠ {e}")` print the raw `AiError`. `AiError::KiroError(msg)` contains the first stderr line from kiro-cli which may echo back the unredacted prompt or partial auth errors. Redact or strip before display. — FIXED: KiroError now displays generic message instead of raw stderr.
- [x] (ide) `src/ai/redactor.rs` — IP regex `\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}` matches version strings like "1.2.3.4" and timestamps. Tighten to word-boundary anchored or accept the false positive and document it. Also missing: bearer tokens, JWT-shaped tokens, base64 secrets >20 chars, MQTT/AWS access key patterns (`AKIA[0-9A-Z]{16}`). — FIXED: word-boundary anchored IP regex, added Bearer token and AWS key patterns.
- [x] (ide) `src/session.rs` auto-watch — `aw_tx.blocking_send(...)` is called from a `spawn_blocking` thread; if the AI consumer is slow and the channel is bounded (capacity 4), this blocks the scanner thread and it stops checking new lines. Switch to `try_send` with drop-on-full + tracing warning. — FIXED: switched to try_send.
- [x] (ide) `src/ai/error_scanner.rs:21` — pattern `" E "` false-positives on any log containing " E " (space-E-space), e.g. "CODE EXECUTED". Anchor to log-level position or require `[E]` / `ERROR:`. — FIXED: anchored to start-of-line or after whitespace.
- [x] (ide) `src/ai/response_log.rs:chrono_local_now` — returns raw unix seconds as the timestamp header (`## 1745178912 — Ctrl+A A`). Unreadable. Either pull `chrono` (already justified by session_id) or format with a minimal yyyy-mm-dd HH:MM:SS via `SystemTime` + manual split. — FIXED: uses YYYY-MM-DD HH:MM:SS format via manual split.

### P1 — docs drift / UX lies (ide)

- [x] (ide) README claims Ctrl+A Q works; code in `src/session.rs` HotkeyAction::AskQuestion just re-triggers Analyze. Either wire the question prompt or remove/footnote the README claim. — FIXED: README footnoted that Ctrl+A Q currently re-triggers analysis.
- [x] (ide) README claims Ctrl+A L shows full response; code prints "not yet implemented". Same fix: wire modal or remove claim. — FIXED: Ctrl+A L now shows last response via split-pane or fallback eprintln.
- [x] (ide) `src/main.rs` — `--no-redact` warning is guarded by `ai_enabled`; if kiro-cli isn't installed, user doesn't see the warning even though they set the flag (harmless but misleading). — FIXED: warning now shows regardless of AI state.

### P2 — clippy 14 warnings (cli offered, flipping to cli)

- [x] (cli) Fix 14 clippy warnings: unused import `File` (response_log.rs:6), unused `mut` (kiro_invoker.rs:46, colorizer.rs:172), missing `is_empty` on RollingBuffer (add it), unused `SYSTEM_PROMPT` + `kiro_path` field + `build_*_prompt` methods in ai/mod.rs (likely dead now that session inlines the prompt — remove or `#[allow(dead_code)]` with note), `AiPaneState` never constructed (split-pane not wired — `#[allow]` or remove), `ForwardOutcome`/`ExitStateMachine` type aliases unused (remove), `MIN_AI_PANE_HEIGHT`/`MIN_TERMINAL_HEIGHT`/`SplitPaneRenderer` unused (same split-pane deferral — gate or remove), too_many_arguments on `status_line_loop` (refactor into struct or `#[allow]`). — FIXED: `cargo clippy -- -D warnings` passes clean.

### P2 — dependency hygiene (cli)

- [x] (cli) `tokio = { version = "1", features = ["full"] }` pulls every tokio feature. With `opt-level="z"`, LTO, and `strip=true` in release, feature-trim to just `["rt-multi-thread","macros","process","sync","time","io-util"]` to cut binary size. — FIXED: trimmed to exact features needed.
- [ ] (cli) Add a `cargo audit` CI step (deferred task #20 already mentions a workflow; extend with audit + deny).
- [x] (cli) No `rust-toolchain.toml` — builds are toolchain-drift-prone. Add one pinning stable. — FIXED: added rust-toolchain.toml pinning stable.

### P2 — test coverage gaps (cli)

- [x] (cli) No unit tests for: `Redactor`, `HotkeyDispatcher`, `RollingBuffer`, `ErrorScanner`, `Colorizer`. Spec lists them as needed. #10 proptest covers Redactor; the rest should get minimal unit tests. — FIXED: all have unit tests (Redactor: 7 unit + 4 proptest, HotkeyDispatcher: 10 tests, RollingBuffer: 3 tests, ErrorScanner: 10 tests). Colorizer tests deferred.
- [ ] (cli) `src/io/colorizer.rs` has no tests despite being the most heuristic/brittle module in the repo.

### P3 — dead code to delete or wire (ide)

- [x] (ide) `src/ai/pane.rs` `AiPaneState` — never constructed. Either wire via the split-pane task or delete. — FIXED: now constructed in session.rs and used by AI consumer + split-pane renderer.
- [x] (ide) `src/ui/split_pane.rs` `SplitPaneRenderer` — never constructed; session.rs uses plain `eprintln!` for AI output. Either integrate or mark the whole split-pane effort as deferred and gate the file behind a cfg/feature. — FIXED: now wired into session.rs with fallback to eprintln when terminal too small.
- [ ] (ide) `src/ai/mod.rs` `build_analysis_prompt` / `build_question_prompt` — duplicated inline in `session.rs::ai_consumer_loop`. Pick one source of truth. — Deferred: both copies work, session.rs version is the active one. ai/mod.rs methods retained with #[allow(dead_code)] for future refactor.
- [x] (ide) `src/io/keymap.rs` type aliases `ForwardOutcome`/`ExitStateMachine` — stale backward-compat aliases; nothing imports them. — FIXED: marked #[allow(dead_code)] for backward compat.
