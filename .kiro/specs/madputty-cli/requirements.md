# Requirements Document

## Introduction

MadPutty is a Rust CLI tool for Windows that manages SSH and serial connection profiles locally, authenticates against a remote API mid-flow to fetch credentials and session tokens, and launches PuTTY or TeraTerm terminal sessions with the fetched credentials. The defining architectural pattern is mid-flow authentication: commands begin execution without authentication and only trigger the auth flow when remote resources are actually needed.

## Glossary

- **MadPutty_CLI**: The command-line application binary (`madputty.exe`) that users invoke to manage profiles, authenticate, and launch terminal sessions.
- **Profile_Store**: The local file system directory (`~/.madputty/profiles/`) containing individual TOML files, each representing a saved connection profile.
- **Connection_Profile**: A TOML file describing a single SSH, Serial, or Telnet connection target, including host, port, protocol, credentials reference, terminal preference, and metadata.
- **Config_Manager**: The subsystem responsible for reading, writing, and merging configuration from `~/.madputty/config.toml` and environment variable overrides.
- **Auth_Gate**: The `ensure_authenticated()` function that checks for a valid cached token, attempts refresh, or prompts the user interactively before allowing access to remote resources.
- **Token_Cache**: The file `~/.madputty/credentials.json` that stores the current authentication token, refresh token, and expiry timestamp.
- **Vault_Client**: The HTTP client subsystem that communicates with the remote secrets API (`GET /api/v1/secrets/{path}`) to fetch credentials after authentication.
- **Credential_Cache**: An in-memory or short-lived on-disk cache of fetched vault credentials with a configurable TTL, used to avoid repeated API calls during batch operations.
- **Terminal_Launcher**: A trait-based abstraction over terminal emulator backends (PuTTY, TeraTerm) responsible for detecting executables, building command-line arguments, and spawning processes.
- **PuTTY_Backend**: The Terminal_Launcher implementation for `putty.exe` (GUI) and `plink.exe` (headless) sessions.
- **TeraTerm_Backend**: The Terminal_Launcher implementation for `ttermpro.exe`, including `.ttl` macro file generation for automated login.
- **Session_Handle**: A struct holding the spawned terminal process PID and associated metadata, returned after a successful launch.
- **Batch_Runner**: The subsystem that resolves a group name to a set of Connection_Profiles and launches multiple terminal sessions after a single authentication.
- **TTL**: Time-To-Live; the duration for which a cached credential or token remains valid before requiring re-fetch or re-authentication.
- **PAT**: Personal Access Token; a long-lived API token that a user can paste interactively or set via environment variable.
- **OAuth_Device_Flow**: An OAuth 2.0 authentication flow where the CLI displays a user code and verification URL, then polls the authorization server until the user completes login in a browser.
- **Dry_Run_Mode**: A CLI flag (`--dry-run`) that causes MadPutty_CLI to display the terminal command that would be executed without actually spawning a process.
- **Headless_Mode**: Operation without interactive prompts, enabled by `--no-interactive` flag or the presence of `MADPUTTY_AUTH_TOKEN` environment variable.


## Requirements

### Requirement 1: Local Profile Management

**User Story:** As an operations engineer, I want to create, list, edit, and delete SSH/serial connection profiles locally without any authentication, so that I can manage my connection inventory offline and independently of the remote vault.

#### Acceptance Criteria

1. WHEN the user runs `madputty profiles add`, THE MadPutty_CLI SHALL interactively prompt for profile fields (name, host, port, protocol, username, auth type, vault secret path, SSH key path, preferred terminal, serial config, startup commands, group, tags) and write a TOML file to the Profile_Store at `~/.madputty/profiles/{name}.toml`.
2. WHEN the user runs `madputty profiles list`, THE MadPutty_CLI SHALL read all TOML files from the Profile_Store and display a tabular summary of each Connection_Profile including name, host, port, protocol, and group.
3. WHEN the user runs `madputty profiles remove <name>`, THE MadPutty_CLI SHALL delete the corresponding TOML file from the Profile_Store and confirm deletion to the user.
4. WHEN the user runs `madputty profiles add` with a name that already exists in the Profile_Store, THE MadPutty_CLI SHALL prompt the user for confirmation before overwriting the existing Connection_Profile.
5. IF a Connection_Profile TOML file contains invalid or missing required fields, THEN THE MadPutty_CLI SHALL report a descriptive parse error identifying the file and the invalid field.
6. THE Profile_Store SHALL store each Connection_Profile as an individual TOML file containing all fields defined in the Connection_Profile glossary entry.
7. WHEN the user runs any `profiles` subcommand, THE MadPutty_CLI SHALL complete the operation without invoking the Auth_Gate.

