//! Command-line interface definition.
//!
//! Mirrors picocom-style invocation: `madputty COM3 --baud 115200` and friends,
//! plus a `list` subcommand (and equivalent `--list` flag) for discovery.

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "madputty", version, about = "Picocom-style serial terminal")]
pub struct Cli {
    /// COM port name, e.g. COM3 (omit when using --list or `list` subcommand).
    pub port: Option<String>,

    /// List available COM ports and exit.
    #[arg(long)]
    pub list: bool,

    /// Baud rate (default 115200).
    #[arg(short = 'b', long, default_value_t = 115_200)]
    pub baud: u32,

    /// Data bits (5, 6, 7, or 8).
    #[arg(short = 'd', long, value_enum, default_value_t = DataBitsArg::Eight)]
    pub data_bits: DataBitsArg,

    /// Parity mode.
    #[arg(short = 'p', long, value_enum, default_value_t = ParityArg::None)]
    pub parity: ParityArg,

    /// Stop bits (1 or 2).
    #[arg(short = 's', long, value_enum, default_value_t = StopBitsArg::One)]
    pub stop_bits: StopBitsArg,

    /// Flow control mode.
    #[arg(short = 'f', long, value_enum, default_value_t = FlowControlArg::None)]
    pub flow_control: FlowControlArg,

    /// Append all port output to this file.
    #[arg(long, value_name = "FILE")]
    pub log: Option<PathBuf>,

    /// Write this file to the port at startup.
    #[arg(long, value_name = "FILE")]
    pub send: Option<PathBuf>,

    /// Echo stdin bytes to stdout before sending them to the port.
    #[arg(long)]
    pub echo: bool,

    /// Disable colors, banner, and decorations (script-friendly output).
    #[arg(long)]
    pub plain: bool,

    /// Enable debug-level tracing on stderr.
    #[arg(long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Option<Subcmd>,
}

#[derive(Subcommand, Debug)]
pub enum Subcmd {
    /// List available COM ports and exit.
    List,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum DataBitsArg {
    #[value(name = "5")]
    Five,
    #[value(name = "6")]
    Six,
    #[value(name = "7")]
    Seven,
    #[value(name = "8")]
    Eight,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum ParityArg {
    None,
    Even,
    Odd,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum StopBitsArg {
    #[value(name = "1")]
    One,
    #[value(name = "2")]
    Two,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum FlowControlArg {
    None,
    Software,
    Hardware,
}
