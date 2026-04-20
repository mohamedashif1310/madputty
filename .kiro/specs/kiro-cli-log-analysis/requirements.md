# Requirements Document

## Introduction

This feature extends the existing `madputty` Windows serial terminal with live AI-powered log analysis backed by Amazon's `kiro-cli` assistant. While the user watches device output stream in real time, `madputty` can send a rolling window of recent log lines to `kiro-cli chat` and render the AI's explanation in a dedicated pane beneath the log stream, without ever interrupting the serial byte pump. Analysis can be triggered manually with hotkeys, on-demand with a custom user question, or automatically when suspicious keywords such as `ERROR`, `FAIL`, or `PANIC` appear in the stream.

The feature is designed so that `madputty` remains fully functional in environments where `kiro-cli` is absent, not logged in, or explicitly disabled. It also treats every outbound AI call as a security boundary: device output is passed through a redaction filter that strips credentials, tokens, IP and MAC addresses, and SSID values before any byte leaves the host. Saved AI responses are accumulated per session to `~/.madputty/ai-responses/<session_id>.md` for later review.

All new functionality builds on the existing tokio runtime, `crossterm` raw-mode input, and `console`-based styling, and adds `regex` as the only new runtime dependency. The serial port reader and writer tasks keep their existing non-blocking semantics; AI work happens on independently scheduled tokio tasks with bounded channels so that a slow or hung `kiro-cli` process can never stall the log stream.

## Glossary

- **Madputty**: The existing CLI binary produced by this crate. The new AI features are added to this binary; no new binary is introduced.
- **Kiro_CLI**: The external executable `kiro-cli` (or `kiro-cli.exe` on Windows) that provides Amazon's AI assistant. Madputty invokes it as a child process using `tokio::process::Command`.
- **Kiro_Detector**: The Madputty component that resolves `kiro-cli` on PATH, records the resolved absolute path, and probes login state at startup.
- **AI_Subsystem**: The collection of Madputty components that orchestrate AI analysis, including the Kiro_Detector, Rolling_Buffer, Redactor, Kiro_Invoker, and AI_Pane.
- **AI_Features_Enabled**: The boolean state that is true when Kiro_CLI is present on PATH, the user is logged in, and the user has not passed `--no-ai`. When true, split-pane UI and AI hotkeys are active.
- **AI_Features_Disabled**: The boolean state that is true when AI_Features_Enabled is false. When true, Madputty behaves exactly like the pre-feature serial terminal.
- **Split_Pane_Layout**: The terminal layout in which the top region renders the live log stream and the bottom region renders the AI_Pane, with the existing status bar moved to the last row of the terminal.
- **Log_Region**: The upper portion of the Split_Pane_Layout that displays the live serial log stream. Occupies approximately 80% of terminal height.
- **AI_Pane**: The lower portion of the Split_Pane_Layout that displays the AI analysis output, with a minimum height of 6 rows and an approximate height of 20% of terminal height.
- **Status_Bar**: The existing single-line status indicator showing PORT, BAUD, UP, RX, TX, and RATE, which is relocated to the last row of the terminal beneath the AI_Pane when Split_Pane_Layout is active.
- **Rolling_Buffer**: A fixed-capacity in-memory structure that retains the most recent 50 decoded log lines from the serial stream. Snapshotted on demand without blocking the serial port reader.
- **Log_Snapshot**: A point-in-time copy of the Rolling_Buffer contents, taken before a Kiro_CLI invocation and held by a single AI task for the duration of that invocation.
- **Redactor**: The component that applies the Redaction_Ruleset to a Log_Snapshot or user-supplied text before it is forwarded to Kiro_CLI.
- **Redaction_Ruleset**: The set of regex-based redaction patterns defined in Requirement 6 and applied by the Redactor.
- **Kiro_Invoker**: The component that spawns Kiro_CLI as a child process with the redacted prompt, enforces a timeout, and delivers the AI response to the AI_Pane.
- **AI_Task**: A single tokio task that executes one Kiro_CLI invocation from prompt construction through response rendering.
- **System_Prompt**: The fixed analyst instruction defined in Requirement 7, prepended to every default AI analysis request.
- **Manual_Analyze_Hotkey**: The keystroke sequence Ctrl+A followed by A, which triggers on-demand AI analysis of the Rolling_Buffer.
- **Custom_Question_Hotkey**: The keystroke sequence Ctrl+A followed by Q, which opens an inline prompt in the AI_Pane for a user-entered question.
- **Last_Response_Hotkey**: The keystroke sequence Ctrl+A followed by L, which opens the most recent AI response in a scrollable modal over the AI_Pane.
- **Auto_Watch_Mode**: The mode enabled by the `--ai-watch` flag in which the AI_Subsystem automatically triggers analysis when an Error_Pattern is detected.
- **Error_Pattern**: Any of the case-sensitive substrings `ERROR`, `FAIL`, `FAILED`, `PANIC`, `EXCEPTION`, `TIMEOUT`, or the log-level marker ` E ` (space, capital E, space) within a log line.
- **Auto_Trigger_Debounce**: The 30-second minimum interval between successive automatic AI triggers in Auto_Watch_Mode.
- **AI_Timeout**: The maximum wall-clock duration allowed for a single Kiro_CLI invocation. Default 30 seconds. Configurable via `--ai-timeout-seconds`.
- **Session_Id**: A timestamp-derived identifier assigned at Madputty startup that names the per-session AI response log file. Format: `YYYYMMDD-HHMMSS-<port>`.
- **AI_Response_Log**: The Markdown file at `~/.madputty/ai-responses/<Session_Id>.md` that accumulates timestamped AI responses across the session.
- **Kiro_Login_Subcommand**: The `madputty kiro-login` subcommand that delegates to `kiro-cli login`.
- **Kiro_Status_Subcommand**: The `madputty kiro-status` subcommand that delegates to `kiro-cli whoami --no-interactive`.
- **Fallback_Mode**: The mode used when the terminal height is less than 12 rows; in this mode Split_Pane_Layout is not drawn and Madputty runs log-only with a notification to the user.

