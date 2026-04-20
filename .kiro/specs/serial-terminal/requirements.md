# Requirements Document

## Introduction

This feature pivots the existing madputty CLI from an SSH/vault profile launcher that spawns external PuTTY and TeraTerm windows into a picocom-style serial terminal that runs entirely inside the current PowerShell or cmd window on Windows. The new tool opens a Windows COM port, streams device output live to stdout, forwards user keystrokes from stdin to the device, and exits on the picocom-compatible Ctrl+A Ctrl+X sequence. The defining behavior is a bidirectional byte pump between the local terminal and the serial port, with optional file logging, startup file injection, and COM port discovery.

All SSH, vault, profile, batch, and external-terminal-launcher functionality is being removed. The existing Rust edition 2021 project, clap v4 CLI parsing, tokio runtime, tracing-based logging, thiserror error type, and console crate coloring are retained and extended with the `serialport` and `crossterm` crates.

## Glossary

- **Madputty**: The CLI binary produced by this crate. Entry point for all commands described in this document.
- **Serial_Session**: A single live connection between Madputty and one COM port, lasting from port open to port close.
- **COM_Port**: A Windows serial port identifier of the form `COM<N>` where N is an integer from 1 to 255, including USB-to-serial adapter ports.
- **Port_Reader**: The component inside Madputty that continuously reads bytes from the COM port and writes them to stdout and optionally to a log file.
- **Input_Forwarder**: The component inside Madputty that reads keystrokes from stdin and writes the corresponding bytes to the COM port.
- **Port_Lister**: The component that enumerates available COM ports on the host system and prints each port name along with any available device metadata.
- **Exit_Sequence**: The two-keystroke sequence Ctrl+A followed by Ctrl+X, interpreted by Madputty as a request to terminate the Serial_Session without forwarding those bytes to the device.
- **Serial_Parameters**: The tuple of baud rate, data bits, parity, stop bits, and flow control applied when opening a COM port.
- **8N1**: Shorthand for 8 data bits, no parity, 1 stop bit. Used as the default Serial_Parameters framing.
- **Log_File**: An optional file path supplied with `--log` to which all bytes received from the COM port are appended in addition to being printed to stdout.
- **Send_File**: An optional file path supplied with `--send` whose contents are written to the COM port once, immediately after the port is opened, before interactive input begins.
- **Local_Echo**: An optional mode enabled by `--echo` in which bytes read from stdin are also printed to stdout before being written to the COM port.
- **Banner**: The multi-line header printed to stdout when a Serial_Session starts, showing the port name, Serial_Parameters, and exit instructions.

## Requirements

### Requirement 1: Open a Serial Session on a Specified Port

**User Story:** As an embedded developer, I want to run `madputty COM3 --baud 115200` and immediately see my microcontroller's debug output, so that I can debug without opening a separate terminal app.

#### Acceptance Criteria

1. WHEN the user invokes `madputty <PORT>` with a single positional port argument, THE Madputty SHALL open the specified COM_Port and start a Serial_Session.
2. WHERE the `--baud` option is omitted, THE Madputty SHALL open the COM_Port at 115200 baud.
3. WHERE the `--data-bits`, `--parity`, `--stop-bits`, and `--flow-control` options are all omitted, THE Madputty SHALL open the COM_Port with 8 data bits, no parity, 1 stop bit, and no flow control.
4. WHEN a Serial_Session starts successfully, THE Madputty SHALL print the Banner to stdout before forwarding any device bytes.
5. IF the specified COM_Port does not exist on the host system, THEN THE Madputty SHALL print an error message naming the port and exit with exit code 2.
6. IF the specified COM_Port exists but is already opened by another process, THEN THE Madputty SHALL print an error message stating the port is busy and exit with exit code 3.

### Requirement 2: Bidirectional Byte Pump Between Stdin/Stdout and COM Port

**User Story:** As an embedded developer, I want the terminal to concurrently stream device output and forward my keystrokes, so that I can interact with a running firmware REPL in real time.

#### Acceptance Criteria

1. WHILE a Serial_Session is active, THE Port_Reader SHALL continuously read bytes from the COM_Port and write each received byte to stdout without buffering beyond one line.
2. WHILE a Serial_Session is active, THE Input_Forwarder SHALL read keystrokes from stdin and write the corresponding bytes to the COM_Port.
3. THE Madputty SHALL perform Port_Reader and Input_Forwarder work concurrently so that neither side blocks the other.
4. THE Port_Reader SHALL write bytes received from the COM_Port to stdout as raw bytes without requiring the data to be valid UTF-8.
5. WHEN the COM_Port returns valid UTF-8 byte sequences, THE Port_Reader SHALL preserve the sequences unchanged in the stdout output.
6. IF a read from the COM_Port returns an error other than a timeout, THEN THE Madputty SHALL print the error message to stderr, close the COM_Port, and exit with exit code 1.