### Requirement 2: Directory Initialization

**User Story:** As a new user, I want to initialize the MadPutty configuration directory, so that all required directories and default config files are created before first use.

#### Acceptance Criteria

1. WHEN the user runs `madputty init`, THE MadPutty_CLI SHALL create the directory structure `~/.madputty/`, `~/.madputty/profiles/`, and a default `~/.madputty/config.toml` if they do not already exist.
2. WHEN the directories and config file already exist, THE MadPutty_CLI SHALL report that initialization is already complete without overwriting existing files.
3. WHEN the user runs `madputty init`, THE MadPutty_CLI SHALL complete the operation without invoking the Auth_Gate.

### Requirement 3: Mid-Flow Authentication

**User Story:** As an operations engineer, I want commands that need remote resources to authenticate only at the point where credentials are required, so that local-only operations remain fast and offline-capable while remote operations seamlessly trigger authentication.

#### Acceptance Criteria

1. WHEN a command reaches a point requiring remote resources, THE Auth_Gate SHALL check the Token_Cache at `~/.madputty/credentials.json` for a valid, non-expired token before prompting the user.
2. WHILE a valid token exists in the Token_Cache and the token expiry timestamp is in the future, THE Auth_Gate SHALL return the cached token without prompting the user.
3. WHEN the cached token has expired, THE Auth_Gate SHALL attempt to refresh the token using the stored refresh token.
4. IF the token refresh fails or no cached token exists, THEN THE Auth_Gate SHALL prompt the user interactively for authentication using the configured auth method (PAT prompt or OAuth_Device_Flow).
5. WHEN authentication succeeds, THE Auth_Gate SHALL write the new token, refresh token, and expiry timestamp to the Token_Cache file.
6. WHEN the environment variable `MADPUTTY_AUTH_TOKEN` is set, THE Auth_Gate SHALL use the environment variable value as the token and skip all interactive prompts and Token_Cache checks.
7. WHILE the `--no-interactive` flag is active and no valid token is available (from cache or environment variable), THE Auth_Gate SHALL exit with an authentication error (exit code 2) instead of prompting.
8. WHEN any API call returns HTTP 401, THE Vault_Client SHALL clear the cached token and invoke the Auth_Gate exactly once more; IF the second attempt also fails, THEN THE MadPutty_CLI SHALL exit with an authentication error (exit code 2).

### Requirement 4: OAuth Device Code Flow

**User Story:** As an operations engineer, I want to authenticate using the OAuth device code flow, so that I can log in securely through my browser without pasting tokens into the terminal.

#### Acceptance Criteria

1. WHEN the configured auth method is `oauth` and interactive authentication is triggered, THE Auth_Gate SHALL POST a device code request to the configured `oauth_device_url` and receive a user code and verification URL.
2. WHEN a device code is received, THE MadPutty_CLI SHALL display the user code and verification URL to the user and show a polling spinner using indicatif.
3. WHILE the device code is pending user authorization, THE Auth_Gate SHALL poll the configured `oauth_token_url` at the interval specified by the authorization server.
4. WHEN the authorization server returns an access token, THE Auth_Gate SHALL store the token in the Token_Cache and resume the calling command.
5. IF the device code expires before the user completes authorization, THEN THE Auth_Gate SHALL display an expiry message and exit with an authentication error (exit code 2).

### Requirement 5: Explicit Authentication Commands

**User Story:** As a user, I want explicit login, logout, and status commands, so that I can manage my authentication state independently of connection commands.

#### Acceptance Criteria

1. WHEN the user runs `madputty auth login`, THE MadPutty_CLI SHALL invoke the Auth_Gate to authenticate and store the resulting token in the Token_Cache.
2. WHEN the user runs `madputty auth logout`, THE MadPutty_CLI SHALL delete the Token_Cache file at `~/.madputty/credentials.json`.
3. WHEN the user runs `madputty auth status`, THE MadPutty_CLI SHALL display the current authentication state including whether a token is cached, the token expiry timestamp, and the auth method used.
4. WHEN no token is cached and the user runs `madputty auth status`, THE MadPutty_CLI SHALL display "Not authenticated" without invoking the Auth_Gate.

