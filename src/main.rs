mod ai;
mod cli;
mod errors;
mod io;
mod list;
mod serial_config;
mod serial_port_trait;
mod session;
mod theme;
mod ui;

use clap::Parser;
use console::Style;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Subcmd};
use crate::errors::{ExitCode, MadPuttyError};
use crate::serial_config::SerialConfig;
use crate::session::SessionOptions;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let filter = if cli.verbose {
        EnvFilter::new("madputty=debug")
    } else {
        EnvFilter::new("madputty=warn")
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();

    let result = dispatch(cli).await;

    match result {
        Ok(()) => std::process::exit(ExitCode::Success as i32),
        Err(err) => {
            let red = Style::new().red().bold();
            eprintln!(
                "\n  {}  {}\n",
                red.apply_to("✗"),
                red.apply_to(format!("{err}"))
            );
            std::process::exit(err.exit_code() as i32);
        }
    }
}

async fn dispatch(cli: Cli) -> Result<(), MadPuttyError> {
    // Subcommand dispatch
    match &cli.command {
        Some(Subcmd::List) => return list::run(cli.plain),
        Some(Subcmd::KiroLogin) => return kiro_login().await,
        Some(Subcmd::KiroStatus) => return kiro_status().await,
        None if cli.list => return list::run(cli.plain),
        None => {}
    }

    let port_name = match &cli.port {
        Some(name) => name.clone(),
        None => {
            return Err(MadPuttyError::InvalidArg(
                "missing COM port argument (try `madputty list` to see available ports)"
                    .to_string(),
            ));
        }
    };

    // Warn about conflicting flags
    if cli.no_ai && (cli.ai_watch || cli.no_redact) {
        eprintln!("⚠ --no-ai overrides other AI flags");
    }

    let config = SerialConfig::from(&cli);
    let opts = SessionOptions {
        log: cli.log.clone(),
        send: cli.send.clone(),
        echo: cli.echo,
        plain: cli.plain,
        ai_watch: cli.ai_watch,
        ai_timeout_seconds: cli.ai_timeout_seconds,
        no_redact: cli.no_redact,
        no_ai: cli.no_ai,
    };

    session::run(&port_name, config, opts).await
}

/// Delegate to `kiro-cli login` with inherited stdio.
async fn kiro_login() -> Result<(), MadPuttyError> {
    let kiro_path = ai::find_kiro_cli_or_error()?;
    let status = std::process::Command::new(&kiro_path)
        .arg("login")
        .status()
        .map_err(|e| MadPuttyError::AiError(format!("failed to run kiro-cli login: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(MadPuttyError::AiError("kiro-cli login failed".to_string()))
    }
}

/// Delegate to `kiro-cli whoami`.
///
/// `whoami` does not accept `--no-interactive`; we inherit stdio so the user
/// sees the real output (Builder ID, profile, etc.).
async fn kiro_status() -> Result<(), MadPuttyError> {
    let kiro_path = ai::find_kiro_cli_or_error()?;
    let status = std::process::Command::new(&kiro_path)
        .arg("whoami")
        .status()
        .map_err(|e| MadPuttyError::AiError(format!("failed to run kiro-cli whoami: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(MadPuttyError::AiError(
            "kiro-cli: not logged in (run `madputty kiro-login`)".to_string(),
        ))
    }
}
