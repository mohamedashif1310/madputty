//! Serial session with AI-powered log analysis via kiro-cli.
//!
//! Two independent lanes:
//! - Log pump lane: port_reader → colorizer → stdout (never blocks)
//! - AI subsystem lane: rolling_buffer → redactor → kiro_invoker → ai_pane

use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use serialport::ErrorKind as SerialErrorKind;
use tokio::sync::{mpsc, oneshot};

use crate::ai::rolling_buffer::RollingBuffer;
use crate::ai::AiSubsystem;
use crate::errors::MadPuttyError;
use crate::io::colorizer::Colorizer;
use crate::io::keymap::{event_to_bytes, HotkeyAction, HotkeyDispatcher};
use crate::io::stdout_sink::StdoutSink;
use crate::serial_config::SerialConfig;
use crate::serial_port_trait::{DuplexStream, SerialPortStream};
use crate::theme::{
    boot_sequence, humanize_bytes, print_banner, print_session_summary, typewriter, Palette,
};

#[derive(Debug, Clone, Default)]
pub struct SessionOptions {
    pub log: Option<PathBuf>,
    pub send: Option<PathBuf>,
    pub echo: bool,
    pub plain: bool,
    pub ai_watch: bool,
    pub ai_timeout_seconds: u32,
    pub no_redact: bool,
    pub no_ai: bool,
}

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
    let msg = err.to_string().to_lowercase();
    match err.kind() {
        SerialErrorKind::NoDevice | SerialErrorKind::Io(ErrorKind::NotFound) => {
            MadPuttyError::PortNotFound {
                port: port.to_string(),
            }
        }
        SerialErrorKind::Io(ErrorKind::PermissionDenied) => MadPuttyError::PortBusy {
            port: port.to_string(),
        },
        _ if msg.contains("access is denied") || msg.contains("in use") => {
            MadPuttyError::PortBusy {
                port: port.to_string(),
            }
        }
        _ => MadPuttyError::Serial(err),
    }
}

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

    // Detect AI subsystem
    let ai = AiSubsystem::detect(opts.no_ai, opts.ai_timeout_seconds).await;
    let ai_enabled = ai.enabled && ai.logged_in;

    // Redaction warning
    if opts.no_redact && ai_enabled {
        eprintln!("⚠ redaction disabled (--no-redact) — credentials may leak to kiro-cli");
    }

    // Auto-watch warning
    if opts.ai_watch && !ai.enabled {
        eprintln!("⚠ --ai-watch ignored because AI is disabled");
    }

    // Pre-flight files
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

    // Open port
    // (moved to "Startup send" section below for write-before-clone pattern)

    // Banner
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
        let hint = if ai_enabled {
            "▌ streaming live — Ctrl+A A for AI analysis, Ctrl+A X to exit"
        } else {
            "▌ streaming live — press Ctrl+A then Ctrl+X to exit"
        };
        typewriter(hint, &palette.success, Duration::from_millis(8), opts.plain);
        println!();
    }

    // Startup send
    let mut initial_tx: u64 = 0;
    let mut stream = SerialPortStream::new(
        config
            .builder(port_name)
            .open()
            .map_err(|e| map_open_error(e, port_name))?,
    );
    if let Some(bytes) = send_bytes {
        use std::io::Write as _;
        stream.write_all(&bytes)?;
        stream.flush()?;
        initial_tx = bytes.len() as u64;
    }

    // Clone port into read/write halves
    let port_read = stream.try_clone_stream()?;
    let port_write: Box<dyn DuplexStream> = Box::new(stream);

    // Channels + counters
    let (tx_bytes, rx_bytes) = mpsc::unbounded_channel::<Vec<u8>>();
    let (tx_exit, rx_exit) = oneshot::channel::<()>();
    let (tx_reader_err, rx_reader_err) = oneshot::channel::<MadPuttyError>();
    let shutdown = Arc::new(AtomicBool::new(false));
    let bytes_rx = Arc::new(AtomicU64::new(0));
    let bytes_tx = Arc::new(AtomicU64::new(initial_tx));
    let started = Instant::now();

    // Rolling buffer for AI context
    let rolling_buffer = RollingBuffer::new();
    let rb_writer = rolling_buffer.clone();

    // AI analysis channel (hotkey triggers → AI task)
    let (tx_ai_trigger, rx_ai_trigger) = mpsc::channel::<AiTrigger>(4);

    // Response log
    let session_id = format!(
        "{}-{}",
        chrono_session_id(),
        port_name.replace(['\\', '/', ':'], "-")
    );
    let response_log = crate::ai::response_log::ResponseLog::new(&session_id);

    // Spawn Port_Reader
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
            rb_writer,
        );
    });

    // Spawn Port_Writer
    let bytes_tx_counter = bytes_tx.clone();
    let writer_handle = tokio::spawn(port_writer_loop(port_write, rx_bytes, bytes_tx_counter));

    // Spawn Input_Forwarder
    let shutdown_forwarder = shutdown.clone();
    let echo = opts.echo;
    let tx_ai = tx_ai_trigger.clone();
    let forwarder_handle = tokio::task::spawn_blocking(move || {
        input_forwarder_loop(tx_bytes, tx_exit, shutdown_forwarder, echo, ai_enabled, tx_ai);
    });

    // Spawn status bar
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

    // Spawn AI analysis consumer task — returns the response_log so we can check has_entries
    let ai_shutdown = shutdown.clone();
    let ai_rb = rolling_buffer.clone();
    let ai_no_redact = opts.no_redact;
    let ai_handle: tokio::task::JoinHandle<crate::ai::response_log::ResponseLog> = tokio::spawn(async move {
        let mut rx = rx_ai_trigger;
        let mut rlog = response_log;
        ai_consumer_loop(ai, ai_rb, &mut rx, &mut rlog, ai_no_redact, ai_shutdown).await;
        rlog
    });

    // Spawn auto-watch error scanner (if enabled)
    let autowatch_handle = if opts.ai_watch && ai_enabled {
        let aw_shutdown = shutdown.clone();
        let aw_rb = rolling_buffer.clone();
        let aw_tx = tx_ai_trigger.clone();
        Some(tokio::task::spawn_blocking(move || {
            // Auto-watch runs in the same thread as port_reader via the rolling buffer
            // For simplicity, we poll the buffer periodically
            let mut scanner = crate::ai::error_scanner::ErrorScanner::new();
            let mut last_len = 0usize;
            while !aw_shutdown.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(500));
                let snapshot = aw_rb.snapshot();
                // Check new lines since last poll
                for line in snapshot.iter().skip(last_len) {
                    if scanner.check(line) {
                        let _ = aw_tx.blocking_send(AiTrigger::AutoWatch);
                    }
                }
                last_len = snapshot.len();
            }
        }))
    } else {
        None
    };

    // Coordinate shutdown
    let result: Result<(), MadPuttyError> = tokio::select! {
        _ = rx_exit => Ok(()),
        reader_err = rx_reader_err => {
            match reader_err {
                Ok(err) => Err(err),
                Err(_) => Ok(()),
            }
        }
    };

    // Teardown
    shutdown.store(true, Ordering::Relaxed);
    let _ = reader_handle.await;
    let _ = forwarder_handle.await;
    let _ = writer_handle.await;
    let _ = status_handle.await;
    let ai_rlog = ai_handle.await.ok();
    if let Some(h) = autowatch_handle {
        let _ = h.await;
    }

    // Clear status line
    if !opts.plain {
        eprint!("\r\x1b[2K");
        let _ = std::io::stderr().flush();
    }

    if let Err(err) = &result {
        let red = console::Style::new().red().bold();
        eprintln!(
            "\n  {}  {}",
            red.apply_to("✗"),
            red.apply_to(format!("{err}"))
        );
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

    // AI response log notification
    if let Some(rlog) = &ai_rlog {
        if rlog.has_entries() {
            println!("AI responses saved to {}", rlog.path().display());
        }
    }

    result
}

