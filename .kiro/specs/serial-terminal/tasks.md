# Implementation Plan: Serial Terminal

## Overview

Transform the `madputty` binary from an SSH/profile launcher into a picocom-style serial terminal. Delete all legacy modules (SSH, vault, OAuth, profiles, batch, PuTTY/TeraTerm, config), trim `Cargo.toml`, and build a new lean crate with `cli`, `serial_config`, `session`, `list`, and `io::{stdout_sink, keymap}` modules. Implementation is Rust 2021 using `serialport` v4 and `crossterm` v0.27 on top of the retained tokio, clap, tracing, thiserror, and console stack. Each task builds on the previous one and ends with a passing `cargo check`.

## Tasks

- [x] 1. Remove legacy source modules
  - Delete `src/api/`, `src/auth/`, `src/commands/`, `src/config/`, `src/profiles/`, `src/terminal/` directories in full
  - _Requirements: 12.1, 12.2_

- [x] 2. Remove legacy tests
  - Delete `tests/auth_properties.rs`, `tests/profile_properties.rs`, `tests/terminal_properties.rs`
  - Delete `tests/integration/` directory and `tests/integration_tests.rs`
  - _Requirements: 12.2_

- [x] 3. Update `Cargo.toml` dependencies
  - Remove `reqwest`, `dialoguer`, `indicatif`, `dirs`, `serde_json`, `toml`, `chrono`, `serde`, `wiremock`, `tokio-test`
  - Add `serialport = "4"` and `crossterm = "0.27"`
  - Keep `clap` v4, `tokio` v1, `tracing`, `tracing-subscriber`, `thiserror`, `console`, `proptest` (dev), `tempfile` (dev)
  - _Requirements: 12.3, 12.4_

- [x] 4. Rewrite `src/errors.rs`
  - [x] 4.1 Define new `MadPuttyError` variants
    - Variants: `PortNotFound { port }`, `PortBusy { port }`, `PortIo(std::io::Error)`, `Serial(serialport::Error)`, `LogFile { path, source }`, `SendFile { path, source }`, `InvalidArg(String)`
    - Derive `Debug` and `thiserror::Error`
    - _Requirements: 1.5, 1.6, 2.6, 6.5, 7.3, 8.2, 8.3, 8.4_
  - [x] 4.2 Define `ExitCode` enum with `Success=0`, `General=1`, `NotFound=2`, `Busy=3`
    - Implement `exit_code(&self) -> ExitCode` mapping variants
    - _Requirements: 8.1, 8.2, 8.3, 8.4_
  - [ ]* 4.3 Write unit tests for error-to-exit-code mapping
    - Cover each variant → expected code
    - _Requirements: 8.2, 8.3, 8.4_

- [x] 5. Rewrite `src/cli.rs`
  - Replace existing SSH-oriented CLI with new `Cli` struct: positional `port: Option<String>`, `--list`, `--baud`/`-b`, `--data-bits`/`-d`, `--parity`/`-p`, `--stop-bits`/`-s`, `--flow-control`/`-f`, `--log`, `--send`, `--echo`, `--verbose`, and `List` subcommand
  - Define `DataBitsArg`, `ParityArg`, `StopBitsArg`, `FlowControlArg` value enums
  - Set defaults: baud 115200, 8N1, no flow control
  - _Requirements: 1.1, 1.2, 1.3, 3.1, 3.2, 3.3, 3.4, 3.5, 5.2, 6.1, 7.1, 7.4, 11.1, 11.2_

- [x] 6. Create `src/serial_config.rs`
  - [x] 6.1 Define `SerialConfig` struct holding baud, data_bits, parity, stop_bits, flow_control as `serialport` enum types
    - Implement `From<&Cli>` to convert CLI arg enums into `serialport` enums
    - Implement `builder(&self, port_name: &str) -> SerialPortBuilder` using 50 ms timeout
    - _Requirements: 1.2, 1.3, 3.1, 3.2, 3.3, 3.4, 3.5, 10.3_
  - [x] 6.2 Implement `framing()` method returning e.g. `"8N1"`
    - Parity letter: `N`/`E`/`O`
    - _Requirements: 9.1_
  - [ ]* 6.3 Write unit tests for `framing()` and `From<&Cli>` conversions
    - Cover all data-bits / parity / stop-bits combinations
    - _Requirements: 3.2, 3.3, 3.4, 9.1_

