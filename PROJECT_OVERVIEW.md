# MadPutty — End-to-End Project Overview

> This document is a complete reference for the MadPutty project. It is designed so that an AI assistant (like Kiro) can fully understand the architecture, patterns, and implementation details to help with related projects.

## 1. What Is MadPutty?

MadPutty is a **Rust CLI application for Windows** that manages SSH and serial terminal sessions. It acts as a wrapper/enhancer around PuTTY and TeraTerm, with three defining capabilities:

1. **Local profile management** — Connection profiles stored as individual TOML files, no auth needed for CRUD.
2. **Mid-flow authentication** — The core architectural pattern: commands start executing locally, then authenticate ONLY at the exact point where remote resources are needed.
3. **AI-powered log monitoring** — Optional real-time analysis of terminal session logs with error detection, credential redaction, and AI debugging suggestions.

**Target platform:** Windows (primary), with code structured for future Linux/macOS support.

**Language:** Rust, edition 2021.

**Key design principle:** Local operations should never require network access. Authentication happens lazily, only when genuinely needed.

---

## 2. The Mid-Flow Authentication Pattern (Core Concept)

This is THE defining pattern of the project. Every command is classified as either:

- **Local-only** (no auth): `init`, `profiles list/add/remove`, `config set/show`, `auth logout/status`
- **Mid-flow auth** (authenticates only when needed): `connect`, `batch`, `vault fetch`, `auth login`, `ask`, `logs analyze`

### How Mid-Flow Auth Works

The `connect` command demonstrates the pattern:

```
$ madputty connect production-server-1

 ● Loading profile "production-server-1"... ✓    ← Local TOML read, NO auth
 ● Host: 10.0.1.50 | Port: 22 | Protocol: SSH    ← Validation, NO auth
 ● Detecting terminal (PuTTY)... ✓                ← PATH lookup, NO auth

 🔐 Authentication required to fetch credentials from vault.  ← AUTH GATE TRIGGERED HERE
 → No cached token found.
 → Please enter your API token: ••••••••••••••
 → Token cached for future sessions.

 ● Fetching credentials from vault... ✓           ← API call WITH token
 ● Launching PuTTY session...
 ✓ Connected (PID: 14320)                         ← Process spawned
```

### The `ensure_authenticated()` Function

Located in `src/auth/mod.rs`. Called lazily by commands that need remote resources.

**Decision flow:**
1. Check `MADPUTTY_AUTH_TOKEN` env var → use directly if set (for CI/automation)
2. Load cached token from `~/.madputty/credentials.json`
3. If cached token valid and not expired → return it
4. If expired → attempt refresh using stored refresh token
5. If refresh fails or no token → prompt interactively (PAT input or OAuth device flow)
6. If `--no-interactive` flag set → return `AuthRequired` error (exit code 2)
7. Save new token → return it

### Auto-Retry on 401

If any vault API call returns HTTP 401:
1. Clear the cached token
2. Call `ensure_authenticated()` exactly once more
3. Retry the API call
4. If second attempt also 401 → exit with code 2

---

## 3. Project Structure

