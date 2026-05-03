# madputty

A picocom-style serial terminal for Windows (and other platforms) that runs inline in your PowerShell or cmd window. Open a COM port, see live device output, type to the device, exit with Ctrl+A Ctrl+X.

## Install

```powershell
cargo install --path .
```

## Usage

```powershell
# Connect to COM3 at 115200 8N1 (the default)
madputty COM3

# Custom baud and framing
madputty COM3 --baud 9600 --data-bits 8 --parity even --stop-bits 1

# List available ports
madputty list
madputty --list

# Mirror device output to a log file (append, never truncate)
madputty COM3 --log session.log

# Send a file to the device once, then drop into interactive mode
madputty COM3 --send bootstrap.txt

# Echo local keystrokes to stdout before sending them to the port
madputty COM3 --echo

# Enable debug tracing to stderr
madputty COM3 --verbose
```

## Flags

| Flag | Short | Default | Description |
| --- | --- | --- | --- |
| `--baud` | `-b` | `115200` | Baud rate (positive integer) |
| `--data-bits` | `-d` | `8` | 5 / 6 / 7 / 8 |
| `--parity` | `-p` | `none` | `none` / `even` / `odd` |
| `--stop-bits` | `-s` | `1` | 1 / 2 |
| `--flow-control` | `-f` | `none` | `none` / `software` / `hardware` |
| `--log FILE` | | | Append all port output to FILE |
| `--send FILE` | | | Write FILE to the port once at startup |
| `--echo` | | `false` | Echo stdin bytes to stdout |
| `--verbose` | | `false` | Debug-level tracing on stderr |
| `--list` | | | List COM ports and exit |

## Exit sequence

Press **Ctrl+A** then **Ctrl+X** to close the port and exit. A lone Ctrl+A followed by any other key forwards both bytes to the device unchanged.

## AI-Powered Log Analysis (via kiro-cli)

When `kiro-cli` is installed and you're logged in, madputty adds a split-pane AI analysis feature:

```powershell
# Connect with AI auto-watch (triggers on error keywords)
madputty COM70 --baud 921600 --ai-watch

# Connect normally, use hotkeys for on-demand analysis
madputty COM70 --baud 921600

# Login to kiro-cli (one-time setup)
madputty kiro-login

# Check kiro-cli status
madputty kiro-status
```

### AI Hotkeys (during a session)

| Hotkey | Action |
| --- | --- |
| **Ctrl+A A** | Analyze last 50 log lines with AI |
| **Ctrl+A Q** | Ask a custom question about the logs ¹ |
| **Ctrl+A L** | Show full last AI response (scrollable) |
| **Ctrl+A X** | Exit session |

¹ Custom question input is not yet wired — currently re-triggers a standard analysis. Full inline prompt coming in a future release.

### AI Flags

| Flag | Default | Description |
| --- | --- | --- |
| `--ai-watch` | off | Auto-trigger AI on error keywords |
| `--ai-timeout-seconds` | `30` | Timeout for each AI call |
| `--no-redact` | off | Disable credential redaction (with warning) |
| `--no-ai` | off | Force AI off even if kiro-cli is installed |

### Scrollback behavior

madputty pins **only the status bar** to the last row of the terminal. The entire area above it is the log region and participates fully in your terminal's native scrollback — scroll up with PgUp / mouse wheel to see any earlier log line, at any time, even while logs keep streaming.

### How AI appears

Pressing **Ctrl+A A** during a session triggers AI analysis of the last 50 log lines. Output appears **inline** in the log stream as a clearly bordered block:

```
─── 🤖 AI Analysis (19:25:08) ──────────────────────────────
  The device booted normally, connected to "Ashif Airtel wifi"
  on channel 1, and started a DING event at 19:25:08. A socket
  read error appeared at 19:49:34 when the ding session ended;
  this is expected RSP protocol cleanup, not a fault.
─── end ────────────────────────────────────────────────────
```