- [x] 7. Create `src/io/mod.rs` and stdout sink
  - [x] 7.1 Create `src/io/mod.rs` that declares `pub mod stdout_sink;` and `pub mod keymap;`
    - _Requirements: 2.1, 6.1_
  - [x] 7.2 Implement `src/io/stdout_sink.rs::StdoutSink`
    - Holds `std::io::Stdout` handle and optional `std::fs::File`
    - Implements `std::io::Write`: fans every `write` to stdout and, if present, to the log file; flushes stdout on each write
    - Constructor opens log file in append-create mode (never truncate) and maps IO errors to `MadPuttyError::LogFile { path, source }`
    - _Requirements: 2.1, 2.4, 2.5, 6.1, 6.2, 6.3, 6.4, 6.5_
  - [ ]* 7.3 Write unit tests for `StdoutSink` log-append behavior
    - Preseed log file with content, verify new bytes append without truncation using `tempfile`
    - _Requirements: 6.2, 6.3_

- [x] 8. Create `src/io/keymap.rs`
  - [x] 8.1 Define `ForwardOutcome` enum (`Bytes(Vec<u8>)`, `ExitRequested`, `Continue`) and `ExitStateMachine { armed: bool }`
    - `ExitStateMachine::feed(&mut self, bytes: &[u8]) -> ForwardOutcome` implements the Ctrl+A Ctrl+X state machine
    - On 0x01 alone → arm, no bytes emitted
    - On 0x18 while armed → `ExitRequested`
    - On any other byte while armed → disarm, emit `[0x01, byte]`
    - Any byte while disarmed → emit `[byte]`
    - _Requirements: 4.1, 4.2, 4.3_
  - [x] 8.2 Implement `key_event_to_bytes(event: KeyEvent) -> Vec<u8>`
    - Maps `Enter` → `\r`, `Backspace` → `\x7f`, `Tab` → `\t`, `Esc` → `\x1b`
    - `Char(c)` with `CONTROL` modifier → `(c.to_ascii_lowercase() as u8) & 0x1f`
    - `Char(c)` plain → UTF-8 bytes
    - Arrow keys → `\x1b[A`, `\x1b[B`, `\x1b[C`, `\x1b[D`
    - Returns empty vec for non-key events
    - _Requirements: 2.2, 4.3_
  - [ ]* 8.3 Write unit tests for `ExitStateMachine` state transitions
    - Cover Ctrl+A then Ctrl+X, Ctrl+A then other byte, plain bytes, multi-byte batches
    - _Requirements: 4.1, 4.2, 4.3_
  - [ ]* 8.4 Write unit tests for `key_event_to_bytes`
    - Cover Enter, Backspace, Ctrl+letter, plain char, arrow keys
    - _Requirements: 2.2_

- [x] 9. Checkpoint — compile and resolve missing-module errors
  - Run `cargo check`, ensure all modules referenced exist
  - Ensure all tests pass, ask the user if questions arise.
  - _Requirements: 12.2, 12.3_

- [x] 10. Create `src/list.rs`
  - Implement `pub fn run() -> Result<(), MadPuttyError>` that calls `serialport::available_ports()`
  - Print `No COM ports found` and return Ok when empty
  - For each port: print `port_name`; if `SerialPortType::UsbPort`, append manufacturer and product on same line
  - _Requirements: 5.1, 5.3, 5.4, 5.5_

- [x] 11. Create `src/session.rs` — banner and open flow
  - [x] 11.1 Define `SessionOptions { log, send, echo }` and `run(port_name, config, opts) -> Result<(), MadPuttyError>` signature
    - _Requirements: 1.1, 6.1, 7.1, 7.4_
  - [x] 11.2 Pre-flight log and send files before opening the port
    - Open log file with `OpenOptions::new().append(true).create(true)` → map errors to `LogFile`
    - Read send file into `Vec<u8>` via `std::fs::read` → map errors to `SendFile`
    - _Requirements: 6.5, 7.3_
  - [x] 11.3 Open serial port via `config.builder(port_name).open()` with `serialport::Error` mapping
    - `ErrorKind::NoDevice` → `PortNotFound { port }`
    - `ErrorKind::Io(PermissionDenied)` or messages containing "Access is denied" / "in use" → `PortBusy { port }`
    - Anything else → `Serial(err)`
    - _Requirements: 1.5, 1.6, 10.4_
  - [x] 11.4 Print cyan banner via `console::Style::new().cyan()` followed by `Port opened`
    - Banner includes port name, baud rate, framing string, and exit instructions
    - _Requirements: 1.4, 9.1, 9.2, 9.4, 9.5_
  - [x] 11.5 Optional startup send: if send bytes were loaded, `write_all` + `flush` on port before spawning tasks
    - _Requirements: 7.1, 7.2_

