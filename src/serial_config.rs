//! Serial port configuration mapping.
//!
//! Bridges the CLI value enums into `serialport` enum types and produces a
//! fully configured `SerialPortBuilder` plus a human-readable framing string
//! (e.g. `"8N1"`) for the session banner.

use std::time::Duration;

use serialport::{DataBits, FlowControl, Parity, SerialPortBuilder, StopBits};

use crate::cli::{Cli, DataBitsArg, FlowControlArg, ParityArg, StopBitsArg};

#[derive(Debug, Clone, Copy)]
pub struct SerialConfig {
    pub baud: u32,
    pub data_bits: DataBits,
    pub parity: Parity,
    pub stop_bits: StopBits,
    pub flow_control: FlowControl,
}

impl SerialConfig {
    /// Default 115200 8N1, no flow control.
    #[allow(dead_code)]
    pub fn defaults() -> Self {
        Self {
            baud: 115_200,
            data_bits: DataBits::Eight,
            parity: Parity::None,
            stop_bits: StopBits::One,
            flow_control: FlowControl::None,
        }
    }

    /// Build a `serialport` builder ready to open. Uses a 50 ms read timeout
    /// so the blocking Port_Reader can cooperatively shut down.
    pub fn builder(&self, port_name: &str) -> SerialPortBuilder {
        serialport::new(port_name, self.baud)
            .data_bits(self.data_bits)
            .parity(self.parity)
            .stop_bits(self.stop_bits)
            .flow_control(self.flow_control)
            .timeout(Duration::from_millis(50))
    }

    /// Render the banner framing string, e.g. `"8N1"`.
    pub fn framing(&self) -> String {
        let d = match self.data_bits {
            DataBits::Five => 5,
            DataBits::Six => 6,
            DataBits::Seven => 7,
            DataBits::Eight => 8,
        };
        let p = match self.parity {
            Parity::None => 'N',
            Parity::Even => 'E',
            Parity::Odd => 'O',
        };
        let s = match self.stop_bits {
            StopBits::One => 1,
            StopBits::Two => 2,
        };
        format!("{d}{p}{s}")
    }
}

impl From<&Cli> for SerialConfig {
    fn from(cli: &Cli) -> Self {
        Self {
            baud: cli.baud,
            data_bits: match cli.data_bits {
                DataBitsArg::Five => DataBits::Five,
                DataBitsArg::Six => DataBits::Six,
                DataBitsArg::Seven => DataBits::Seven,
                DataBitsArg::Eight => DataBits::Eight,
            },
            parity: match cli.parity {
                ParityArg::None => Parity::None,
                ParityArg::Even => Parity::Even,
                ParityArg::Odd => Parity::Odd,
            },
            stop_bits: match cli.stop_bits {
                StopBitsArg::One => StopBits::One,
                StopBitsArg::Two => StopBits::Two,
            },
            flow_control: match cli.flow_control {
                FlowControlArg::None => FlowControl::None,
                FlowControlArg::Software => FlowControl::Software,
                FlowControlArg::Hardware => FlowControl::Hardware,
            },
        }
    }
}