```
madputty/
├── Cargo.toml                    # Dependencies, build profile
├── README.md                     # User-facing documentation
├── ARCHITECTURE.md               # High-level architecture
├── CONTRIBUTING.md               # Development guide
├── LICENSE                       # MIT
├── .gitignore
├── .github/workflows/
│   ├── ci.yml                    # Build + test on 3 platforms
│   └── release.yml               # Cross-platform release binaries
├── .kiro/specs/
│   ├── madputty-cli/            # Core CLI spec (requirements, design, tasks)
│   └── ai-log-monitoring/       # AI feature spec
├── src/
│   ├── main.rs                   # tokio::main, CLI parse, dispatch
│   ├── cli.rs                    # All clap derive structs and enums
│   ├── errors.rs                 # MadPuttyError enum + exit code mapping
│   ├── auth/
│   │   ├── mod.rs                # ensure_authenticated() — the auth gate
│   │   ├── token.rs              # AuthToken struct, load/save/validate
│   │   ├── oauth.rs              # OAuth device code flow
│   │   └── prompt.rs             # Interactive PAT prompt
│   ├── api/
│   │   ├── mod.rs                # ApiClient, in-memory credential cache
│   │   └── secrets.rs            # Vault fetch with TTL cache + 401 retry
│   ├── terminal/
│   │   ├── mod.rs                # TerminalLauncher trait
│   │   ├── putty.rs              # PuTTY/plink backend
│   │   └── teraterm.rs           # TeraTerm + .ttl macro generation
│   ├── profiles/
│   │   ├── mod.rs                # CRUD operations
│   │   └── models.rs             # ConnectionProfile, enums
│   ├── config/
│   │   └── mod.rs                # AppConfig, TOML + env overrides
│   ├── commands/
│   │   ├── mod.rs                # Dispatch router
│   │   ├── init.rs               # madputty init
│   │   ├── connect.rs            # madputty connect (the star command)
│   │   ├── batch.rs              # madputty batch
│   │   ├── profiles.rs           # madputty profiles {list|add|remove}
│   │   ├── vault.rs              # madputty vault fetch
│   │   ├── auth_cmd.rs           # madputty auth {login|logout|status}
│   │   ├── config_cmd.rs         # madputty config {set|show}
│   │   ├── ask.rs                # madputty ask "question"
│   │   └── logs_cmd.rs           # madputty logs {show|analyze}
│   └── ai/
│       ├── mod.rs                # AiProvider trait, AiSuggestion
│       ├── redaction.rs          # Credential masking engine
│       ├── log_watcher.rs        # Async file watcher + error detection
│       ├── notification.rs       # Windows toast / console fallback
│       └── providers/
│           ├── mod.rs            # Provider factory
│           ├── openai.rs         # OpenAI GPT backend
│           ├── anthropic.rs      # Anthropic Claude backend
│           └── local.rs          # Local Ollama backend
└── tests/
    ├── auth_properties.rs        # Property tests: token expiry
    ├── profile_properties.rs     # Property tests: TOML round-trip
    ├── redaction_properties.rs   # Property tests: redaction idempotence
    ├── terminal_properties.rs    # Property tests: argument building
    ├── integration_tests.rs      # Integration test entry
    └── integration/
        ├── vault_fetch_test.rs   # Mock vault API tests (wiremock)
        └── config_round_trip_test.rs
```

**Total:** 33 source files, 7 test files, 50+ files overall.


---

## 12. Configuration

### Config file: `~/.madputty/config.toml`

```toml
[general]
default_terminal = "putty"
verbose = false

[terminals.putty]
path = "C:/Program Files/PuTTY/putty.exe"
plink_path = "C:/Program Files/PuTTY/plink.exe"

[terminals.teraterm]
path = "C:/Program Files (x86)/teraterm/ttermpro.exe"

[api]
base_url = "https://api.mycompany.com"
timeout_seconds = 30

[auth]
method = "token"
oauth_client_id = "madputty-cli"
oauth_device_url = "https://auth.mycompany.com/device"
oauth_token_url = "https://auth.mycompany.com/token"

[ai]
enabled = true
provider = "openai"
model = "gpt-4o"

[ai.redaction]
patterns = ["MY_SECRET=\\S+"]

[ai.detection]
patterns = ["(?i)error", "(?i)fatal"]
```

### Environment Variable Overrides

| Variable | Overrides |
|----------|-----------|
| `MADPUTTY_AUTH_TOKEN` | Skip all auth (CI mode) |
| `MADPUTTY_API_URL` | `api.base_url` |
| `MADPUTTY_TERMINAL` | `general.default_terminal` |
| `MADPUTTY_PUTTY_PATH` | `terminals.putty.path` |
| `MADPUTTY_TERATERM_PATH` | `terminals.teraterm.path` |
| `MADPUTTY_AI_API_KEY` | `ai.api_key` |
| `MADPUTTY_AI_PROVIDER` | `ai.provider` |