### Requirement 6: Vault Credential Fetching

**User Story:** As an operations engineer, I want to fetch credentials from a remote vault API after authentication, so that I can retrieve secrets needed to connect to target hosts.

#### Acceptance Criteria

1. WHEN the user runs `madputty vault fetch <path>`, THE MadPutty_CLI SHALL invoke the Auth_Gate mid-flow, then send `GET /api/v1/secrets/{path}` to the configured API base URL with the authenticated token, and print the retrieved credential fields (username, masked password, expiry) to stdout.
2. WHEN the Vault_Client fetches credentials, THE Credential_Cache SHALL store the fetched credentials with a configurable TTL to avoid repeated API calls for the same secret path within the TTL window.
3. WHILE a cached credential for a given secret path exists in the Credential_Cache and the TTL has not expired, THE Vault_Client SHALL return the cached credential without making an API call.
4. IF the Vault_Client receives an API error other than 401, THEN THE MadPutty_CLI SHALL display the HTTP status code and error message with an actionable suggestion and exit with exit code 1.


### Requirement 7: PuTTY Terminal Integration

**User Story:** As an operations engineer, I want MadPutty to detect and launch PuTTY or plink with the correct arguments, so that I can open GUI or headless SSH sessions using fetched credentials.

#### Acceptance Criteria

1. WHEN the selected terminal is PuTTY and the protocol is SSH, THE PuTTY_Backend SHALL detect `putty.exe` at the configured path or via PATH lookup and launch it with `-ssh`, `-P <port>`, and the appropriate credential flags (`-pw` for password, `-i` for key file).
2. WHEN the selected terminal is PuTTY and the Connection_Profile specifies a `putty_session_name`, THE PuTTY_Backend SHALL pass `-load <session_name>` to `putty.exe`.
3. WHEN the `--dry-run` flag is active, THE PuTTY_Backend SHALL print the full command line that would be executed (with credentials masked) without spawning a process.
4. WHEN `putty.exe` is launched, THE PuTTY_Backend SHALL spawn the process in detached mode and return a Session_Handle containing the child process PID.
5. WHEN headless mode is required, THE PuTTY_Backend SHALL launch `plink.exe` instead of `putty.exe` with the same credential and connection arguments.
6. IF `putty.exe` or `plink.exe` cannot be found at the configured path or on PATH, THEN THE PuTTY_Backend SHALL exit with exit code 3 and display an error message suggesting how to configure the terminal path.

### Requirement 8: TeraTerm Terminal Integration

**User Story:** As a network administrator, I want MadPutty to detect and launch TeraTerm with the correct arguments and macro files, so that I can open SSH and serial sessions with automated login.

#### Acceptance Criteria

1. WHEN the selected terminal is TeraTerm and the protocol is SSH, THE TeraTerm_Backend SHALL detect `ttermpro.exe` at the configured path or via PATH lookup and launch it with `/ssh /auth=password /user=<username> /passwd=<password>` arguments.
2. WHEN the selected terminal is TeraTerm and the protocol is Serial, THE TeraTerm_Backend SHALL launch `ttermpro.exe` with `/C=<com_port> /BAUD=<baud_rate>` arguments derived from the Connection_Profile serial config.
3. WHEN the Connection_Profile contains startup commands, THE TeraTerm_Backend SHALL generate a temporary `.ttl` macro file containing the login sequence and startup commands, and pass the macro file path via the `/M=` flag.
4. WHEN `ttermpro.exe` has been launched, THE TeraTerm_Backend SHALL delete the temporary `.ttl` macro file to prevent credential leakage.
5. WHEN `ttermpro.exe` is launched, THE TeraTerm_Backend SHALL spawn the process in detached mode and return a Session_Handle containing the child process PID.
6. IF `ttermpro.exe` cannot be found at the configured path or on PATH, THEN THE TeraTerm_Backend SHALL exit with exit code 3 and display an error message suggesting how to configure the terminal path.

### Requirement 9: Connect Command (Mid-Flow Auth Orchestration)

**User Story:** As an operations engineer, I want to run `madputty connect <profile>` and have it load my profile locally, authenticate mid-flow, fetch credentials from the vault, and launch the terminal session in one seamless command.