### Requirement 3: Configurable Serial Parameters via CLI Flags

**User Story:** As a hardware engineer, I want to override baud rate and framing on the command line, so that I can connect to devices that use non-default serial settings.

#### Acceptance Criteria

1. WHERE the user supplies `--baud <RATE>` or `-b <RATE>` with a positive integer, THE Madputty SHALL open the COM_Port at the supplied baud rate.
2. WHERE the user supplies `--data-bits <N>` or `-d <N>` with a value in the set {5, 6, 7, 8}, THE Madputty SHALL configure the COM_Port with the supplied number of data bits.
3. WHERE the user supplies `--parity <VALUE>` or `-p <VALUE>` with a value in the set {none, even, odd}, THE Madputty SHALL configure the COM_Port with the supplied parity mode.
4. WHERE the user supplies `--stop-bits <N>` or `-s <N>` with a value in the set {1, 2}, THE Madputty SHALL configure the COM_Port with the supplied number of stop bits.
5. WHERE the user supplies `--flow-control <VALUE>` or `-f <VALUE>` with a value in the set {none, software, hardware}, THE Madputty SHALL configure the COM_Port with the supplied flow control mode.
6. IF the user supplies a value for `--data-bits`, `--parity`, `--stop-bits`, or `--flow-control` that is outside its defined set, THEN THE Madputty SHALL print an error identifying the invalid flag and value and exit with exit code 1.
7. IF the user supplies a `--baud` value that is not a positive integer, THEN THE Madputty SHALL print an error identifying the invalid baud rate and exit with exit code 1.

### Requirement 4: Picocom-Compatible Exit Sequence

**User Story:** As a picocom user, I want the familiar Ctrl+A Ctrl+X exit sequence, so that muscle memory carries over from Linux.

#### Acceptance Criteria

1. WHEN stdin produces the byte sequence Ctrl+A (0x01) followed by Ctrl+X (0x18) during an active Serial_Session, THE Madputty SHALL terminate the Serial_Session.
2. WHEN the Exit_Sequence is recognized, THE Madputty SHALL NOT forward the Ctrl+A or Ctrl+X bytes to the COM_Port.
3. WHEN stdin produces a Ctrl+A byte that is followed by any byte other than Ctrl+X, THE Input_Forwarder SHALL forward both the Ctrl+A byte and the following byte to the COM_Port in the original order.
4. WHEN a Serial_Session terminates due to the Exit_Sequence, THE Madputty SHALL close the COM_Port before exiting.
5. WHEN a Serial_Session terminates due to the Exit_Sequence, THE Madputty SHALL print the message `Exited session` to stdout and exit with exit code 0.

### Requirement 5: List Available COM Ports

**User Story:** As a hardware engineer, I want to list available COM ports with `madputty list`, so that I can quickly find which port my device is connected to.

#### Acceptance Criteria

1. WHEN the user invokes `madputty list`, THE Port_Lister SHALL print every COM_Port currently enumerable on the host system to stdout, one port per line.
2. WHEN the user invokes `madputty --list`, THE Madputty SHALL behave identically to `madputty list`.
3. WHERE the host operating system exposes device metadata for a COM_Port, THE Port_Lister SHALL print the metadata on the same line as the port name.
4. WHEN no COM_Port is enumerable on the host system, THE Port_Lister SHALL print the message `No COM ports found` to stdout and exit with exit code 0.
5. WHEN the COM_Port listing completes successfully, THE Madputty SHALL exit with exit code 0.

### Requirement 6: Mirror Port Output to a Log File

**User Story:** As a developer, I want to log serial output to a file while watching live, so that I can review logs later without missing the live output.

#### Acceptance Criteria

1. WHERE the user supplies `--log <FILE>`, THE Port_Reader SHALL append every byte received from the COM_Port to the supplied Log_File in addition to writing it to stdout.
2. WHEN the Log_File does not exist at Serial_Session start, THE Madputty SHALL create the Log_File before the first byte is received.
3. WHEN the Log_File already exists at Serial_Session start, THE Madputty SHALL append new bytes without truncating existing content.
4. WHERE `--log <FILE>` is not supplied, THE Port_Reader SHALL write received bytes only to stdout.
5. IF the Log_File cannot be opened or created due to a filesystem error, THEN THE Madputty SHALL print an error identifying the file path and exit with exit code 1 before opening the COM_Port.

### Requirement 7: Startup File Injection and Local Echo

**User Story:** As a developer, I want to send a batch of commands from a file at startup and optionally echo my keystrokes locally, so that I can automate device provisioning and work with devices that do not echo.

#### Acceptance Criteria