---

## 13. Testing Strategy

### Property-Based Tests (proptest)

- `tests/auth_properties.rs` — Token expiry logic, JSON round-trip
- `tests/profile_properties.rs` — ConnectionProfile TOML/JSON round-trip
- `tests/redaction_properties.rs` — Idempotence, password/token/IP masking
- `tests/terminal_properties.rs` — PuTTY/TeraTerm argument building

### Integration Tests (wiremock)

- `tests/integration/vault_fetch_test.rs` — Mock vault API (200/401/404/500)
- `tests/integration/config_round_trip_test.rs` — Full config TOML round-trip

### Running Tests

```bash
cargo test --tests           # All tests
cargo test --test auth_properties   # Single test file
```

---

## 14. How to Extend

### Add a New Terminal Backend

1. Create `src/terminal/newterm.rs` implementing `TerminalLauncher`
2. Add variant to `TerminalType` enum in `src/profiles/models.rs`
3. Add match arm in `detect_terminal()` in `src/terminal/mod.rs`
4. Add property tests in `tests/terminal_properties.rs`

### Add a New AI Provider

1. Create `src/ai/providers/newprovider.rs` implementing `AiProvider`
2. Add match arm in `create_provider()` in `src/ai/providers/mod.rs`

### Add a New Command

1. Define variant in `Command` enum in `src/cli.rs`
2. Create `src/commands/newcmd.rs` with `run()` function
3. Add `pub mod newcmd;` in `src/commands/mod.rs`
4. Wire dispatch in `commands::dispatch()` match arm

---

## 15. Installation & Usage

```bash
# Build release binary
cargo build --release
# Binary: target/release/madputty.exe

# Or install globally
cargo install --path .

# First-time setup
madputty init
madputty profiles add

# Daily use
madputty connect prod-db
madputty connect prod-db --ai-debug
madputty batch datacenter-east
madputty ask "why is SSH timing out?"
```

---

## 16. Key Patterns to Apply to Other Projects

### Pattern 1: Mid-Flow Lazy Authentication

Instead of authenticating at startup, defer auth until the exact point it's needed. Benefits:
- Local operations remain offline-capable
- Faster feedback for errors that don't need auth
- Better UX (user only sees auth prompts when truly needed)

Implementation: Have a single `ensure_authenticated()` function. Call it from command handlers right before the first remote operation.

### Pattern 2: Trait-Based Backend Abstraction

Define a trait for each pluggable component:
- `TerminalLauncher` for terminal emulators
- `AiProvider` for AI backends

Use a factory function to select the implementation based on config. Box<dyn Trait> for dynamic dispatch.

### Pattern 3: Layered Configuration

Config precedence (lowest to highest):
1. Built-in defaults
2. TOML config file
3. Environment variables
4. CLI flags (per-command)

Apply overrides in `load_config()` before returning the final `AppConfig`.

### Pattern 4: Security Boundaries via Types

Use the type system to enforce security rules. Example: the `RedactionEngine` is the ONLY way to prepare text for AI providers. Make provider methods require `&str` inputs that were generated by `RedactionEngine::redact()`.

### Pattern 5: Error Types Map to Exit Codes

Use `thiserror` for error enums. Each variant has an associated exit code. Single `exit()` method centralizes error display + process exit.

### Pattern 6: Background Tasks via tokio + mpsc

For real-time processing pipelines (like log watching → error detection → AI analysis):
- Spawn background tasks with `tokio::spawn`
- Connect stages with `tokio::sync::mpsc::channel`
- Each stage can operate independently and backpressure is handled by channel capacity

### Pattern 7: Round-Trip Properties

For any data that gets serialized to disk, write a property test:
```rust
proptest! {
    fn round_trip(data in arb_data()) {
        let serialized = serialize(&data);
        let deserialized = deserialize(&serialized);
        prop_assert_eq!(data, deserialized);
    }
}
```

