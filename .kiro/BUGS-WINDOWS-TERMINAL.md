# Windows Terminal Bug Report

Bugs observed running `cargo run -- COM66 --baud 921600` on Windows Terminal (PowerShell). Captured here so Kiro on Mac can pick these up as a bugfix spec.

## Environment
- OS: Windows 11
- Shell: PowerShell / Windows Terminal
- Port: COM66 @ 921600 baud
- Observed: 26 April 2026

---

## Bug 1: Cannot scroll up to see older log lines

### Symptom
Once the split-pane scroll region is active, the user cannot scroll the terminal's scrollback buffer to see log lines that have scrolled off the top of the log region. Mouse wheel and `Shift+PageUp` don't reach any history.

### Likely root cause
`src/ui/split_pane.rs::setup()` sets an ANSI scroll region with `\x1b[1;{log_region_height}r`. Windows Terminal treats this as the *only* scrollable region, and nothing is written to the native scrollback buffer. So any line that leaves the visible top of the log region is gone forever.

### Expected
Either
- log lines that leave the top of the log region should be preserved in the terminal scrollback, OR
- the AI pane should be drawn *above* the log output (banner-style) so the log lines flow naturally through Windows Terminal's own scrollback

### Files to investigate
- `src/ui/split_pane.rs` — scroll region setup
- `src/session.rs` — how log writes are routed through `SplitPaneRenderer::write_log`

---

## Bug 2: Live logs barely visible / get pushed out by status bar and AI pane

### Symptom
At 921600 baud with a chatty device, the log region shows only a few lines before they scroll off. The AI pane (20% of screen) plus the status bar eat a big chunk of vertical space on a normal-height terminal. Combined with bug 1, logs are effectively unreadable in real time.

### Likely root cause
- Fixed 20% of terminal height allocated to AI pane regardless of whether it's populated
- Status bar always drawn on the last row
- No way to toggle or collapse the AI pane

### Expected
- AI pane should collapse to a single-line hint (e.g. `▌ Ctrl+A A for AI analysis`) when there's no active AI content, freeing log real estate
- Optionally: a hotkey (e.g. `Ctrl+A P`) to toggle the AI pane on/off
- When AI pane expands on trigger, it can grow to 20% — but not before

### Files to investigate
- `src/ui/split_pane.rs::new()` — `ai_pane_height = (height * 20 / 100).max(6)` is always applied
- `src/session.rs` — session setup calls `SplitPaneRenderer::new` unconditionally when AI enabled

---

## Bug 3: AI analyser output not good / not visible

### Symptom
Triggering `Ctrl+A A` does not produce a visible, useful AI response in the pane. Either:
- the pane stays blank/spinner-stuck, OR
- the response is truncated and user cannot read it

### Likely root cause (multiple possible)
1. `kiro-cli` invocation — even with the recent `--trust-all-tools` fix, the corporate / Midway auth path may be failing silently and the error is being rendered inside the tiny AI pane where it's invisible or truncated
2. AI pane body rendering truncates long responses to the 20% region with no scroll / no modal trigger
3. `Ctrl+A L` (show last response modal) may not actually open the modal in all paths, so long responses have no viewing surface

### Expected
- Clear spinner → clear response text, scrollable within the pane OR openable in full-screen modal
- Any error from kiro-cli should be shown with full stderr text, not just the first line
- Even if the pane is tiny, the full response must be reachable via the modal (`Ctrl+A L`)

### Files to investigate
- `src/ai/kiro_invoker.rs` — does it surface the full stderr, or just first line?
- `src/ai/mod.rs::AiSubsystem` — task orchestration, response delivery to pane
- `src/ai/pane.rs` — modal open logic, body truncation
- `src/ui/split_pane.rs::draw_ai_pane` — body rendering and truncation behavior

---

## Bug 4: Windows Terminal ANSI quirks

### Symptom
The box-drawing characters, cursor save/restore (`\x1b7` / `\x1b8`), and scroll region (`\x1b[1;Nr`) may behave differently on:
- Windows Terminal (best)
- Windows ConHost / classic cmd.exe (worst)
- VS Code integrated terminal
- PowerShell ISE (no raw mode at all)

At minimum, there should be a runtime check that the host terminal supports the required features, and a fallback to non-split mode when it doesn't.

### Expected
- Detect lack of ANSI / scroll region support on Windows and fall back to plain-mode (already exists at `height < 12`, but should also trigger for hosts that swallow `\x1b[r`)
- Document which Windows terminals are supported in README