## Requirements

### Requirement 1: Kiro CLI Detection and Graceful Degradation

**User Story:** As a firmware engineer running madputty on a machine that may or may not have kiro-cli installed, I want madputty to detect kiro-cli and log-in state at startup and print a single clear note if AI is unavailable, so that I always get a working serial terminal regardless of my environment.

#### Acceptance Criteria

1. WHEN Madputty starts a Serial_Session, THE Kiro_Detector SHALL search PATH for an executable named `kiro-cli` or `kiro-cli.exe` before any AI feature is activated.
2. WHEN the Kiro_Detector finds Kiro_CLI on PATH, THE Kiro_Detector SHALL record the resolved absolute path for use by the Kiro_Invoker.
3. WHEN the Kiro_Detector finds Kiro_CLI on PATH, THE Kiro_Detector SHALL invoke `kiro-cli whoami --no-interactive` with a 5-second timeout to determine login state.
4. WHEN `kiro-cli whoami --no-interactive` exits with status code 0 within the 5-second timeout, THE Kiro_Detector SHALL set AI_Features_Enabled to true.
5. IF the Kiro_Detector does not find Kiro_CLI on PATH, THEN THE Madputty SHALL print the single-line note `⚠ kiro-cli not found — AI analysis disabled. Install kiro-cli for AI features.` to stderr and set AI_Features_Disabled to true.
6. IF the Kiro_Detector finds Kiro_CLI on PATH but `kiro-cli whoami --no-interactive` exits with a non-zero status or exceeds the 5-second timeout, THEN THE Madputty SHALL print the single-line note ``⚠ kiro-cli found but not logged in. Run `madputty kiro-login` to enable AI.`` to stderr and set AI_Features_Enabled to true with login state false.
7. WHERE the user has supplied `--no-ai` on the command line, THE Kiro_Detector SHALL skip detection and set AI_Features_Disabled to true.
8. THE Kiro_Detector SHALL print at most one AI-related startup note per Serial_Session.

### Requirement 2: Split-Pane Terminal UI

**User Story:** As a firmware engineer using AI-enabled madputty, I want the terminal divided so that the live log stream never stops in the upper region while the AI_Pane shows analysis in the lower region, so that I can read AI insights without losing any device output.

#### Acceptance Criteria