This catches subtle serialization bugs and validates serde configuration.

---

## 17. CI/CD

### `.github/workflows/ci.yml`

Runs on push/PR to main:
- Matrix: windows-latest, ubuntu-latest, macos-latest
- Jobs: check, test, clippy, fmt
- Uses `Swatinem/rust-cache@v2` for dependency caching

### `.github/workflows/release.yml`

Triggers on tag push (`v*`):
- Builds release binaries for Windows/Linux/macOS
- Creates GitHub release with artifacts
- Auto-generates release notes

---

## 18. File-by-File Purpose

### Core Files

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point, tokio runtime setup, tracing config, dispatch |
| `src/cli.rs` | All clap structs: Cli, Command, ProfileAction, etc. |
| `src/errors.rs` | MadPuttyError enum with exit code mapping |

### Auth

| File | Purpose |
|------|---------|
| `src/auth/mod.rs` | `ensure_authenticated()` — the central auth gate |
| `src/auth/token.rs` | AuthToken struct, load/save to credentials.json |
| `src/auth/oauth.rs` | OAuth device code flow with polling |
| `src/auth/prompt.rs` | Interactive PAT input via dialoguer |

### API

| File | Purpose |
|------|---------|
| `src/api/mod.rs` | ApiClient struct with reqwest client + credential cache |
| `src/api/secrets.rs` | Vault fetch, TTL cache, 401 retry logic |

### Terminal

| File | Purpose |
|------|---------|
| `src/terminal/mod.rs` | TerminalLauncher trait, detect_terminal() |
| `src/terminal/putty.rs` | putty.exe / plink.exe detection + spawn |
| `src/terminal/teraterm.rs` | ttermpro.exe + .ttl macro generation |

### Profiles

| File | Purpose |
|------|---------|
| `src/profiles/mod.rs` | load/list/save/remove profile TOML files |
| `src/profiles/models.rs` | ConnectionProfile, Protocol, enums |

### Config

| File | Purpose |
|------|---------|
| `src/config/mod.rs` | AppConfig + all subconfigs, load with env overrides |

### Commands

| File | Purpose |
|------|---------|
| `src/commands/mod.rs` | Dispatch Command enum to handler functions |
| `src/commands/init.rs` | Create ~/.madputty/ structure |
| `src/commands/connect.rs` | THE mid-flow auth command (star of the show) |
| `src/commands/batch.rs` | Group-based multi-connect (auth once) |
| `src/commands/profiles.rs` | profiles list/add/remove |
| `src/commands/vault.rs` | vault fetch |
| `src/commands/auth_cmd.rs` | auth login/logout/status |
| `src/commands/config_cmd.rs` | config set/show |
| `src/commands/ask.rs` | AI freeform question |
| `src/commands/logs_cmd.rs` | logs show/analyze |

### AI Module

| File | Purpose |
|------|---------|
| `src/ai/mod.rs` | AiProvider trait, AiSuggestion |
| `src/ai/redaction.rs` | RedactionEngine (security boundary) |
| `src/ai/log_watcher.rs` | notify-based file watcher, error detection, debouncing |
| `src/ai/notification.rs` | Toast notifications (Windows) or console (others) |
| `src/ai/providers/mod.rs` | Provider factory |
| `src/ai/providers/openai.rs` | OpenAI via async-openai |
| `src/ai/providers/anthropic.rs` | Anthropic Claude via reqwest |
| `src/ai/providers/local.rs` | Local Ollama via reqwest to localhost:11434 |

---

## 19. Technology Summary

**Async runtime:** tokio (full features)
**CLI parsing:** clap v4 with derive macros
**Serialization:** serde + toml + serde_json
**HTTP client:** reqwest with json feature
**Error handling:** thiserror
**Terminal UI:** console (colors) + indicatif (spinners) + dialoguer (prompts)
**Logging:** tracing + tracing-subscriber
**File watching:** notify v6
**Regex:** regex v1
**AI SDK:** async-openai (OpenAI), manual reqwest (Anthropic/Ollama)
**Testing:** proptest (property-based) + wiremock (HTTP mocking)

