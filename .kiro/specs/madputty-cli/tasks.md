# Implementation Plan: MadPutty CLI

## Overview

Implement a Rust CLI tool for managing SSH/serial terminal sessions with mid-flow authentication. The implementation follows a bottom-up approach: scaffolding → data models → config → auth → CLI parsing → terminal backends → API client → profile management → commands → main entry point.

## Tasks

- [x] 1. Project scaffolding
  - [x] 1.1 Create Cargo.toml with all dependencies (clap, tokio, serde, toml, serde_json, reqwest, chrono, thiserror, console, indicatif, tracing, tracing-subscriber, dirs)
    - _Requirements: 13.7, 14.3_
  - [x] 1.2 Create directory structure under src/ (auth/, api/, terminal/, profiles/, commands/, config/)
    - _Requirements: 14.4_
  - [x] 1.3 Create README.md with project overview and usage
    - _Requirements: 13.3_

- [x] 2. Core data models and error types
  - [x] 2.1 Implement errors.rs with MadPuttyError enum using thiserror, mapping variants to exit codes (0 success, 1 general, 2 auth, 3 terminal not found, 4 profile not found)
    - _Requirements: 13.6_
  - [x] 2.2 Implement profiles/models.rs with ConnectionProfile, Protocol, ProfileAuthType, TerminalType, SerialConfig structs and enums with serde derives
    - _Requirements: 1.6, 15.1, 15.2_
  - [x] 2.3 Implement auth/token.rs with AuthToken, AuthMethod structs, is_expired(), load/save/validate functions for ~/.madputty/credentials.json
    - _Requirements: 3.1, 3.5, 16.1, 16.2_
  - [ ]* 2.4 Write property test for ConnectionProfile TOML round-trip serialization
    - **Property 1: Round-trip consistency for ConnectionProfile**
    - **Validates: Requirements 15.3**
  - [ ]* 2.5 Write property test for AuthToken JSON round-trip serialization
    - **Property 2: Round-trip consistency for AuthToken**
    - **Validates: Requirements 16.3**

- [x] 3. Checkpoint - Ensure cargo check passes on data models
  - Ensure all tests pass, ask the user if questions arise.

- [x] 4. Config module
  - [x] 4.1 Implement config/mod.rs with AppConfig, GeneralConfig, TerminalsConfig, ApiConfig, AuthConfig structs and load_config() that reads ~/.madputty/config.toml with env var overrides (MADPUTTY_API_URL, MADPUTTY_TERMINAL, MADPUTTY_PUTTY_PATH, MADPUTTY_TERATERM_PATH)
    - _Requirements: 11.1, 11.2, 11.3, 11.4, 11.5, 11.8_
  - [x] 4.2 Implement set_config_value() for updating individual keys in config.toml
    - _Requirements: 11.6_

- [x] 5. Auth module
  - [x] 5.1 Implement auth/mod.rs with ensure_authenticated() orchestration: env var check → cache load → refresh → interactive prompt → no-interactive error
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.6, 3.7_
  - [x] 5.2 Implement auth/prompt.rs with interactive PAT prompt using console crate
    - _Requirements: 3.4, 13.1_
  - [x] 5.3 Implement auth/oauth.rs with OAuth device code flow: POST device code, display user code + URL, poll with indicatif spinner, handle expiry
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5_

- [x] 6. CLI structure
  - [x] 6.1 Implement cli.rs with clap v4 derive macros: Cli struct with global flags (--verbose, --dry-run, --no-interactive), Command enum with all subcommands (Init, Profiles, Connect, Batch, Vault, Auth, Config), and sub-enums (ProfileAction, VaultAction, AuthAction, ConfigAction)
    - _Requirements: 13.7, 9.5_

- [x] 7. Terminal launcher trait and implementations
  - [x] 7.1 Implement terminal/mod.rs with TerminalLauncher trait (detect, build_args, launch), SessionHandle struct, detect_terminal() function, and TerminalType re-export
    - _Requirements: 14.1, 14.2, 14.4_
  - [x] 7.2 Implement terminal/putty.rs PuTTY backend: detect putty.exe/plink.exe, build SSH args (-ssh, -P, -pw, -i, -load), spawn detached, dry-run support
    - _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5, 7.6_
  - [x] 7.3 Implement terminal/teraterm.rs TeraTerm backend: detect ttermpro.exe, build SSH/serial args, generate .ttl macro for startup commands, delete temp file after launch
    - _Requirements: 8.1, 8.2, 8.3, 8.4, 8.5, 8.6_

- [x] 8. API client
  - [x] 8.1 Implement api/mod.rs with ApiClient struct holding reqwest::Client and in-memory credential cache (HashMap<String, CredentialCacheEntry>)
    - _Requirements: 6.2, 6.3, 12.5_
  - [x] 8.2 Implement api/secrets.rs with Credentials struct, CredentialCacheEntry, fetch_credentials() with TTL cache and 401 retry logic
    - _Requirements: 6.1, 6.2, 6.3, 6.4, 3.8_

- [x] 9. Profile management
  - [x] 9.1 Implement profiles/mod.rs with load_profile(), list_profiles(), save_profile(), remove_profile(), profile_exists() operating on ~/.madputty/profiles/*.toml
    - _Requirements: 1.1, 1.2, 1.3, 1.5, 1.6, 1.7_

- [x] 10. Checkpoint - Ensure cargo check passes on all modules
  - Ensure all tests pass, ask the user if questions arise.

- [x] 11. Connect command
  - [x] 11.1 Implement commands/connect.rs: load profile → detect terminal → auth gate (mid-flow) → fetch vault credentials → launch terminal, with --terminal override and dry-run support
    - _Requirements: 9.1, 9.2, 9.3, 9.4, 9.5, 9.6_

- [x] 12. Batch command
  - [x] 12.1 Implement commands/batch.rs: resolve group → auth gate once → iterate profiles → fetch credentials (using cache) → launch terminals with indicatif progress, continue on individual failure
    - _Requirements: 10.1, 10.2, 10.3, 10.4, 10.5, 10.6_

- [x] 13. Other commands
  - [x] 13.1 Implement commands/profiles.rs: list (tabular display), add (interactive prompts), remove (with confirmation)
    - _Requirements: 1.1, 1.2, 1.3, 1.4_
  - [x] 13.2 Implement commands/vault.rs: auth gate → fetch credentials → display (username, masked password, expiry)
    - _Requirements: 6.1, 12.1_
  - [x] 13.3 Implement commands/auth_cmd.rs: login (invoke auth gate), logout (delete credentials.json), status (display token state)
    - _Requirements: 5.1, 5.2, 5.3, 5.4_
  - [x] 13.4 Implement commands/config_cmd.rs: set (update key in config.toml), show (display effective config with env overrides)
    - _Requirements: 11.6, 11.7_
  - [x] 13.5 Implement commands/init.rs: create ~/.madputty/, ~/.madputty/profiles/, default config.toml; skip if exists
    - _Requirements: 2.1, 2.2, 2.3_
  - [x] 13.6 Implement commands/mod.rs: dispatch function routing Command enum to handler functions
    - _Requirements: 13.7_

- [x] 14. Main entry point
  - [x] 14.1 Implement main.rs with tokio::main, clap parse, tracing setup (verbose flag), command dispatch, exit code handling
    - _Requirements: 13.4, 13.6, 12.1_

- [x] 15. Final checkpoint - Ensure cargo check passes
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- Mid-flow auth is the core architectural pattern — connect and batch commands demonstrate it
- Security: never print/log secrets even in verbose mode (Requirement 12)
