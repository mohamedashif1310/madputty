//! Serial session with the cyberpunk-aesthetic presentation layer.

use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use serialport::{ErrorKind as SerialErrorKind, SerialPort};
use tokio::sync::{mpsc, oneshot};

use crate::errors::MadPuttyError;
use crate::io::colorizer::Colorizer;
use crate::io::keymap::{event_to_bytes, ExitStateMachine, ForwardOutcome};
use crate::io::stdout_sink::StdoutSink;
use crate::serial_config::SerialConfig;
use crate::theme::{
    boot_sequence, humanize_bytes, print_banner, print_session_summary, typewriter, Palette,
};

#[derive(Debug, Clone, Default)]
pub struct SessionOptions {
    pub log: Option<PathBuf>,
    pub send: Option<PathBuf>,
    pub echo: bool,
    pub plain: bool,
}

/// RAII guard that disables crossterm raw mode on drop.
struct RawModeGuard;

impl RawModeGuard {
    fn new() -> std::io::Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

fn map_open_error(err: serialport::Error, port: &str) -> MadPuttyError {
    let msg = err.to_string();
    let msg_lower = msg.to_lowercase();
    match err.kind() {
        SerialErrorKind::NoDevice => MadPuttyError::PortNotFound {
            port: port.to_string(),
        },
        SerialErrorKind::Io(ErrorKind::NotFound) => MadPuttyError::PortNotFound {
            port: port.to_string(),
        },
        SerialErrorKind::Io(ErrorKind::PermissionDenied) => MadPuttyError::PortBusy {
            port: port.to_string(),
        },
        _ if msg_lower.contains("access is denied") || msg_lower.contains("in use") => {
            MadPuttyError::PortBusy {
                port: port.to_string(),
            }
        }
        _ => MadPuttyError::Serial(err),
    }
}

/// Run a serial session on `port_name`.
pub async fn run(
    port_name: &str,
    config: SerialConfig,
    opts: SessionOptions,
) -> Result<(), MadPuttyError> {
    let palette = if opts.plain {
        Palette::plain()
    } else {
        Palette::amazon()
    };

    // Pre-flight files.
    let sink = match &opts.log {
        Some(path) => StdoutSink::with_log(path)?,
        None => StdoutSink::new(),
    };

    let send_bytes = match &opts.send {
        Some(path) => Some(std::fs::read(path).map_err(|e| MadPuttyError::SendFile {
            path: path.display().to_string(),
            source: e,
        })?),
        None => None,
    };

    // Open the port.
    let mut port = config
        .builder(port_name)
        .open()
        .map_err(|e| map_open_error(e, port_name))?;

    // Banner + boot sequence.
    let framing = config.framing();
    if opts.plain {
        println!(
            "madputty — port={} baud={} framing={}",
            port_name, config.baud, framing
        );
    } else {
        print_banner(port_name, config.baud, &framing, &palette);
        boot_sequence(&palette, opts.plain);
        println!();
        typewriter(
            "▌ streaming live — press Ctrl+A then Ctrl+X to exit",
            &palette.success,
            Duration::from_millis(8),
            opts.plain,
        );
        println!();
    }

    // Optional startup send.
    let mut initial_tx: u64 = 0;
    if let Some(bytes) = send_bytes {
        port.write_all(&bytes)?;
        port.flush()?;
        initial_tx = bytes.len() as u64;
    }

    // Clone port.
    let port_read = port.try_clone()?;
    let port_write = port;

    // Channels + counters.
    let (tx_bytes, rx_bytes) = mpsc::unbounded_channel::<Vec<u8>>();
    let (tx_exit, rx_exit) = oneshot::channel::<()>();
    let (tx_reader_err, rx_reader_err) = oneshot::channel::<MadPuttyError>();
    let shutdown = Arc::new(AtomicBool::new(false));
    let bytes_rx = Arc::new(AtomicU64::new(0));
    let bytes_tx = Arc::new(AtomicU64::new(initial_tx));
    let started = Instant::now();

    // Spawn Port_Reader.
    let shutdown_reader = shutdown.clone();
    let bytes_rx_reader = bytes_rx.clone();
    let palette_reader = if opts.plain {
        Palette::plain()
    } else {
        Palette::amazon()
    };
    let plain = opts.plain;
    let reader_handle = tokio::task::spawn_blocking(move || {
        port_reader_loop(
            port_read,
            sink,
            shutdown_reader,
            tx_reader_err,
            bytes_rx_reader,
            palette_reader,
            plain,
        );
    });

    // Spawn Port_Writer.
    let bytes_tx_counter = bytes_tx.clone();
    let writer_handle = tokio::spawn(port_writer_loop(port_write, rx_bytes, bytes_tx_counter));

    // Spawn Input_Forwarder.
    let shutdown_forwarder = shutdown.clone();
    let echo = opts.echo;
    let forwarder_handle = tokio::task::spawn_blocking(move || {
        input_forwarder_loop(tx_bytes, tx_exit, shutdown_forwarder, echo);
    });

    // Spawn optional status-line refresher (only when not plain and stderr is a tty).
    let status_shutdown = shutdown.clone();
    let status_bytes_rx = bytes_rx.clone();
    let status_bytes_tx = bytes_tx.clone();
    let status_port_name = port_name.to_string();
    let status_baud = config.baud;
    let status_plain = opts.plain;
    let status_handle = tokio::spawn(async move {
        if status_plain {
            return;
        }
        status_line_loop(
            status_shutdown,
            status_bytes_rx,
            status_bytes_tx,
            status_port_name,
            status_baud,
            started,
        )
        .await;
    });

    // Coordinate shutdown.
    let result: Result<(), MadPuttyError> = tokio::select! {
        _ = rx_exit => Ok(()),
        reader_err = rx_reader_err => {
            match reader_err {
                Ok(err) => Err(err),
                Err(_) => Ok(()),
            }
        }
    };

    // Teardown.
    shutdown.store(true, Ordering::Relaxed);
    let _ = reader_handle.await;
    let _ = forwarder_handle.await;
    let _ = writer_handle.await;
    let _ = status_handle.await;

    // Clear the status line.
    if !opts.plain {
        eprint!("\r\x1b[2K");
        let _ = std::io::stderr().flush();
    }

    if let Err(err) = &result {
        let red = console::Style::new().red().bold();
        eprintln!("\n  {}  {}", red.apply_to("✗"), red.apply_to(format!("{err}")));
    }

    if !opts.plain {
        print_session_summary(
            port_name,
            config.baud,
            bytes_rx.load(Ordering::Relaxed),
            bytes_tx.load(Ordering::Relaxed),
            started,
            &palette,
        );
    }

    result
}

fn port_reader_loop(
    mut port: Box<dyn SerialPort>,
    mut sink: StdoutSink,
    shutdown: Arc<AtomicBool>,
    err_tx: oneshot::Sender<MadPuttyError>,
    bytes_rx: Arc<AtomicU64>,
    palette: Palette,
    plain: bool,
) {
    let mut buf = [0u8; 4096];
    let mut colorizer = Colorizer::new(palette, !plain);
    while !shutdown.load(Ordering::Relaxed) {
        match port.read(&mut buf) {
            Ok(0) => continue,
            Ok(n) => {
                bytes_rx.fetch_add(n as u64, Ordering::Relaxed);

                // Write raw bytes to the log file (if any) and colored output
                // to stdout via the colorizer + sink.
                if let Some(log) = sink.log_mut() {
                    if let Err(e) = log.write_all(&buf[..n]) {
                        let _ = err_tx.send(MadPuttyError::PortIo(e));
                        return;
                    }
                }
                let mut stdout = std::io::stdout().lock();
                if let Err(e) = colorizer.feed(&buf[..n], &mut stdout) {
                    let _ = err_tx.send(MadPuttyError::PortIo(e));
                    return;
                }
            }
            Err(e) if e.kind() == ErrorKind::TimedOut => {
                // Flush any partial line (for prompts without newlines).
                let mut stdout = std::io::stdout().lock();
                let _ = colorizer.flush(&mut stdout);
                continue;
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => {
                let _ = err_tx.send(MadPuttyError::PortIo(e));
                return;
            }
        }
    }
    let mut stdout = std::io::stdout().lock();
    let _ = colorizer.flush(&mut stdout);
}

async fn port_writer_loop(
    mut port: Box<dyn SerialPort>,
    mut rx: mpsc::UnboundedReceiver<Vec<u8>>,
    bytes_tx: Arc<AtomicU64>,
) {
    while let Some(bytes) = rx.recv().await {
        bytes_tx.fetch_add(bytes.len() as u64, Ordering::Relaxed);
        if let Err(e) = port.write_all(&bytes) {
            tracing::warn!("port write failed: {e}");
            break;
        }
        if let Err(e) = port.flush() {
            tracing::warn!("port flush failed: {e}");
            break;
        }
    }
}

fn input_forwarder_loop(
    tx_bytes: mpsc::UnboundedSender<Vec<u8>>,
    exit_tx: oneshot::Sender<()>,
    shutdown: Arc<AtomicBool>,
    echo: bool,
) {
    let _raw = match RawModeGuard::new() {
        Ok(g) => g,
        Err(e) => {
            tracing::warn!("failed to enable raw mode: {e}");
            let _ = exit_tx.send(());
            return;
        }
    };

    let mut state = ExitStateMachine::new();
    let mut exit_tx = Some(exit_tx);

    while !shutdown.load(Ordering::Relaxed) {
        match event::poll(Duration::from_millis(50)) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(e) => {
                tracing::warn!("crossterm poll error: {e}");
                break;
            }
        }

        let evt = match event::read() {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("crossterm read error: {e}");
                break;
            }
        };

        if !matches!(evt, Event::Key(_)) {
            continue;
        }

        let bytes = event_to_bytes(&evt);
        if bytes.is_empty() {
            continue;
        }

        match state.feed(&bytes) {
            ForwardOutcome::Bytes(v) => {
                if echo {
                    let mut out = std::io::stdout().lock();
                    let _ = out.write_all(&v);
                    let _ = out.flush();
                }
                if tx_bytes.send(v).is_err() {
                    break;
                }
            }
            ForwardOutcome::ExitRequested => {
                if let Some(tx) = exit_tx.take() {
                    let _ = tx.send(());
                }
                break;
            }
            ForwardOutcome::Continue => {}
        }
    }

