mod cli;
mod errors;
mod io;
mod list;
mod serial_config;
mod session;
mod theme;

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
            eprintln!("\n  {}  {}\n", red.apply_to("✗"), red.apply_to(format!("{err}")));
            std::process::exit(err.exit_code() as i32);
        }
    }
}

async fn dispatch(cli: Cli) -> Result<(), MadPuttyError> {
    if cli.list || matches!(cli.command, Some(Subcmd::List)) {
        return list::run(cli.plain);
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

    let config = SerialConfig::from(&cli);
    let opts = SessionOptions {
        log: cli.log.clone(),
        send: cli.send.clone(),
        echo: cli.echo,
        plain: cli.plain,
    };

    session::run(&port_name, config, opts).await
}
