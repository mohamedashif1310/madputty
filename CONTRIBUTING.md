# Contributing to MadPutty

## Development Setup

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone <repo-url>
cd madputty
cargo build
```

## Running Tests

```bash
cargo test --tests       # Property-based tests
cargo test --test '*'    # Integration tests
cargo clippy             # Lint checks
cargo fmt -- --check     # Format check
```

## Code Style

- Follow standard Rust conventions (rustfmt defaults)
- Use `thiserror` for error types, map to exit codes
- Use `tracing` for logging, never log secrets
- All public functions need doc comments
- New terminal backends implement `TerminalLauncher` trait
- New AI providers implement `AiProvider` trait

## Pull Request Process

1. Create a feature branch
2. Make changes, ensure `cargo check` passes
3. Add tests for new functionality
4. Run `cargo clippy -- -D warnings`
5. Run `cargo fmt`
6. Submit PR with description of changes

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for module structure and design decisions.

## Adding a Terminal Backend

1. Create `src/terminal/yourterm.rs`
2. Implement `TerminalLauncher` trait (detect, build_args, launch)
3. Add enum variant to `TerminalType`
4. Add match arm in `detect_terminal()`
5. Add property tests in `tests/terminal_properties.rs`

## Adding an AI Provider

1. Create `src/ai/providers/yourprovider.rs`
2. Implement `AiProvider` trait (analyze_error)
3. Add match arm in `create_provider()`
4. Add config documentation
