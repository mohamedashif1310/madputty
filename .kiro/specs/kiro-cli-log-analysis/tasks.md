# Implementation Plan: Kiro CLI Log Analysis

## Overview

Additive AI analysis lane for the madputty serial terminal, powered by `kiro-cli`. The implementation follows the design's two-lane architecture: the existing log pump lane remains untouched except for fan-out hooks, while a new AI subsystem lane runs independently on tokio tasks. One new dependency (`regex = "1"`) is added. All new code lives in `src/ai/` and `src/ui/` modules, with minimal modifications to existing files.

## Tasks

- [ ] 1. Add `regex` dependency to Cargo.toml
  - Add `regex = "1"` to `[dependencies]` in `Cargo.toml`
  - Run `cargo check` to verify resolution
  - _Requirements: 18.1, 18.2_

- [ ] 2. Create `src/ai/rolling_buffer.rs` — RollingBuffer
  - [ ] 2.1 Implement `RollingBuffer` struct with `Arc<Mutex<VecDeque<String>>>` and capacity of 50
    - `push(&self, line: String)` — lock, push_back, pop_front if over capacity
    - `snapshot(&self) -> Vec<String>` — lock, clone into Vec
    - UTF-8 lossy decoding handled by caller before push
    - _Requirements: 17.1, 17.2, 17.3, 17.4, 17.5_

  - [ ]* 2.2 Write unit tests for RollingBuffer
    - Test push evicts oldest when over capacity
    - Test snapshot returns independent copy
    - Test empty buffer snapshot returns empty vec
    - _Requirements: 17.1, 17.2, 17.3_

- [ ] 3. Create `src/ai/redactor.rs` — Redaction engine
  - [ ] 3.1 Implement `Redactor` struct with 6 compiled regex patterns
    - `new()` — compile all patterns once at construction
    - `redact(&self, input: &str) -> String` — apply all patterns sequentially
    - Patterns: password, token, IPv4, MAC, SSID, api/secret/access key
    - Replacements per design: `[REDACTED]`, `[IP]`, `[MAC]`, `[SSID]`
    - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5, 6.6, 6.7, 6.8_

  - [ ]* 3.2 Write property tests for Redactor idempotence
    - proptest: `redact(redact(x)) == redact(x)` for arbitrary strings
    - proptest: output never contains original password/token/IP values when input contains them
    - _Requirements: 6.8_

  - [ ]* 3.3 Write unit tests for Redactor patterns
    - Test each of the 6 patterns individually
    - Test combined input with multiple sensitive values
    - Test that non-sensitive text passes through unchanged
    - _Requirements: 6.2, 6.3, 6.4, 6.5, 6.6, 6.7_

- [ ] 4. Create `src/ai/kiro_invoker.rs` — Shell out to kiro-cli
  - [ ] 4.1 Implement `KiroInvoker` struct
    - `kiro_path: PathBuf` and `timeout: Duration` fields
    - `invoke(&self, prompt: &str) -> Result<String, AiError>` — spawn `kiro-cli chat --no-interactive <prompt>`, enforce timeout via `tokio::time::timeout`, kill on timeout, return stdout on success
    - Handle non-zero exit → `AiError::KiroError(stderr_first_line)`
    - Handle timeout → `AiError::Timeout`
    - Reap child process on timeout to avoid zombies
    - _Requirements: 7.4, 11.1, 11.4, 11.5, 11.6, 18.3, 18.4_

  - [ ]* 4.2 Write unit tests for KiroInvoker
    - Test timeout handling with a mock slow process
    - Test non-zero exit code error extraction
    - _Requirements: 11.4, 11.6_

- [ ] 5. Create `src/ai/error_scanner.rs` — Error pattern matching with debounce
  - [ ] 5.1 Implement `ErrorScanner` struct
    - Compiled regex patterns for: `" E "`, `"ERROR"`, `"FAIL"`, `"FAILED"`, `"PANIC"`, `"EXCEPTION"`, `"TIMEOUT"`
    - `last_trigger: Option<Instant>` for 30s debounce
    - `check(&mut self, line: &str) -> bool` — returns true if pattern matched AND debounce allows
    - _Requirements: 5.2, 5.3, 5.4_

  - [ ]* 5.2 Write unit tests for ErrorScanner
    - Test each error pattern triggers
    - Test debounce suppresses rapid-fire triggers within 30s
    - Test debounce allows trigger after 30s elapsed
    - _Requirements: 5.3, 5.4_

- [ ] 6. Create `src/ai/pane.rs` — AiPaneState struct
  - Implement `AiPaneState` with fields: `header_time`, `body`, `body_truncated`, `spinner_active`, `error`, `modal_open`, `modal_scroll_offset`
  - Add methods for updating state: `set_response`, `set_error`, `set_spinner`, `open_modal`, `close_modal`, `scroll_modal`
  - _Requirements: 8.1, 8.3, 8.4, 8.5, 8.6, 8.7, 8.8_