---

## 20. Key Design Decisions & Rationale

| Decision | Rationale |
|----------|-----------|
| Individual TOML files per profile | Simple CRUD, git-friendly, easy to share/backup |
| JSON for token cache | Fast parsing, includes DateTime with chrono |
| Mid-flow auth | Local ops offline-capable, better UX |
| Trait-based backends | Future-proof for new terminals/AI providers |
| Env var overrides | Required for CI/automation |
| `async-trait` for AiProvider | HTTP calls need async, trait methods can't be async natively |
| notify crate for log watching | Cross-platform, event-based (not polling) |
| 5s debounce window | Avoids AI API spam on error bursts |
| 50-line rolling buffer | Provides context for AI without overloading |
| winrt-notification (cfg(windows)) | Native Windows UX, console fallback elsewhere |
| thiserror for errors | Clean error types, integrates with `?` operator |
| Release profile with LTO+strip | Smaller binary (~2MB vs ~15MB unoptimized) |

---

## 21. Testing Philosophy

Every serialization code path has a property-based round-trip test. Every security boundary (redaction) has idempotence tests. Every external API has mock-based integration tests.

**What property-based testing validates:**
- Round-trip consistency: `deserialize(serialize(x)) == x`
- Idempotence: `f(f(x)) == f(x)` (for redaction)
- Invariants: "every SSH argument list contains `-ssh` and `-P`"
- Universal properties: "redacted output never contains the original password"

**What integration tests validate:**
- HTTP status code handling (200/401/404/500)
- Request construction (headers, paths, methods)
- Response parsing

---

## 22. Quick Start for New Developers

```bash
# 1. Clone and build
git clone <repo>
cd madputty
cargo build

# 2. Run tests
cargo test --tests

# 3. Try it
cargo run -- init
cargo run -- profiles add
cargo run -- profiles list

# 4. Release build (smaller binary)
cargo build --release
# → target/release/madputty.exe

# 5. Install globally
cargo install --path .
```

---

## 23. Common Pitfalls

1. **Don't use `format!("{var}")` inside `prop_assert!`** — macro hygiene breaks captured variables. Use `format!("{}", var)` or assign to a local variable first.

2. **proptest tuples max at 12 elements** — For larger data, nest two tuples and destructure in `.prop_map()`.

3. **Terminal `launch()` signature takes `log_file: Option<&Path>`** — Don't forget to pass `None` for commands that don't support log watching (like `batch`).

4. **Redaction must happen before AI calls** — The pattern is: `RedactionEngine::new() → redact() → provider.analyze_error(redacted)`.

5. **Config env vars are applied after TOML load** — So env vars always win over config file values. This is intentional.

6. **Windows toast requires `cfg(windows)`** — Other platforms use console fallback. Don't assume `winrt-notification` is always available.

7. **Session IDs need both profile name AND timestamp** — Same profile connected twice must produce different session IDs.

---

## 24. What to Remember for Related Projects

When starting a new project inspired by MadPutty, remember these core architectural choices:

1. **Lazy auth is powerful** — Don't authenticate until you must. This is the single most valuable pattern.
2. **Traits for extensibility** — Define traits for any component that might have multiple implementations.
3. **Env vars > config file > defaults** — This precedence is standard and expected.
4. **Exit codes matter** — Map error types to specific exit codes for scripting.
5. **Security boundaries via the type system** — Use types to enforce "redact before sending to AI".
6. **Property tests for serialization** — Always, no exceptions.
7. **Background tasks + channels** — Decouple detection from processing for resilience.
8. **Colored output but credential masking** — Make output friendly but never leak secrets.

---

**End of overview.** This document captures the complete design and implementation of MadPutty. Use it as a reference when building related projects or extending this one.