1. WHILE AI_Features_Enabled is true and the terminal height is at least 12 rows, THE Madputty SHALL render the terminal in Split_Pane_Layout with a Log_Region, an AI_Pane, and a Status_Bar.
2. WHILE Split_Pane_Layout is active, THE Madputty SHALL size the AI_Pane at approximately 20% of the terminal height with a minimum of 6 rows.
3. WHILE Split_Pane_Layout is active, THE Madputty SHALL size the Log_Region to fill the terminal height minus the AI_Pane height and the Status_Bar height.
4. WHILE Split_Pane_Layout is active, THE Madputty SHALL render the Status_Bar on the final row of the terminal beneath the AI_Pane.
5. WHILE Split_Pane_Layout is active, THE Log_Region SHALL continue to receive and display new bytes from the serial port even while an AI_Task is running.
6. WHILE Split_Pane_Layout is active, THE AI_Pane SHALL update independently of the Log_Region and a slow AI_Pane render SHALL NOT delay output to the Log_Region.
7. WHEN the terminal emits a resize event, THE Madputty SHALL recompute the Log_Region, AI_Pane, and Status_Bar dimensions and redraw all three regions.
8. IF the terminal height is less than 12 rows when Split_Pane_Layout would otherwise start, THEN THE Madputty SHALL enter Fallback_Mode, print the single-line note `⚠ terminal too small for split pane — AI pane hidden`, and continue log-only rendering.
9. WHILE AI_Features_Disabled is true, THE Madputty SHALL render the terminal using the pre-feature log-only layout with the Status_Bar in its original position.

### Requirement 3: Manual Analyze Hotkey

**User Story:** As a firmware engineer watching a live log, I want to press Ctrl+A A to send the last 50 lines to kiro-cli for analysis without exiting my session, so that I can get an explanation on demand.

#### Acceptance Criteria

1. WHILE AI_Features_Enabled is true and a Serial_Session is active, WHEN the user presses the Manual_Analyze_Hotkey, THE Madputty SHALL snapshot the Rolling_Buffer without blocking the serial port reader.
2. WHEN the Manual_Analyze_Hotkey is pressed, THE Madputty SHALL spawn an AI_Task that passes the Log_Snapshot through the Redactor and invokes Kiro_CLI with the System_Prompt and the redacted snapshot.
3. WHEN the AI_Task is in progress, THE Log_Region SHALL continue to render new serial bytes at full rate.
4. WHEN the AI_Task produces a response, THE Madputty SHALL render the response in the AI_Pane with word-wrap at the AI_Pane width.
5. THE Manual_Analyze_Hotkey SHALL NOT terminate the Serial_Session.
6. WHEN the Manual_Analyze_Hotkey is pressed, THE Input_Forwarder SHALL NOT forward the Ctrl+A byte or the `A` byte to the serial port.
7. WHERE AI_Features_Enabled is true but the login state is false, WHEN the user presses the Manual_Analyze_Hotkey, THE Madputty SHALL render the message ``⚠ Please run `madputty kiro-login` first`` in the AI_Pane and SHALL NOT invoke Kiro_CLI.
8. WHILE AI_Features_Disabled is true, THE Manual_Analyze_Hotkey SHALL be inactive and the Ctrl+A prefix SHALL continue to behave per the pre-feature Exit_Sequence rules.

### Requirement 4: Ask Custom Question Hotkey

**User Story:** As a firmware engineer who wants to ask kiro-cli a specific question about what I am seeing, I want to press Ctrl+A Q and type my question inline, so that I can get targeted help without switching windows.

#### Acceptance Criteria