/// Trigger types for the AI consumer task.
#[derive(Debug)]
enum AiTrigger {
    Analyze,
    #[allow(dead_code)]
    Question(String),
    AutoWatch,
}

/// AI consumer loop — receives triggers, snapshots buffer, redacts, invokes kiro-cli.
async fn ai_consumer_loop(
    ai: AiSubsystem,
    rb: RollingBuffer,
    rx: &mut mpsc::Receiver<AiTrigger>,
    response_log: &mut crate::ai::response_log::ResponseLog,
    no_redact: bool,
    shutdown: Arc<AtomicBool>,
) {
    let invoker = match ai.invoker {
        Some(inv) => inv,
        None => return, // AI disabled
    };
    let redactor = ai.redactor;

    while !shutdown.load(Ordering::Relaxed) {
        let trigger = tokio::select! {
            t = rx.recv() => match t {
                Some(t) => t,
                None => break,
            },
            _ = tokio::time::sleep(Duration::from_millis(100)) => continue,
        };

        // Snapshot + redact
        let snapshot = rb.snapshot();
        let context = snapshot.join("\n");
        let redacted = if no_redact {
            context
        } else {
            redactor.redact(&context)
        };

        // Build prompt
        let system_prompt = "You are a serial log analyst helping a firmware engineer. \
            Analyze these live serial logs and explain what is happening in plain English. \
            Call out errors, state transitions, and likely root causes. Be concise — 3 to 5 sentences. \
            If you see WiFi connection attempts, identify the security mode, SSID if present, \
            and whether the attempt succeeded or failed.";

        let (prompt, trigger_name, question) = match &trigger {
            AiTrigger::Analyze | AiTrigger::AutoWatch => {
                let name = match trigger {
                    AiTrigger::AutoWatch => "Auto-watch",
                    _ => "Ctrl+A A",
                };
                (format!("{system_prompt}\n\nLogs:\n{redacted}"), name, None)
            }
            AiTrigger::Question(q) => (
                format!("{q}\n\nLogs:\n{redacted}"),
                "Ctrl+A Q",
                Some(q.as_str()),
            ),
        };

        // Invoke kiro-cli
        match invoker.invoke(&prompt).await {
            Ok(response) => {
                // Print AI response below the log stream
                let yellow = "\x1b[33;1m";
                let reset = "\x1b[0m";
                let dim = "\x1b[2m";
                eprintln!();
                eprintln!("{yellow}─── 🤖 AI Analysis ───{reset}");
                for line in response.lines() {
                    eprintln!("{dim}{line}{reset}");
                }
                eprintln!("{yellow}───────────────────────{reset}");
                eprintln!();

                // Save to response log
                if let Err(e) = response_log.append(trigger_name, question, &response) {
                    tracing::warn!("AI response log write failed: {e}");
                }
            }
            Err(e) => {
                eprintln!("\x1b[31;1m⚠ {e}\x1b[0m");
            }
        }
    }
}

