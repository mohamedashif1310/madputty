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
