//! Enumerate available COM ports with a decorated table.

use serialport::SerialPortType;

use crate::errors::MadPuttyError;
use crate::theme::Palette;

pub fn run(plain: bool) -> Result<(), MadPuttyError> {
    let ports = serialport::available_ports()?;
    let palette = if plain {
        Palette::plain()
    } else {
        Palette::amazon()
    };

    if ports.is_empty() {
        if plain {
            println!("No COM ports found");
        } else {
            println!(
                "\n  {}  No COM ports found\n",
                palette.error.apply_to("✗")
            );
        }
        return Ok(());
    }

    if plain {
        for p in ports {
            match p.port_type {
                SerialPortType::UsbPort(info) => {
                    let meta = format_usb_meta(&info);
                    if meta.is_empty() {
                        println!("{}", p.port_name);
                    } else {
                        println!("{}  {}", p.port_name, meta);
                    }
                }
                _ => println!("{}", p.port_name),
            }
        }
        return Ok(());
    }

    println!();
    println!(
        "  {}  {}  {}",
        palette.logo_yellow.apply_to("◆"),
        palette.logo_yellow.apply_to("AVAILABLE COM PORTS"),
        palette.dim.apply_to(format!("({} found)", ports.len()))
    );
    println!();

    let top = "  ╭──────────┬──────────┬──────────────────────────────────╮";
    let mid = "  ├──────────┼──────────┼──────────────────────────────────┤";
    let bot = "  ╰──────────┴──────────┴──────────────────────────────────╯";
    println!("{}", palette.border.apply_to(top));
    println!(
        "  {} {:<8} {} {:<8} {} {:<32} {}",
        palette.border.apply_to("│"),
        palette.label.apply_to("PORT"),
        palette.border.apply_to("│"),
        palette.label.apply_to("TYPE"),
        palette.border.apply_to("│"),
        palette.label.apply_to("DEVICE"),
        palette.border.apply_to("│"),
    );
    println!("{}", palette.border.apply_to(mid));

    for p in ports {
        let (kind, detail) = match &p.port_type {
            SerialPortType::UsbPort(info) => ("USB", format_usb_meta(info)),
            SerialPortType::PciPort => ("PCI", String::new()),
            SerialPortType::BluetoothPort => ("BT", String::new()),
            SerialPortType::Unknown => ("Unknown", String::new()),
        };
        let icon = match kind {
            "USB" => "⌨",
            "PCI" => "▣",
            "BT" => "✦",
            _ => "·",
        };

        let detail_display = if detail.is_empty() {
            "—".to_string()
        } else if detail.chars().count() > 30 {
            let mut truncated: String = detail.chars().take(29).collect();
            truncated.push('…');
            truncated
        } else {
            detail
        };

        println!(
            "  {} {:<8} {} {} {:<6} {} {:<32} {}",
            palette.border.apply_to("│"),
            palette.value.apply_to(&p.port_name),
            palette.border.apply_to("│"),
            palette.logo_yellow.apply_to(icon),
            palette.dim.apply_to(kind),
            palette.border.apply_to("│"),
            palette.label.apply_to(detail_display),
            palette.border.apply_to("│"),
        );
    }
    println!("{}", palette.border.apply_to(bot));
    println!();
    println!(
        "  {} madputty {} to connect",
        palette.dim.apply_to("→"),
        palette.value.apply_to("<PORT>")
    );
    println!();

    Ok(())
}

fn format_usb_meta(info: &serialport::UsbPortInfo) -> String {
    let manufacturer = info.manufacturer.as_deref().unwrap_or("").trim();
    let product = info.product.as_deref().unwrap_or("").trim();
    match (manufacturer.is_empty(), product.is_empty()) {
        (true, true) => String::new(),
        (false, true) => manufacturer.to_string(),
        (true, false) => product.to_string(),
        (false, false) => format!("{manufacturer} {product}"),
    }
}