- [x] 12. Implement byte-pump tasks in `src/session.rs`
  - [x] 12.1 Clone the port via `try_clone()` into read and write halves
    - _Requirements: 2.3, 10.3_
  - [x] 12.2 Spawn `port_reader` via `tokio::task::spawn_blocking`
    - Owns read half and `StdoutSink`
    - 4 KB buffer, loops calling `port.read`; ignores `ErrorKind::TimedOut`; writes `&buf[..n]` to sink
    - Terminates on shutdown signal (`Arc<AtomicBool>`); on other IO error reports `PortIo` back via `oneshot`
    - _Requirements: 2.1, 2.3, 2.4, 2.5, 2.6, 6.1, 6.4_
  - [x] 12.3 Spawn `port_writer` via `tokio::spawn`
    - Owns write half and `mpsc::UnboundedReceiver<Vec<u8>>`
    - For each frame: `write_all` + `flush`; ends when sender is dropped
    - _Requirements: 2.2, 2.3, 4.3_
  - [x] 12.4 Spawn `input_forwarder` via `tokio::task::spawn_blocking`
    - Constructs a `RawModeGuard` that enables raw mode on new and disables on drop
    - Loops: `crossterm::event::poll(50ms)` then `event::read()`; converts `KeyEvent` via `key_event_to_bytes`; feeds to `ExitStateMachine`
    - On `Bytes(v)`: if `opts.echo`, write v to stdout with flush; then `mpsc::Sender::send(v)`
    - On `ExitRequested`: signal main via `oneshot::Sender<()>` and break loop
    - _Requirements: 2.2, 2.3, 4.1, 4.2, 4.3, 7.4, 7.5_
  - [x] 12.5 Coordinate shutdown in main `select!`
    - Branches: exit oneshot, reader error oneshot, reader join handle
    - On exit: set shutdown atomic, drop mpsc sender, await writer, drop port
    - Print `Port closed` then `Exited session`, return `Ok(())`
    - _Requirements: 2.6, 4.4, 4.5, 9.6_

- [x] 13. Rewrite `src/main.rs`
  - Declare new modules: `mod cli; mod errors; mod io; mod list; mod serial_config; mod session;`
  - Parse CLI, init `tracing_subscriber` with `debug` level when `--verbose` else `warn`, routed to stderr
  - Dispatch: if `--list` or `List` subcommand → `list::run()`; else if `port` is `Some` → `session::run(...).await`; else print help via clap and exit 1
  - On error: format in red with `console::style(err).red()` to stderr; exit with `err.exit_code() as i32`
  - _Requirements: 1.1, 5.1, 5.2, 5.5, 8.1, 8.5, 8.6, 9.3, 9.4, 11.1, 11.2, 11.3, 12.1_

- [x] 14. Update `README.md`
  - Replace SSH/profile descriptions with picocom-style serial terminal usage
  - Document `madputty COM3 --baud 115200`, `--list`, `--log`, `--send`, `--echo`, exit sequence, exit codes
  - _Requirements: 1.1, 4.1, 5.1, 6.1, 7.1, 7.4_

- [x] 15. Remove or rewrite `ARCHITECTURE.md`
  - Delete the legacy document since it describes removed SSH/profile architecture
  - _Requirements: 12.2_

- [x] 16. Final checkpoint — `cargo check` passes cleanly
  - Run `cargo check` on Windows target; fix any remaining compile errors
  - Ensure all tests pass, ask the user if questions arise.
  - _Requirements: 12.2, 12.3, 12.4_

## Notes

- Tasks marked with `*` are optional unit tests and can be skipped for faster MVP.
- Each task references specific sub-requirements for traceability.
- No property-based tests are planned — the design has no "Correctness Properties" section, so unit tests cover the testable state-machine and config logic.
- Checkpoints (tasks 9 and 16) ensure incremental validation via `cargo check`.