The full response is always printed (word-wrapped to your terminal width), so it's captured in scrollback alongside the log lines that were analyzed. No truncation, no hidden errors — if kiro-cli fails, the actual error (e.g. `KIRO_API_KEY` missing) appears inline in red.

**Hotkeys:**

- `Ctrl+A A` — analyze recent logs
- `Ctrl+A L` — re-print the last response (useful if logs have scrolled it away)
- `Ctrl+A X` — exit

When kiro-cli isn't installed or API key is missing, madputty still works as a plain serial terminal — the AI hotkeys just produce an error line.

### Triggering AI analysis

During a session, press **Ctrl+A** (hold Ctrl, press A, release Ctrl), then press **A** again. You'll see `[AI] Analyzing recent logs...` inline, then the response a few seconds later. If nothing happens:

- Check that `kiro-cli whoami` works in the same shell (not just another tab)
- Run with `--verbose 2> debug.log` and check `debug.log` for `kiro-cli stderr: ...` messages
- Try `--ai-timeout-seconds 60` on slow networks



### Authentication requirements

madputty uses `kiro-cli chat --no-interactive` under the hood. Per the [Kiro CLI headless mode docs](https://kiro.dev/docs/cli/headless/), **headless mode requires `KIRO_API_KEY`** — interactive browser login alone is NOT enough.

**Setup steps (Windows):**

```cmd
REM 1. Install kiro-cli from https://kiro.dev/downloads
REM 2. Generate an API key at https://app.kiro.dev/ (requires Kiro Pro / Pro+ / Power)
REM 3. Set the environment variable in your current shell
set KIRO_API_KEY=kir_xxxxxxxxxxxxxxxxxxxx

REM 4. Verify kiro-cli works headlessly
kiro-cli chat --no-interactive "hello"

REM 5. Run madputty in the same shell
.\target\release\madputty.exe COM66 --baud 921600
```

**Linux / macOS:**

```bash
export KIRO_API_KEY=kir_xxxxxxxxxxxxxxxxxxxx
kiro-cli chat --no-interactive "hello"   # verify first
madputty /dev/ttyUSB0 --baud 921600
```

At startup, madputty warns if `KIRO_API_KEY` is missing. If you press Ctrl+A A and the AI pane shows an error like `kiro-cli error: You must be logged in` or similar, it means the API key is missing from the shell where you launched madputty.

**Troubleshooting:**

1. Test kiro-cli directly first: `kiro-cli chat --no-interactive "ping"`. If THAT fails, madputty can't help it — fix kiro-cli first.
2. Set the env var in the SAME shell as madputty. Env vars are per-shell — setting it in another tab won't help.
3. Slow corporate networks may exceed the 30s timeout. Try `--ai-timeout-seconds 60`.
4. Run madputty with `--verbose 2> debug.log` and check debug.log for `kiro-cli stderr: ...` lines.

### How it works

- Terminal splits: top ~80% shows live logs (never stops), bottom ~20% shows AI analysis
- Credentials (passwords, tokens, IPs, MACs, SSIDs) are redacted before sending to AI
- AI responses are saved to `~/.madputty/ai-responses/<session_id>.md`
- If kiro-cli is not installed, madputty works exactly like before (no AI, no split pane)

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | General error (I/O, invalid argument, etc.) |
| 2 | COM port not found |
| 3 | COM port busy (in use by another process) |

## Banner

When a session starts you'll see a cyan banner showing the port name, baud rate, framing (e.g. `8N1`), and exit reminder, followed by `Port opened`. On exit you'll see `Port closed` and `Exited session`.

## Dependencies

- [`serialport`](https://crates.io/crates/serialport) v4 for COM port I/O
- [`crossterm`](https://crates.io/crates/crossterm) v0.27 for raw-mode stdin so Ctrl+A / Ctrl+X reach madputty instead of being swallowed by the terminal
- [`clap`](https://crates.io/crates/clap) v4, [`tokio`](https://crates.io/crates/tokio) v1, [`tracing`](https://crates.io/crates/tracing), [`thiserror`](https://crates.io/crates/thiserror), [`console`](https://crates.io/crates/console)
