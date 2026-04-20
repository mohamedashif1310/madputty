//! Integration test for the log pump's non-blocking guarantee.
//!
//! The full contract: while a slow AI task is running, bytes from the
//! serial port must keep flowing to stdout with no perceptible stall.
//! Proving that requires injecting a mock duplex stream in place of the
//! concrete `serialport::SerialPort`, which is not yet wired.
//!
//! This file ships the smoke harness (process spawn + clean exit) now so
//! the test infrastructure is in place, and keeps the full assertion as
//! `#[ignore]` until the (ide) serial-mock task lands.

use std::process::Command;
use std::time::{Duration, Instant};

/// Path to the built `madputty` binary (provided by cargo at test time).
const MADPUTTY: &str = env!("CARGO_BIN_EXE_madputty");

/// Smoke test — `madputty --list` exits cleanly and promptly under --plain.
///
/// This proves the binary links, the CLI parses, and the list subcommand
/// returns without hanging. It's a floor for everything else.
#[test]
fn list_subcommand_exits_promptly() {
    let start = Instant::now();
    let output = Command::new(MADPUTTY)
        .args(["--plain", "list"])
        .output()
        .expect("failed to spawn madputty");

    let elapsed = start.elapsed();
    assert!(
        output.status.success(),
        "madputty --plain list failed: status={:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    // 5s ceiling is generous; typical is <500ms.
    assert!(
        elapsed < Duration::from_secs(5),
        "madputty --plain list took {elapsed:?}, expected <5s"
    );
}

/// Smoke test — help/usage exits cleanly.
#[test]
fn help_exits_promptly() {
    let start = Instant::now();
    let output = Command::new(MADPUTTY)
        .arg("--help")
        .output()
        .expect("failed to spawn madputty");
    let elapsed = start.elapsed();

    assert!(
        output.status.success(),
        "madputty --help failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "madputty --help took {elapsed:?}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("madputty") || stdout.contains("Usage"),
        "help output looks wrong: {stdout}"
    );
}

/// Full non-blocking pump assertion.
///
/// Contract: while a slow AI task runs, the log pump continues draining
/// the serial stream with no stall longer than one poll cycle.
///
/// Blocked on: `SerialPort` abstraction / mock duplex stream in session.rs
/// (see `(ide)` task "Extract a SerialPort-like trait…" in .kiro/tasks.md).
///
/// Planned assertion:
/// 1. Spawn `madputty COMMOCK --plain --ai-timeout-seconds 30`
/// 2. Inject a stream that emits one byte every 1ms for 5s.
/// 3. Mid-stream, trigger an AI call that blocks for 3s.
/// 4. Assert stdout byte-arrival inter-gap stays <50ms (no multi-second pause).
#[test]
#[ignore = "blocked on SerialPort abstraction (see .kiro/tasks.md (ide) task)"]
fn bytes_keep_flowing_during_slow_ai_task() {
    unimplemented!("serial mock harness not yet available");
}