### Files to investigate
- `src/ui/split_pane.rs::new()` — fallback criteria
- `src/session.rs` — where to add host detection

---

## Suggested bugfix spec scope

Group these into one bugfix spec titled **"windows-terminal-ux-fixes"** with 4 bug conditions:

1. `C_1(X)`: After N log lines exceed visible log region, user cannot retrieve any of them
2. `C_2(X)`: At T ≥ 0, AI pane occupies ≥ 20% of screen even when it has no content
3. `C_3(X)`: After AI trigger, full response text is not reachable by the user
4. `C_4(X)`: On a Windows host that does not support `\x1b[1;Nr`, split pane still activates and corrupts output

Priority: **P0** for bug 1 and 3, **P1** for bug 2, **P2** for bug 4.

---

## FIX SUMMARY (April 30, 2026, macOS side)

### P0 Bug 1 (scrollback) — FIXED

The split-pane scroll-region design was retired. The renderer now uses only a
single-row scroll region (the status bar on the last row). Rows 1..N-1 are the
log region and fully participate in the terminal's native scrollback. Users can
scroll up with PgUp / mouse wheel to see any earlier log line, at any time,
even while logs keep arriving.

### P0 Bug 3 (AI not visible / truncated) — FIXED

AI output is no longer confined to a tiny fixed pane. Pressing Ctrl+A A now:
1. Prints a visible inline spinner line (`🤖 Analyzing recent logs...`)
2. When the response arrives, prints a bordered inline block with the FULL
   word-wrapped response, captured in scrollback just like a normal log line
3. Errors (KIRO_API_KEY missing, timeout, etc.) are printed with the full
   first line of kiro-cli's stderr, not hidden behind a generic message
4. Ctrl+A L re-prints the last response at the current cursor position,
   useful if logs have scrolled it out of view

### P1 Bug 2 (pane wastes screen space) — FIXED AS SIDE EFFECT

With the fixed pane gone, 100% of non-status vertical space is logs until the
user actively triggers AI analysis.

### P2 Bug 4 (ANSI quirks) — MITIGATED

Only one feature is required now: the single-row scroll region (`\x1b[1;Nr`
where N = height - 1). This is supported by Windows Terminal, ConHost,
ConEmu, PowerShell ISE with VT-enabled, the VS Code integrated terminal,
and all Unix terminals. The `--plain` flag remains available for any host
that mishandles it.



---

## Workaround (for now on Windows)

Until fixed, running without AI is the cleanest path:

```powershell
cargo run -- COM66 --baud 921600 --no-ai
```

This skips the split pane entirely and you get normal scrollable serial log output.


---

# ROUND 3 — NEW ISSUES (3 May 2026, Windows)

After commit `0cd7bf7` (retire split-pane, inline AI). Tested on
Windows 11 / Windows Terminal, COM66 @ 921600 baud, binary built
from `.\target\release\madputty.exe`.

---

## Bug 5 — P0: AI banner appears ONCE at session start and never updates

### Symptom
At session start, immediately after the boot sequence finishes, the following
line is printed exactly once and never updates:

```
🤖 AI ready — press Ctrl+A A to analyze recent logs
```

After that, serial logs stream continuously and the AI banner is scrolled
off screen within seconds. There is no visible AI pane, no bordered inline
block, no spinner, and no sign that AI is active.

Pressing Ctrl+A A has no observable effect — **no** `🤖 Analyzing recent logs...`
spinner line is printed, **no** response block appears, **no** error message
appears. It is indistinguishable from the hotkey being dropped entirely.

Ctrl+A X still works.