- [ ] 7. Create `src/ai/response_log.rs` — Append-only Markdown response log
  - [ ] 7.1 Implement `ResponseLog` struct
    - `path: PathBuf` — `~/.madputty/ai-responses/<session_id>.md`
    - `has_entries: bool` — track whether anything was written
    - `append(&mut self, trigger: &str, question: Option<&str>, response: &str) -> io::Result<()>` — create dir + file on first write, append Markdown section with timestamp header
    - `has_entries(&self) -> bool` and `path(&self) -> &Path` accessors
    - Session ID format: `YYYYMMDD-HHMMSS-<port>`
    - _Requirements: 12.1, 12.2, 12.3, 12.4, 12.5, 12.6, 12.7_

  - [ ]* 7.2 Write unit tests for ResponseLog
    - Test Markdown format output
    - Test directory creation on first write
    - Test has_entries tracking
    - _Requirements: 12.3, 12.4, 12.6_

- [ ] 8. Create `src/ai/mod.rs` — AiSubsystem orchestrator
  - [ ] 8.1 Implement `AiSubsystem` struct with kiro-cli detection
    - `detect(no_ai: bool) -> Self` — PATH lookup for `kiro-cli`/`kiro-cli.exe`, probe login via `kiro-cli whoami --no-interactive` with 5s timeout
    - Print appropriate stderr notes per detection outcome
    - Store `kiro_path: Option<PathBuf>`, `logged_in: bool`, `enabled: bool`
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.7, 1.8_

  - [ ] 8.2 Implement AI task orchestration
    - One AI_Task at a time (latest wins, cancel previous)
    - Wire: snapshot → redact → invoke → update pane → append log
    - System prompt construction per Requirement 7
    - Custom question prompt construction
    - _Requirements: 3.1, 3.2, 4.4, 7.1, 7.2, 7.3_

- [ ] 9. Create `src/ui/split_pane.rs` — SplitPaneRenderer
  - [ ] 9.1 Implement `SplitPaneRenderer` struct
    - Compute dimensions: `ai_pane_height = max(6, height * 20 / 100)`, `log_region_height = height - ai_pane_height - 1`
    - `setup()` — set ANSI scroll region `\x1b[1;{log_region_height}r`
    - `write_log(&self, bytes: &[u8])` — write within scroll region
    - `draw_ai_pane(&self, state: &AiPaneState)` — cursor to ai_pane_top_row, clear, draw header + body
    - `draw_status_bar(&self, status: &str)` — write to last row
    - `on_resize(&mut self, w, h)` — recompute, reset scroll region, redraw
    - `teardown()` — reset scroll region `\x1b[r`
    - Fallback_Mode when height < 12
    - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 2.7, 2.8, 2.9_

  - [ ]* 9.2 Write unit tests for SplitPaneRenderer dimension calculations
    - Test dimension computation for various terminal sizes
    - Test fallback mode triggers at height < 12
    - _Requirements: 2.2, 2.3, 2.8_

- [ ] 10. Create `src/ui/mod.rs` — Re-exports
  - Re-export `SplitPaneRenderer` and related types from `split_pane`
  - _Requirements: N/A (structural)_

- [ ] 11. Checkpoint — New modules compile independently
  - Ensure `cargo check` passes with all new `src/ai/` and `src/ui/` modules
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 12. Extend `src/io/keymap.rs` — Replace ExitStateMachine with HotkeyDispatcher
  - [ ] 12.1 Implement `HotkeyAction` enum and `HotkeyDispatcher` struct
    - `HotkeyAction`: `Forward(Vec<u8>)`, `Exit`, `Analyze`, `AskQuestion`, `ShowLastResponse`, `Continue`
    - `HotkeyDispatcher` with `armed: bool` and `ai_enabled: bool`
    - State machine: Armed + `a`/`A` → Analyze, Armed + `q`/`Q` → AskQuestion, Armed + `l`/`L` → ShowLastResponse, Armed + Ctrl+X → Exit
    - When `ai_enabled` is false, AI hotkeys fall through as `Forward([0x01, byte])`
    - Keep `ExitStateMachine` as a type alias or deprecate for backward compat
    - _Requirements: 3.5, 3.6, 3.8, 4.7, 9.5, 15.2_

  - [ ]* 12.2 Write unit tests for HotkeyDispatcher
    - Test all Ctrl+A + letter combinations
    - Test ai_enabled=false disables AI hotkeys
    - Test armed state resets correctly
    - _Requirements: 3.6, 3.8, 15.2_