1. WHILE AI_Features_Enabled is true and a Serial_Session is active, WHEN the user presses the Custom_Question_Hotkey, THE Madputty SHALL display an input prompt inside the AI_Pane with the label `Ask: ` and a visible cursor.
2. WHILE the Custom_Question_Hotkey prompt is active, THE Log_Region SHALL continue to render new serial bytes at full rate.
3. WHILE the Custom_Question_Hotkey prompt is active, THE Input_Forwarder SHALL route keystrokes to the AI_Pane input buffer instead of the serial port.
4. WHEN the user presses Enter while the Custom_Question_Hotkey prompt is active and the input buffer is non-empty, THE Madputty SHALL spawn an AI_Task that invokes Kiro_CLI with the user's question as the primary instruction and the redacted Log_Snapshot attached as context.
5. WHEN the user presses Escape while the Custom_Question_Hotkey prompt is active, THE Madputty SHALL cancel the prompt, clear the input buffer, and return the Input_Forwarder to serial forwarding.
6. WHEN the user presses Enter while the Custom_Question_Hotkey prompt is active and the input buffer is empty, THE Madputty SHALL cancel the prompt without invoking Kiro_CLI.
7. WHEN the Custom_Question_Hotkey is pressed, THE Input_Forwarder SHALL NOT forward the Ctrl+A byte or the `Q` byte to the serial port.
8. WHERE AI_Features_Enabled is true but the login state is false, WHEN the user presses the Custom_Question_Hotkey, THE Madputty SHALL render the message ``⚠ Please run `madputty kiro-login` first`` in the AI_Pane and SHALL NOT open the input prompt.

### Requirement 5: Auto-Watch Mode

**User Story:** As a firmware engineer debugging a flaky device overnight, I want madputty to automatically ask kiro-cli for analysis when an error keyword appears in the stream, so that I find out about problems without staring at the screen.

#### Acceptance Criteria

1. WHERE the user supplies `--ai-watch` and AI_Features_Enabled is true, THE Madputty SHALL enter Auto_Watch_Mode for the duration of the Serial_Session.
2. WHILE Auto_Watch_Mode is active, THE Madputty SHALL inspect each complete log line written to the Log_Region for the Error_Pattern substrings.
3. WHEN an Error_Pattern is detected in a log line and the time since the last automatic trigger is at least 30 seconds, THE Madputty SHALL spawn an AI_Task using the same pipeline as the Manual_Analyze_Hotkey.
4. WHEN an Error_Pattern is detected in a log line and the time since the last automatic trigger is less than 30 seconds, THE Madputty SHALL suppress the automatic trigger and SHALL NOT invoke Kiro_CLI for that line.
5. WHILE Auto_Watch_Mode is active, THE Manual_Analyze_Hotkey SHALL continue to trigger an AI_Task immediately regardless of the Auto_Trigger_Debounce state.
6. WHEN the user presses the Manual_Analyze_Hotkey while Auto_Watch_Mode is active, THE Auto_Trigger_Debounce timer SHALL NOT be reset by the manual trigger.
7. WHERE `--ai-watch` is supplied and AI_Features_Disabled is true, THE Madputty SHALL print the single-line note `⚠ --ai-watch ignored because AI is disabled` to stderr at startup and SHALL NOT perform any Error_Pattern scanning.
8. WHERE `--ai-watch` is not supplied, THE Madputty SHALL NOT perform any automatic AI triggering regardless of log content.

### Requirement 6: Credential Redaction Before AI Call

**User Story:** As a firmware engineer whose logs may contain passwords, tokens, IP addresses, or WiFi SSIDs, I want madputty to redact sensitive values before sending anything to kiro-cli, so that credentials never leave my machine.

#### Acceptance Criteria

1. THE Madputty SHALL pass every Log_Snapshot and every user-supplied question through the Redactor before invoking Kiro_CLI.
2. THE Redactor SHALL replace every match of the pattern `password=\S+` with `password=[REDACTED]`.
3. THE Redactor SHALL replace every match of the pattern `token=\S+` with `token=[REDACTED]`.
4. THE Redactor SHALL replace every match of an IPv4 address of the form `\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}` with `[IP]`.
5. THE Redactor SHALL replace every match of a MAC address of the form `([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}` with `[MAC]`.
6. THE Redactor SHALL replace every match of `SSID=\S+` with `SSID=[SSID]`.
7. THE Redactor SHALL replace every case-insensitive match of `(api[_-]?key|secret[_-]?key|access[_-]?key)\s*[=:]\s*\S+` with the matched key name followed by `=[REDACTED]`.
8. THE Redactor SHALL be idempotent such that applying the Redactor twice to any input produces the same output as applying it once.
9. WHERE the user supplies `--no-redact`, THE Redactor SHALL pass input through unchanged, AND THE Madputty SHALL print the single-line warning `⚠ redaction disabled (--no-redact) — credentials may leak to kiro-cli` to stderr at startup.
10. IF `--no-redact` is not supplied, THEN THE Madputty SHALL NOT transmit any bytes to Kiro_CLI that have not passed through the Redactor.