fn port_reader_loop(
    mut port: Box<dyn DuplexStream>,
    mut sink: StdoutSink,
    shutdown: Arc<AtomicBool>,
    err_tx: oneshot::Sender<MadPuttyError>,
    bytes_rx: Arc<AtomicU64>,
    palette: Palette,
    plain: bool,
    rolling_buffer: RollingBuffer,
) {
    let mut buf = [0u8; 4096];
    let mut colorizer = Colorizer::new(palette, !plain);
    let mut line_buf = String::new();

    while !shutdown.load(Ordering::Relaxed) {
        match port.read(&mut buf) {
            Ok(0) => continue,
            Ok(n) => {
                bytes_rx.fetch_add(n as u64, Ordering::Relaxed);

                // Log file (raw bytes)
                if let Some(log) = sink.log_mut() {
                    if let Err(e) = log.write_all(&buf[..n]) {
                        let _ = err_tx.send(MadPuttyError::PortIo(e));
                        return;
                    }
                }

                // Feed to rolling buffer (line-by-line)
                let text = String::from_utf8_lossy(&buf[..n]);
                line_buf.push_str(&text);
                while let Some(idx) = line_buf.find('\n') {
                    let line: String = line_buf.drain(..=idx).collect();
                    rolling_buffer.push(line.trim_end().to_string());
                }

                // Colorized stdout
                let mut stdout = std::io::stdout().lock();
                if let Err(e) = colorizer.feed(&buf[..n], &mut stdout) {
                    let _ = err_tx.send(MadPuttyError::PortIo(e));
                    return;
                }
            }
            Err(e) if e.kind() == ErrorKind::TimedOut => {
                // Flush partial lines
                if !line_buf.is_empty() {
                    rolling_buffer.push(std::mem::take(&mut line_buf));
                }
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
    mut port: Box<dyn DuplexStream>,
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
    ai_enabled: bool,
    tx_ai: mpsc::Sender<AiTrigger>,
) {
    let _raw = match RawModeGuard::new() {
        Ok(g) => g,
        Err(e) => {
            tracing::warn!("failed to enable raw mode: {e}");
            let _ = exit_tx.send(());
            return;
        }
    };

    let mut state = HotkeyDispatcher::new(ai_enabled);
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
            HotkeyAction::Forward(v) => {
                if echo {
                    let mut out = std::io::stdout().lock();
                    let _ = out.write_all(&v);
                    let _ = out.flush();
                }
                if tx_bytes.send(v).is_err() {
                    break;
                }
            }
            HotkeyAction::Exit => {
                if let Some(tx) = exit_tx.take() {
                    let _ = tx.send(());
                }
                break;
            }
            HotkeyAction::Analyze => {
                let _ = tx_ai.blocking_send(AiTrigger::Analyze);
            }
            HotkeyAction::AskQuestion => {
                // For now, trigger a default analysis (full question prompt requires
                // inline input which needs the split-pane UI — deferred to UI task)
                let _ = tx_ai.blocking_send(AiTrigger::Analyze);
            }
            HotkeyAction::ShowLastResponse => {
                // Deferred to split-pane UI task (needs modal overlay)
                eprintln!("\x1b[33m[AI] Last response view not yet implemented\x1b[0m");
            }
            HotkeyAction::Continue => {}
        }
    }

    drop(tx_bytes);
}

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
    tokio::time::sleep(Duration::from_millis(500)).await;

    while !shutdown.load(Ordering::Relaxed) {
        interval.tick().await;
        let rx = bytes_rx.load(Ordering::Relaxed);
        let tx = bytes_tx.load(Ordering::Relaxed);
        let rate = rx.saturating_sub(last_rx);
        last_rx = rx;
        let elapsed = started.elapsed();
        let elapsed_str = format_short_elapsed(elapsed);

        let bg = "\x1b[43m";
        let fg = "\x1b[30m";
        let reset = "\x1b[0m";
        let label = "\x1b[30;1m";
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

fn chrono_session_id() -> String {
    let now = std::time::SystemTime::now();
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}