### What the user saw
- Full terminal height is used for log lines — this part is correct ✅
- AI "ready" hint printed once before logs, then disappeared up the scrollback
- Ctrl+A A produced nothing in the output stream over the entire 3+ minute session
- Status bar at the bottom stayed pinned correctly ✅ (side effect: the previous
  round's status-bar fix is working)

### Likely root causes (multiple candidates — Mac should investigate)

1. **Ctrl+A A not firing at all (Windows key-event duplication).**
   Previously diagnosed in the P0 bug I filed before. The Mac commit message
   said the split-pane retirement fixed both scrollback and AI visibility,
   but the underlying Windows KeyEventKind duplication may still be causing
   the dispatcher to never see the `[CTRL_A, b'a']` sequence. If so:
   - Ctrl+A A produces NO output because `HotkeyAction::Analyze` never fires
   - The `[AI] Analyzing recent logs...` eprintln in session.rs line ~775 never runs
   - Mac could not have confirmed the fix because macOS does not have this
     key-event duplication behavior

2. **Ctrl+A A fires but AI task silently errors.**
   If KIRO_API_KEY is missing, the kiro_invoker should now print the real
   stderr inline. If this is broken:
   - The spinner line fires but the error path does NOT print
   - Or the error is printed to stderr while the log pump writes to stdout,
     and the two streams interleave in ways that lose the error

3. **Inline AI writes go to a stream the user does not see.**
   If the inline block is written to stderr while the log pump writes to
   stdout, the Windows Terminal may not render them on the same scroll
   track. User would only see the log stream and never the AI output.

### Expected
When I press Ctrl+A A on Windows:

- Within 500ms, a visible `🤖 Analyzing recent logs...` line must appear inline
  in the log stream (stdout, not stderr)
- Within `--ai-timeout-seconds` (default 30s), either:
  - a bordered inline block with the full AI response, OR
  - a visible red error line (e.g. `[AI error] KIRO_API_KEY not set. See https://...`)
- All of the above must be captured in Windows Terminal scrollback

### Debugging steps the Mac should take

1. **Verify Ctrl+A A fires at all on Windows.** Add a `tracing::info!` (not
   debug) at the top of the `HotkeyAction::Analyze` match arm in
   `src/session.rs` so it always shows even without `--verbose`. Ask user
   to run with the binary, press Ctrl+A A, exit, and confirm whether the
   tracing line appeared. This distinguishes candidate 1 from 2/3.

2. **Filter crossterm KeyEventKind on Windows.** In the input_forwarder
   loop, ignore `KeyEventKind::Release`:
   ```rust
   if let Event::Key(ke) = &evt {
       if ke.kind != crossterm::event::KeyEventKind::Press {
           continue;
       }
   }
   ```
   Add a test simulating duplicate `0x01` bytes (Press then Release of
   Ctrl+A) and confirm Analyze only fires once.

3. **Verify AI inline writes go to stdout.** Grep `src/session.rs` for
   where the inline `🤖 Analyzing...` and bordered block are printed.
   Confirm they use `println!` / `print!` (stdout) or explicit writes to
   `io::stdout()`, not `eprintln!`. If stderr, change to stdout.

4. **Redact `--verbose` stderr from the user's log stream.** With
   `--verbose`, tracing writes to stderr. On Windows Terminal, stderr
   and stdout appear on the same scroll track but their ordering with
   respect to the log pump is non-deterministic. This may explain why
   the user did not see `[AI] Analyzing recent logs...` in the flood —
   if it was there, it may have been buried.

### Files to investigate (in order)
1. `src/session.rs` — the input_forwarder loop and the `HotkeyAction::Analyze`
   arm around line 772-780. Check if the acknowledgment line is written to
   stdout or stderr.
2. `src/io/keymap.rs` — confirm HotkeyDispatcher handles Windows Press+Release
   byte duplication (may need a test case with `[0x01, 0x01, b'a']`).
3. `src/ai/mod.rs` — verify the AI task's success and error paths both write
   visible output, and to the same stream as the log pump.
4. `src/ui/split_pane.rs` or wherever the inline AI block renderer lives now —
   confirm it actually draws anything.

### Priority
**P0** — the core feature of madputty is still unreachable on Windows after
the round 2 fix. The previous fix addressed UX geometry (pane size, scrollback,
status bar) but the AI trigger path itself appears to still be broken on
Windows specifically.

---

## Suggested bugfix spec for round 3

Single bug: **"windows-ai-trigger-no-op-after-pane-retirement"**

Bug condition `C_5(X)`: On Windows, after pressing Ctrl+A A at time T, no
visible output is emitted to stdout within 500ms referencing AI analysis,
a spinner, or an error. On macOS/Linux, the same action emits a visible
`🤖 Analyzing...` line at or before T+100ms.

Preservation check: Ctrl+A X still exits cleanly on Windows.

Fix check: After fix, Ctrl+A A on Windows produces a visible stdout line
(`🤖 Analyzing recent logs...` or the error variant) that appears in the
terminal's scrollback.

---

## Workaround for round 3

None — `--no-ai` disables the feature entirely. User cannot reach AI
analysis from within madputty on Windows.