### Requirement 7: System Prompt for AI

**User Story:** As a firmware engineer, I want kiro-cli to receive a consistent analyst-style system prompt so that responses focus on errors, state transitions, root causes, and WiFi connection details.

#### Acceptance Criteria

1. WHEN Madputty invokes Kiro_CLI for a Manual_Analyze_Hotkey trigger or an Auto_Watch_Mode trigger, THE Madputty SHALL construct the prompt by concatenating the System_Prompt, then the literal string `\n\nLogs:\n`, then the redacted Log_Snapshot.
2. THE System_Prompt SHALL be the exact text: `You are a serial log analyst helping a firmware engineer. Analyze these live serial logs and explain what is happening in plain English. Call out errors, state transitions, and likely root causes. Be concise — 3 to 5 sentences. If you see WiFi connection attempts, identify the security mode, SSID if present, and whether the attempt succeeded or failed.`
3. WHEN Madputty invokes Kiro_CLI for a Custom_Question_Hotkey trigger, THE Madputty SHALL construct the prompt by concatenating the user's question text, then the literal string `\n\nLogs:\n`, then the redacted Log_Snapshot.
4. THE Madputty SHALL invoke Kiro_CLI with the argument list `chat --no-interactive <prompt>` where `<prompt>` is the fully assembled prompt string passed as a single argument.

### Requirement 8: AI Pane Rendering

**User Story:** As a firmware engineer, I want the AI_Pane to show a header with the last update time, a spinner while a new analysis is running, and error messages clearly distinguished from successful responses, so that I always know the state of the AI at a glance.

#### Acceptance Criteria

1. WHILE Split_Pane_Layout is active, THE AI_Pane SHALL render a header line in bold Amazon-yellow containing the text `🤖 AI Analysis (updated HH:MM:SS)` where HH:MM:SS is the local time of the most recent successful response.
2. WHILE Split_Pane_Layout is active, THE AI_Pane SHALL render a separator line directly beneath the header using Unicode box-drawing characters in Amazon-yellow.
3. WHILE an AI_Task is in progress, THE AI_Pane header SHALL display the animated spinner `⠋ Analyzing...` and SHALL continue to show the previously rendered response body beneath the separator.
4. WHEN a successful AI response is received, THE AI_Pane SHALL render the response body as white text word-wrapped to the AI_Pane width.
5. WHEN Kiro_CLI returns a non-zero exit status or produces output that cannot be decoded as UTF-8, THE AI_Pane SHALL render the line `⚠ AI error: <message>` in red, where `<message>` is the first line of Kiro_CLI stderr or a generic error if stderr is empty.
6. THE AI_Pane SHALL retain the most recent successful response body until it is replaced by a newer successful response.
7. IF the rendered response body is taller than the AI_Pane content area, THEN THE AI_Pane SHALL truncate the visible text and append the line `... (press Ctrl+A L for full)` as the final visible row.
8. THE AI_Pane content SHALL NOT auto-scroll and SHALL NOT change focus away from the Log_Region.

### Requirement 9: Last Response Hotkey

**User Story:** As a firmware engineer who received a truncated AI response, I want to press Ctrl+A L to view the full response in a scrollable overlay so that I can read it end-to-end without leaving the session.

#### Acceptance Criteria

1. WHILE AI_Features_Enabled is true and at least one AI response has been received in the current Serial_Session, WHEN the user presses the Last_Response_Hotkey, THE Madputty SHALL overlay a scrollable modal on top of the AI_Pane that displays the full text of the most recent successful response.
2. WHILE the Last_Response_Hotkey modal is open, THE Log_Region SHALL continue to render new serial bytes at full rate.
3. WHILE the Last_Response_Hotkey modal is open, THE Madputty SHALL accept arrow-key and Page Up / Page Down input to scroll the modal content.
4. WHEN the user presses any other key while the modal is open, THE Madputty SHALL close the modal and restore the AI_Pane to its previous rendering.
5. WHEN the Last_Response_Hotkey is pressed, THE Input_Forwarder SHALL NOT forward the Ctrl+A byte or the `L` byte to the serial port.
6. WHERE no AI response has been received in the current Serial_Session, WHEN the user presses the Last_Response_Hotkey, THE Madputty SHALL render the line `⚠ no AI responses yet` in the AI_Pane for 2 seconds.

