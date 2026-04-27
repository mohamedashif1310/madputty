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
| `--split-pane` | off | Enable split-pane UI (top log / bottom AI). Note: disables terminal scrollback while active. |

### Scrollback behavior

By default, madputty **pins the status bar** to the last row of your terminal and lets logs scroll normally in the region above it. Your terminal's native scrollback still works — scroll up with your mouse or PgUp to see earlier log lines.

AI responses appear inline between log lines (look for the `─── 🤖 AI Analysis ───` separator).

If you prefer a fixed AI pane at the bottom (tmux-style), pass `--split-pane`. The trade-off: while active, terminal scrollback is disabled for the log region. For both worlds, use `--log session.log` to capture everything to a file you can `less` through later.

### Triggering AI analysis

During a session, press **Ctrl+A** (hold Ctrl, press A, release Ctrl), then press **A** again. You'll see `[AI] Analyzing recent logs...` inline, then the response a few seconds later. If nothing happens:

- Check that `kiro-cli whoami` works in the same shell (not just another tab)
- Run with `--verbose 2> debug.log` and check `debug.log` for `kiro-cli stderr: ...` messages
- Try `--ai-timeout-seconds 60` on slow networks



### Authentication requirements

madputty uses `kiro-cli chat --no-interactive --trust-all-tools` under the hood. For this to work you need one of:

- **Interactive login** — run `kiro-cli login` (opens a browser or shows a device code). Your login state is shared with madputty. Verify with `kiro-cli whoami` or `madputty kiro-status`.
- **API key** — set the `KIRO_API_KEY` environment variable to a Kiro Pro/Pro+/Power API key (required for CI/headless use).

If you see "Midway authentication required" inside madputty but `kiro-cli` works fine in another terminal, check:

1. Run `kiro-cli whoami` in the same shell as madputty. If that fails, the shell is missing your Midway cookie (re-login with `mwinit` on Amazon corporate machines, then `kiro-cli login`).
2. Make sure you launched madputty from a shell that inherits your auth environment (don't launch via a clean `cmd` window or a new SSH session without `mwinit`).
3. The `--ai-timeout-seconds` default (30s) may not be enough on slow corporate networks — try `--ai-timeout-seconds 60`.

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