    drop(tx_bytes);
}

/// Refresh a single-line status bar on stderr every second.
/// Uses ANSI `\r\x1b[2K` to clear and rewrite the line.
async fn status_line_loop(
    shutdown: Arc<AtomicBool>,
    bytes_rx: Arc<AtomicU64>,
    bytes_tx: Arc<AtomicU64>,
    port: String,
    baud: u32,
    started: Instant,
) {
    let mut last_rx = 0u64;
    let mut interval = tokio::time::interval(Duration::from_millis(1000));
    // Wait a bit before first draw so the banner settles.
    tokio::time::sleep(Duration::from_millis(500)).await;

    while !shutdown.load(Ordering::Relaxed) {
        interval.tick().await;
        let rx = bytes_rx.load(Ordering::Relaxed);
        let tx = bytes_tx.load(Ordering::Relaxed);
        let rate = rx.saturating_sub(last_rx);
        last_rx = rx;
        let elapsed = started.elapsed();
        let elapsed_str = format_short_elapsed(elapsed);

        // Amazon status bar: yellow background, black text.
        let bg = "\x1b[43m"; // on yellow
        let fg = "\x1b[30m"; // black
        let reset = "\x1b[0m";
        let label = "\x1b[30;1m"; // bold black
        let line = format!(
            "\r\x1b[2K{bg}{fg} ▌ {label}PORT {fg}{port}  {label}BAUD {fg}{baud}  {label}UP {fg}{elapsed_str}  {label}RX {fg}{}  {label}TX {fg}{}  {label}RATE {fg}{}/s {reset}",
            humanize_bytes(rx),
            humanize_bytes(tx),
            humanize_bytes(rate),
        );
        eprint!("{line}");
        let _ = std::io::stderr().flush();
    }
}

fn format_short_elapsed(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m{:02}s", s / 60, s % 60)
    } else {
        format!("{}h{:02}m", s / 3600, (s % 3600) / 60)
    }
}