### Requirement 10: Kiro CLI Auth Helper Subcommands

**User Story:** As a firmware engineer who does not want to remember the kiro-cli command surface, I want madputty to expose `kiro-login` and `kiro-status` subcommands that delegate to kiro-cli, so that I can manage AI auth from the same binary.

#### Acceptance Criteria

1. WHEN the user invokes `madputty kiro-login`, THE Madputty SHALL spawn Kiro_CLI with the argument `login` and inherit stdin, stdout, and stderr from the parent process.
2. WHEN the `kiro-cli login` child process exits, THE Madputty SHALL exit with the same exit code as the child process.
3. WHEN the user invokes `madputty kiro-status`, THE Madputty SHALL spawn Kiro_CLI with the arguments `whoami --no-interactive` and print the child's stdout and stderr to the corresponding Madputty streams.
4. WHEN the `kiro-cli whoami --no-interactive` child process exits with status 0, THE Madputty SHALL exit with status 0.
5. WHEN the `kiro-cli whoami --no-interactive` child process exits with a non-zero status, THE Madputty SHALL exit with exit code 1.
6. IF Kiro_CLI is not found on PATH when the user invokes `madputty kiro-login` or `madputty kiro-status`, THEN THE Madputty SHALL print the single-line error `kiro-cli not found on PATH` to stderr and exit with exit code 1.
7. THE Kiro_Login_Subcommand and Kiro_Status_Subcommand SHALL NOT require any positional COM port argument.

### Requirement 11: Timeout for Kiro CLI Calls

**User Story:** As a firmware engineer, I want every kiro-cli call to have a bounded timeout so that a hung AI call never freezes the UI, and I want to override the default for slow networks.

#### Acceptance Criteria

1. WHEN Madputty invokes Kiro_CLI for a Manual_Analyze_Hotkey, Custom_Question_Hotkey, or Auto_Watch_Mode trigger, THE Kiro_Invoker SHALL enforce an AI_Timeout of 30 seconds by default.
2. WHERE the user supplies `--ai-timeout-seconds <N>` with a positive integer `N`, THE Kiro_Invoker SHALL use `N` seconds as the AI_Timeout for every subsequent Kiro_CLI invocation in the Serial_Session.
3. IF the user supplies `--ai-timeout-seconds <N>` with a value that is not a positive integer, THEN THE Madputty SHALL print the single-line error `invalid --ai-timeout-seconds value: <N>` to stderr and exit with exit code 1.
4. IF the AI_Timeout expires before Kiro_CLI produces a response, THEN THE Kiro_Invoker SHALL kill the child process, SHALL render `⚠ AI timed out after <N>s` in the AI_Pane in red, and SHALL NOT retry the invocation automatically.
5. WHILE the AI_Timeout is being enforced, THE Log_Region SHALL continue to render new serial bytes at full rate.
6. WHEN the AI_Timeout expires and the child process is killed, THE Madputty SHALL reap the child to avoid leaving a zombie.

### Requirement 12: AI Responses Saved to Disk

**User Story:** As a firmware engineer reviewing a session after the fact, I want every AI response saved to a per-session Markdown file so that I can grep or share insights later.

#### Acceptance Criteria

