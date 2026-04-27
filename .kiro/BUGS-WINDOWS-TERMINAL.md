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

## Workaround (for now on Windows)

Until fixed, running without AI is the cleanest path:

```powershell
cargo run -- COM66 --baud 921600 --no-ai
```

This skips the split pane entirely and you get normal scrollable serial log output.