- [ ] 13. Update `src/cli.rs` — Add AI flags and subcommands
  - Add flags: `--ai-watch` (bool), `--ai-timeout-seconds` (u32, default 30), `--no-redact` (bool), `--no-ai` (bool)
  - Add subcommands: `KiroLogin`, `KiroStatus` to `Subcmd` enum
  - Preserve all existing flags and `List` subcommand unchanged
  - _Requirements: 13.1, 13.2, 13.3, 13.4, 13.5, 14.1, 14.2, 14.3, 14.4_

- [ ] 14. Update `src/errors.rs` — Add AiError variant
  - Add `#[error("AI error: {0}")] AiError(String)` variant to `MadPuttyError`
  - Maps to `ExitCode::General`
  - Non-fatal during sessions (caught by AI task, rendered in pane)
  - _Requirements: design error handling section_

- [ ] 15. Update `src/theme.rs` — Add AI pane styles
  - Add to `Palette`: `ai_header`, `ai_separator`, `ai_spinner`, `ai_error`, `ai_body`
  - Amazon-yellow bold for header, yellow box-drawing for separator, red for errors, white for body
  - Add corresponding plain-mode entries
  - _Requirements: 8.1, 8.2, 8.5_

- [ ] 16. Update `src/session.rs` — Integrate AI subsystem
  - [ ] 16.1 Wire SplitPaneRenderer into session
    - Replace direct stdout writes with `SplitPaneRenderer::write_log` when AI enabled
    - Route colorizer output through renderer's log region
    - Handle resize events to call `renderer.on_resize()`
    - Move status bar to last row via renderer
    - _Requirements: 2.1, 2.5, 2.7, 16.1, 16.2_

  - [ ] 16.2 Spawn AI subsystem tasks
    - Create `RollingBuffer` and fan-out from port_reader via `try_send` on bounded channel(32)
    - Spawn error scanner task consuming from bounded channel
    - Spawn AI task on hotkey trigger or auto-watch trigger
    - Cancel previous AI task when new one starts (latest wins)
    - _Requirements: 3.1, 3.2, 3.3, 5.1, 5.2, 5.3, 16.3, 16.4, 16.5, 16.7_

  - [ ] 16.3 Wire hotkey dispatch
    - Replace `ExitStateMachine` usage with `HotkeyDispatcher`
    - Route `Analyze` → snapshot + AI task
    - Route `AskQuestion` → open input prompt in AI pane
    - Route `ShowLastResponse` → open modal overlay
    - Handle login-state-false case with warning message
    - _Requirements: 3.1, 3.7, 4.1, 4.2, 4.3, 4.4, 4.5, 4.6, 4.7, 4.8, 9.1, 9.2, 9.3, 9.4, 9.6_

  - [ ] 16.4 Manage session lifecycle with AI
    - Detect kiro-cli at session start via `AiSubsystem::detect()`
    - Handle `--no-redact` warning, `--ai-watch` + disabled warning, `--no-ai` override warning
    - Print AI response log path on exit if entries exist
    - Skip AI response log creation when AI disabled
    - _Requirements: 1.5, 1.6, 1.8, 5.7, 6.9, 6.10, 12.5, 12.6, 13.6, 15.1, 15.3, 15.4, 15.5_

- [ ] 17. Update `src/main.rs` — Add mod declarations and subcommand dispatch
  - Add `mod ai;` and `mod ui;` declarations
  - Dispatch `KiroLogin` → spawn `kiro-cli login` with inherited stdio, exit with child's code
  - Dispatch `KiroStatus` → spawn `kiro-cli whoami --no-interactive`, print output, exit appropriately
  - Handle kiro-cli not found for both subcommands
  - _Requirements: 10.1, 10.2, 10.3, 10.4, 10.5, 10.6, 10.7, 14.1, 14.2_

- [ ] 18. Checkpoint — `cargo check` passes
  - Ensure `cargo check` passes with all modifications wired together
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 19. Update README.md with AI flags and hotkey table
  - Document `--ai-watch`, `--ai-timeout-seconds`, `--no-redact`, `--no-ai` flags
  - Document `kiro-login` and `kiro-status` subcommands
  - Add hotkey table: Ctrl+A A (analyze), Ctrl+A Q (question), Ctrl+A L (last response), Ctrl+A X (exit)
  - Document AI pane behavior and auto-watch mode
  - _Requirements: 13.1, 13.2, 13.3, 13.4, 14.1, 14.2_

- [ ] 20. Update PROJECT_OVERVIEW.md with kiro-cli integration section
  - Add section describing the AI subsystem architecture
  - Document the two-lane design and non-blocking guarantee
  - List new modules in `src/ai/` and `src/ui/`
  - _Requirements: N/A (documentation)_

- [ ] 21. Final checkpoint — All tests pass
  - Ensure `cargo check` and `cargo test` pass
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- The design has no Correctness Properties section; property tests are included only where the design's testing strategy explicitly calls for them (Redactor idempotence)
- All new code is additive — existing serial terminal behavior is preserved when AI is disabled
- The `regex` crate is the only new runtime dependency (Requirement 18.2)