1. WHEN a Serial_Session with AI_Features_Enabled starts, THE Madputty SHALL compute a Session_Id of the form `YYYYMMDD-HHMMSS-<port>` using the local start time and the sanitized port name.
2. WHEN a Serial_Session with AI_Features_Enabled starts, THE Madputty SHALL create the directory `~/.madputty/ai-responses/` if it does not already exist.
3. WHEN Kiro_CLI returns a successful response to a Manual_Analyze_Hotkey, Custom_Question_Hotkey, or Auto_Watch_Mode trigger, THE Madputty SHALL append to the AI_Response_Log a Markdown section containing a level-2 header with the local timestamp, a `Trigger:` line identifying which trigger produced the response, and the response body.
4. WHEN Kiro_CLI returns a successful response to a Custom_Question_Hotkey trigger, THE Madputty SHALL also append a `Question:` line with the user's question in the Markdown section above the response body.
5. WHEN a Serial_Session ends and at least one AI response was appended to the AI_Response_Log, THE Madputty SHALL print the single line `AI responses saved to <absolute_path>` to stdout before exit.
6. WHEN a Serial_Session ends and no AI response was appended, THE Madputty SHALL NOT print the save-notification line and SHALL NOT leave an empty AI_Response_Log file on disk.
7. IF the AI_Response_Log cannot be created or written due to a filesystem error, THEN THE Madputty SHALL print the single-line warning `⚠ AI response log write failed: <reason>` to stderr and SHALL continue the Serial_Session without disabling AI features.

### Requirement 13: New CLI Flags

**User Story:** As a firmware engineer who runs madputty from scripts and interactive shells, I want every AI behavior controllable by a command-line flag so that I can pin behavior for reproducible runs and CI.

#### Acceptance Criteria

1. THE Madputty SHALL accept the flag `--ai-watch` with no value, and WHERE it is supplied, THE Madputty SHALL enable Auto_Watch_Mode per Requirement 5.
2. THE Madputty SHALL accept the flag `--ai-timeout-seconds <N>`, and WHERE it is supplied with a positive integer, THE Madputty SHALL use `N` as the AI_Timeout per Requirement 11.
3. THE Madputty SHALL accept the flag `--no-redact` with no value, and WHERE it is supplied, THE Madputty SHALL disable redaction per Requirement 6 criterion 9.
4. THE Madputty SHALL accept the flag `--no-ai` with no value, and WHERE it is supplied, THE Madputty SHALL set AI_Features_Disabled to true per Requirement 1 criterion 7.
5. THE Madputty SHALL treat the new flags as optional and SHALL preserve the behavior of all pre-feature flags such as `--baud`, `--log`, `--send`, `--echo`, `--plain`, and `--verbose`.
6. IF the user supplies an AI-related flag together with `--no-ai`, THEN THE Madputty SHALL print the single-line warning `⚠ --no-ai overrides other AI flags` to stderr and proceed with AI disabled.

### Requirement 14: New Subcommands

**User Story:** As a firmware engineer, I want madputty to expose `kiro-login` and `kiro-status` as first-class subcommands so that I can manage kiro-cli auth from the same binary.

#### Acceptance Criteria

1. THE Madputty SHALL accept the subcommand `kiro-login` that behaves per Requirement 10 criteria 1 and 2.
2. THE Madputty SHALL accept the subcommand `kiro-status` that behaves per Requirement 10 criteria 3 through 5.
3. THE Madputty SHALL preserve the existing `list` subcommand and the existing `--list` flag behavior unchanged by this feature.
4. IF the user invokes a subcommand that is not one of `list`, `kiro-login`, `kiro-status`, THEN THE Madputty SHALL print a clap-style error identifying the unknown subcommand and exit with exit code 1.

### Requirement 15: Non-AI Users Get the Full Existing Experience

**User Story:** As a firmware engineer on a machine without kiro-cli or running CI, I want madputty to behave exactly like the current version when AI is unavailable or disabled, so that existing scripts and workflows are not disrupted.

#### Acceptance Criteria

1. WHILE AI_Features_Disabled is true, THE Madputty SHALL NOT render the Split_Pane_Layout and SHALL use the pre-feature full-height log layout.
2. WHILE AI_Features_Disabled is true, THE Manual_Analyze_Hotkey, Custom_Question_Hotkey, and Last_Response_Hotkey SHALL NOT be active, and the Ctrl+A prefix SHALL retain the pre-feature Exit_Sequence behavior.
3. WHILE AI_Features_Disabled is true, THE Madputty SHALL NOT scan log lines for Error_Pattern substrings.
4. WHILE AI_Features_Disabled is true, THE Madputty SHALL NOT create the directory `~/.madputty/ai-responses/` and SHALL NOT create any AI_Response_Log.
5. WHILE AI_Features_Disabled is true, THE Banner SHALL NOT contain any AI-related messages and THE Madputty SHALL NOT render any AI-related startup note beyond the one defined in Requirement 1.
6. WHILE AI_Features_Enabled is true and the login state is false, THE Madputty SHALL render the Split_Pane_Layout so that the AI_Pane is visible but empty, AND when the user presses any AI hotkey, THE Madputty SHALL render the message ``⚠ Please run `madputty kiro-login` first`` in the AI_Pane.