1. WHERE the user supplies `--send <FILE>`, THE Madputty SHALL write the full contents of the Send_File to the COM_Port once, after the port is opened and before any stdin input is read.
2. WHEN the Send_File has been fully written to the COM_Port, THE Madputty SHALL continue the Serial_Session with normal Input_Forwarder behavior.
3. IF the Send_File does not exist or cannot be read, THEN THE Madputty SHALL print an error identifying the file path and exit with exit code 1 before opening the COM_Port.
4. WHERE the user supplies `--echo`, THE Input_Forwarder SHALL write each byte read from stdin to stdout before writing the byte to the COM_Port.
5. WHERE `--echo` is not supplied, THE Input_Forwarder SHALL write stdin bytes only to the COM_Port and not to stdout.

### Requirement 8: Error Handling and Exit Codes

**User Story:** As a CLI user, I want clear errors and predictable exit codes, so that I can script madputty reliably and diagnose failures quickly.

#### Acceptance Criteria

1. WHEN the Madputty exits successfully after a Serial_Session or a list operation, THE Madputty SHALL use exit code 0.
2. IF a general error occurs that is not covered by a more specific exit code, THEN THE Madputty SHALL use exit code 1.
3. IF the requested COM_Port does not exist on the host system, THEN THE Madputty SHALL use exit code 2.
4. IF the requested COM_Port exists but is in use by another process, THEN THE Madputty SHALL use exit code 3.
5. WHEN the Madputty prints an error message for any non-zero exit, THE Madputty SHALL write the message to stderr.
6. THE Madputty SHALL print error messages in a single line that names the failing resource, for example the port name or file path.

### Requirement 9: Visual Banner and Colored Output

**User Story:** As a CLI user, I want a clear startup banner and color-coded messages, so that I can see connection state and errors at a glance.

#### Acceptance Criteria

1. WHEN a Serial_Session starts, THE Madputty SHALL print a Banner that includes the COM_Port name, the baud rate, the framing string in the form `<data-bits><parity-letter><stop-bits>`, and the Exit_Sequence instructions.
2. THE Madputty SHALL render the Banner text in cyan using the console crate when stdout is a terminal.
3. THE Madputty SHALL render error messages in red using the console crate when stderr is a terminal.
4. WHEN stdout or stderr is not a terminal, THE Madputty SHALL omit ANSI color codes from the corresponding stream.
5. WHEN a Serial_Session starts, THE Madputty SHALL print the message `Port opened` on its own line after the Banner.
6. WHEN a Serial_Session ends for any reason, THE Madputty SHALL print the message `Port closed` on its own line before exiting.

### Requirement 10: Windows COM Port Support

**User Story:** As a Windows developer, I want madputty to support the full range of Windows COM port names including USB-to-serial adapters, so that I can use it with any device I plug into my machine.

#### Acceptance Criteria

1. THE Madputty SHALL accept COM_Port names of the form `COM<N>` where N is an integer from 1 through 255.
2. THE Madputty SHALL open COM_Ports backed by USB-to-serial adapters using the same interface as built-in COM_Ports.
3. THE Madputty SHALL use the `serialport` crate for all COM_Port open, read, write, and close operations.
4. WHEN the user supplies a port name that is syntactically valid but not present on the host, THE Madputty SHALL treat the port as non-existent and apply the behavior defined in Requirement 1 criterion 5.

### Requirement 11: Verbose Debug Logging

**User Story:** As a developer debugging madputty itself, I want a verbose flag that turns on tracing output, so that I can see internal state transitions without modifying the code.

#### Acceptance Criteria

1. WHERE the user supplies `--verbose`, THE Madputty SHALL enable `tracing` output at the `debug` level for the `madputty` target on stderr.
2. WHERE `--verbose` is not supplied, THE Madputty SHALL enable `tracing` output at the `warn` level for the `madputty` target on stderr.
3. THE Madputty SHALL route all `tracing` output to stderr and SHALL NOT mix tracing output into stdout.

### Requirement 12: Removal of Legacy Functionality

**User Story:** As a maintainer, I want the legacy SSH, vault, profile, and external-terminal functionality removed from the binary, so that madputty's surface area matches its new purpose.

#### Acceptance Criteria

1. THE Madputty SHALL NOT expose the `init`, `connect`, `batch`, `vault`, `auth`, `profiles`, or `config` subcommands on the CLI.
2. THE Madputty crate SHALL NOT contain source modules for SSH or vault authentication, PuTTY launching, TeraTerm launching, connection profiles, or profile-based batch connections.
3. THE Madputty crate SHALL NOT declare runtime dependencies on `reqwest`, `dialoguer`, `indicatif`, `dirs`, `serde_json`, `toml`, or `chrono`.
4. THE Madputty crate SHALL declare runtime dependencies on `serialport` version 4 and `crossterm` version 0.27 in addition to the retained dependencies `clap` v4, `tokio` v1, `tracing`, `tracing-subscriber`, `thiserror`, and `console`.