#### Acceptance Criteria

1. WHEN the user runs `madputty connect <profile>`, THE MadPutty_CLI SHALL load the named Connection_Profile from the Profile_Store without invoking the Auth_Gate.
2. WHEN the Connection_Profile is loaded, THE MadPutty_CLI SHALL detect the appropriate Terminal_Launcher (based on the profile's preferred terminal, the `--terminal` flag override, or the default terminal from config) without invoking the Auth_Gate.
3. WHEN the terminal is detected and the Connection_Profile auth type is `VaultSecret`, THE MadPutty_CLI SHALL invoke the Auth_Gate mid-flow, then fetch credentials from the Vault_Client using the profile's vault secret path.
4. WHEN credentials are fetched, THE MadPutty_CLI SHALL pass the Connection_Profile and credentials to the Terminal_Launcher to spawn the terminal session.
5. WHEN the `--terminal` flag is provided, THE MadPutty_CLI SHALL use the specified terminal type instead of the profile's preferred terminal or the default terminal.
6. IF the named profile does not exist in the Profile_Store, THEN THE MadPutty_CLI SHALL exit with exit code 4 and display an error message listing available profiles.

### Requirement 10: Batch Connections

**User Story:** As a network administrator, I want to run `madputty batch <group>` and have it authenticate once and then launch terminal sessions for all profiles in the named group, so that I can connect to multiple hosts efficiently.

#### Acceptance Criteria

1. WHEN the user runs `madputty batch <group>`, THE Batch_Runner SHALL query the Profile_Store for all Connection_Profiles with a matching group field.
2. WHEN matching profiles are found, THE Batch_Runner SHALL invoke the Auth_Gate exactly once before launching any terminal sessions.
3. WHEN authentication succeeds, THE Batch_Runner SHALL iterate over each matching Connection_Profile, fetch credentials from the Vault_Client (using the Credential_Cache to avoid redundant API calls), and launch a terminal session for each profile.
4. WHILE launching batch sessions, THE MadPutty_CLI SHALL display progress for each connection using indicatif, showing the profile name and launch status (success or failure).
5. IF no profiles match the specified group, THEN THE Batch_Runner SHALL exit with exit code 4 and display an error message listing available groups.
6. IF an individual session launch fails within a batch, THEN THE Batch_Runner SHALL log the error for that profile and continue launching the remaining sessions.

### Requirement 11: Configuration Management

**User Story:** As a user, I want to configure default settings like terminal preference, API URL, and executable paths via a config file and environment variables, so that I can customize MadPutty for my environment.

#### Acceptance Criteria

1. THE Config_Manager SHALL read configuration from `~/.madputty/config.toml` on startup, supporting sections for `general` (default_terminal, verbose), `terminals.putty` (path, plink_path), `terminals.teraterm` (path), `api` (base_url, timeout_seconds), and `auth` (method, oauth_client_id, oauth_device_url, oauth_token_url).
2. WHEN the environment variable `MADPUTTY_API_URL` is set, THE Config_Manager SHALL use the environment variable value for the API base URL, overriding the config file value.
3. WHEN the environment variable `MADPUTTY_TERMINAL` is set, THE Config_Manager SHALL use the environment variable value for the default terminal, overriding the config file value.
4. WHEN the environment variable `MADPUTTY_PUTTY_PATH` is set, THE Config_Manager SHALL use the environment variable value for the PuTTY executable path, overriding the config file value.
5. WHEN the environment variable `MADPUTTY_TERATERM_PATH` is set, THE Config_Manager SHALL use the environment variable value for the TeraTerm executable path, overriding the config file value.
6. WHEN the user runs `madputty config set <key> <value>`, THE Config_Manager SHALL update the specified key in `~/.madputty/config.toml`.
7. WHEN the user runs `madputty config show`, THE Config_Manager SHALL display the current effective configuration including values from the config file and any active environment variable overrides.
8. IF the config file does not exist or contains invalid TOML, THEN THE Config_Manager SHALL report a descriptive error and suggest running `madputty init`.

### Requirement 12: Security

**User Story:** As a security-conscious user, I want MadPutty to protect credentials at rest and in transit, so that passwords and tokens are never exposed in logs, output, or leftover files.

#### Acceptance Criteria

1. THE MadPutty_CLI SHALL mask password and token values in all terminal output, including when the `--verbose` flag is active, replacing sensitive values with `****`.
2. WHEN the Auth_Gate writes the Token_Cache file, THE MadPutty_CLI SHALL set file permissions on `~/.madputty/credentials.json` to owner-read-write only (0600 on Unix, restricted ACL on Windows).
3. WHEN the TeraTerm_Backend generates a temporary `.ttl` macro file containing credentials, THE TeraTerm_Backend SHALL delete the file after the terminal process has been spawned.
4. IF a temporary `.ttl` macro file deletion fails, THEN THE MadPutty_CLI SHALL log a warning to the user indicating the file path that should be manually deleted.
5. THE MadPutty_CLI SHALL transmit authentication tokens to the remote API only over HTTPS.

### Requirement 13: User Experience and CLI Flags

**User Story:** As a user, I want clear, colored output with progress indicators and helpful error messages, so that I can understand what MadPutty is doing and quickly resolve issues.

#### Acceptance Criteria

1. THE MadPutty_CLI SHALL use the `console` crate to produce colored terminal output for status messages, errors, and warnings.
2. WHILE the Auth_Gate is polling during OAuth_Device_Flow or the Vault_Client is making API calls, THE MadPutty_CLI SHALL display a spinner using the `indicatif` crate.
3. WHEN an error occurs, THE MadPutty_CLI SHALL display a human-readable error message with an actionable suggestion for resolution.
4. WHEN the `--verbose` flag is active, THE MadPutty_CLI SHALL enable debug-level tracing output via the `tracing` and `tracing-subscriber` crates, excluding sensitive credential values.
5. WHEN the `--dry-run` flag is active, THE MadPutty_CLI SHALL display the terminal command and arguments that would be executed (with credentials masked) and exit with code 0 without spawning a process.
6. THE MadPutty_CLI SHALL use the following exit codes: 0 for success, 1 for general error, 2 for authentication error, 3 for terminal not found, 4 for profile not found.
7. THE MadPutty_CLI SHALL parse commands and flags using `clap` v4 with derive macros, supporting the command structure: `init`, `profiles {list|add|remove}`, `connect <profile>`, `batch <group>`, `vault fetch <path>`, `auth {login|logout|status}`, `config {set|show}`.

### Requirement 14: Cross-Platform Architecture

**User Story:** As a developer, I want the codebase structured with trait-based terminal backends, so that Linux and macOS terminal emulators can be added in the future without restructuring the application.

#### Acceptance Criteria

1. THE Terminal_Launcher SHALL be defined as a Rust trait with methods `detect() -> Result<PathBuf>`, `build_args(profile, creds) -> Vec<String>`, and `launch(profile, creds) -> Result<Session_Handle>`.
2. THE MadPutty_CLI SHALL use `std::process::Command` to spawn terminal processes in detached mode, allowing the terminal to outlive the MadPutty_CLI process.
3. THE MadPutty_CLI SHALL compile and run on Windows as the primary target platform.
4. THE MadPutty_CLI SHALL isolate all platform-specific terminal detection and launching logic within the Terminal_Launcher trait implementations (PuTTY_Backend, TeraTerm_Backend).

### Requirement 15: TOML Profile Serialization Round-Trip

**User Story:** As a developer, I want to ensure that connection profiles can be serialized to TOML and deserialized back without data loss, so that profile storage is reliable.

#### Acceptance Criteria

1. THE Profile_Store SHALL serialize Connection_Profile structs to TOML format using the `toml` crate and the `serde` Serialize trait.
2. THE Profile_Store SHALL deserialize TOML files back into Connection_Profile structs using the `toml` crate and the `serde` Deserialize trait.
3. FOR ALL valid Connection_Profile values, serializing to TOML then deserializing back SHALL produce an equivalent Connection_Profile (round-trip property).

### Requirement 16: Token Cache Serialization Round-Trip

**User Story:** As a developer, I want to ensure that authentication tokens can be serialized to JSON and deserialized back without data loss, so that token caching is reliable.

#### Acceptance Criteria

1. THE Token_Cache SHALL serialize token data (access token, refresh token, expiry timestamp) to JSON format using the `serde_json` crate.
2. THE Token_Cache SHALL deserialize JSON files back into token data structs using the `serde_json` crate.
3. FOR ALL valid token data values, serializing to JSON then deserializing back SHALL produce equivalent token data (round-trip property).