### Requirement 16: Log Pump Non-Blocking Guarantee

**User Story:** As a firmware engineer capturing a once-in-a-week bug, I want the serial byte pump to keep flowing even when AI code is slow, hung, or misbehaving, so that I never lose a single byte of device output because of an AI feature.

#### Acceptance Criteria

1. THE Port_Reader task SHALL NOT await any future owned by the AI_Subsystem.
2. THE Port_Writer task SHALL NOT await any future owned by the AI_Subsystem.
3. WHEN the Port_Reader produces a log line, THE Madputty SHALL deliver the line to the Rolling_Buffer and to the AI_Subsystem's Error_Pattern scanner using a bounded tokio channel with `try_send` semantics.
4. IF the AI_Subsystem's bounded channel is full when the Port_Reader tries to deliver a new line, THEN THE Port_Reader SHALL drop the line from the AI delivery path and SHALL continue writing the line to the Log_Region and the Log_File without delay.
5. WHILE an AI_Task is in progress, THE Rolling_Buffer SHALL continue to accept new log lines and SHALL NOT block the Port_Reader.
6. IF the Kiro_CLI child process becomes stuck, THEN the AI_Timeout defined in Requirement 11 SHALL fire and the Port_Reader SHALL remain unaffected for the entire duration of the stuck process.
7. THE AI_Pane rendering task SHALL run as a separate tokio task from the Log_Region rendering task, AND a slow AI_Pane render SHALL NOT delay Log_Region writes.

### Requirement 17: Rolling Buffer of Recent Log Lines

**User Story:** As the AI_Subsystem, I want access to the most recent 50 lines of log output at any moment so that I can send accurate context to kiro-cli without re-reading the serial stream.

#### Acceptance Criteria

1. THE Madputty SHALL maintain a Rolling_Buffer that holds the most recent 50 complete log lines decoded from the serial stream.
2. WHEN the Port_Reader emits a complete log line, THE Madputty SHALL append the line to the Rolling_Buffer and SHALL evict the oldest line if the buffer size exceeds 50.
3. WHEN an AI_Task requires context, THE Madputty SHALL take a Log_Snapshot of the Rolling_Buffer in a non-blocking operation that copies the current contents into an owned `Vec<String>`.
4. WHILE a Log_Snapshot is in use by an AI_Task, THE Rolling_Buffer SHALL continue to accept new lines from the Port_Reader without waiting for the AI_Task to finish.
5. THE Rolling_Buffer SHALL store lines decoded as UTF-8 using lossy decoding so that binary device output does not panic the buffer.

### Requirement 18: Platform and Dependency Constraints

**User Story:** As the maintainer of madputty, I want the feature to reuse existing dependencies where possible, add only one new crate for redaction, and continue to compile on Linux and macOS even if Windows is the primary target, so that CI remains healthy and the binary size stays small.

#### Acceptance Criteria

1. THE Madputty crate SHALL declare a new runtime dependency on `regex` version 1 in `Cargo.toml`.
2. THE Madputty crate SHALL NOT declare any additional runtime dependencies beyond `regex` for this feature.
3. THE Madputty crate SHALL use the existing `tokio` dependency and its `process` feature to spawn Kiro_CLI via `tokio::process::Command`.
4. THE Madputty crate SHALL use the existing `tokio` dependency and its `time` feature to enforce the AI_Timeout via `tokio::time::timeout`.
5. THE Madputty crate SHALL use the existing `crossterm` dependency for terminal size detection, cursor positioning, and key event reading required by the Split_Pane_Layout and AI hotkeys.
6. THE Madputty crate SHALL compile on Linux and macOS without any `cfg(target_os = "windows")`-only code blocking the build, even though the Split_Pane_Layout and Kiro_CLI invocation are exercised only on Windows.
