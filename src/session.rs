//! Serial session with AI-powered log analysis via kiro-cli.
//!
//! Two independent lanes:
//! - Log pump lane: port_reader → colorizer → stdout (never blocks)
//! - AI subsystem lane: rolling_buffer → redactor → kiro_invoker → ai_pane

use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use serialport::ErrorKind as SerialErrorKind;
use tokio::sync::{mpsc, oneshot};

use crate::ai::pane::AiPaneState;
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
use crate::ui::split_pane::SplitPaneRenderer;

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
    pub no_split_pane: bool,
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

    // Redaction warning — show even if AI is not available so the user
    // knows the flag was acknowledged.
    if opts.no_redact {
        if ai_enabled {
            eprintln!("⚠ redaction disabled (--no-redact) — credentials may leak to kiro-cli");
        } else {
            eprintln!("⚠ --no-redact set but AI is not active (flag has no effect)");
        }
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

    // Renderer selection (restored to the original design):
    //   plain mode           → no renderer (raw writes, no status bar)
    //   AI enabled (default) → full split: log + static AI pane + status bar
    //   AI disabled / --no-split-pane → status-bar-only: pinned status, scrollback
    let split_pane: Option<Arc<Mutex<SplitPaneRenderer>>> = if opts.plain {
        None
    } else if ai_enabled && !opts.no_split_pane {
        let (w, h) = crossterm::terminal::size().unwrap_or((80, 24));
        let renderer = SplitPaneRenderer::new(w, h);
        if renderer.active {
            let _ = renderer.setup();
            Some(Arc::new(Mutex::new(renderer)))
        } else {
            // Terminal too small for split — fall back to status-bar-only.
            let renderer = SplitPaneRenderer::status_bar_only(w, h);
            if renderer.active {
                let _ = renderer.setup();
                Some(Arc::new(Mutex::new(renderer)))
            } else {
                None
            }
        }
    } else {
        // AI unavailable, or user passed --no-split-pane — pin status only.
        let (w, h) = crossterm::terminal::size().unwrap_or((80, 24));
        let renderer = SplitPaneRenderer::status_bar_only(w, h);
        if renderer.active {
            let _ = renderer.setup();
            Some(Arc::new(Mutex::new(renderer)))
        } else {
            None
        }
    };

    // AI pane state (shared between consumer and renderer)
    let ai_pane_state: Arc<Mutex<AiPaneState>> = Arc::new(Mutex::new(AiPaneState::new()));

    // Draw the initial AI pane immediately so the user sees the pinned
    // "press Ctrl+A A to run" hint from second zero.
    if let Some(ref sp) = split_pane {
        if let Ok(r) = sp.lock() {
            let ps = ai_pane_state.lock().unwrap();
            let _ = r.draw_ai_pane(&ps);
        }
    }

    // Spawn Port_Reader
    let shutdown_reader = shutdown.clone();
    let bytes_rx_reader = bytes_rx.clone();
    let palette_reader = if opts.plain {
        Palette::plain()
    } else {
        Palette::amazon()
    };
    let plain = opts.plain;
    let reader_split_pane = split_pane.clone();
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
            reader_split_pane,
        );
    });

    // Spawn Port_Writer
    let bytes_tx_counter = bytes_tx.clone();
    let writer_handle = tokio::spawn(port_writer_loop(port_write, rx_bytes, bytes_tx_counter));

    // Spawn Input_Forwarder
    let shutdown_forwarder = shutdown.clone();
    let echo = opts.echo;
    let tx_ai = tx_ai_trigger.clone();
    let forwarder_split_pane = split_pane.clone();
    let forwarder_pane_state = ai_pane_state.clone();
    let forwarder_handle = tokio::task::spawn_blocking(move || {
        input_forwarder_loop(
            tx_bytes,
            tx_exit,
            shutdown_forwarder,
            echo,
            ai_enabled,
            tx_ai,
            forwarder_split_pane,
            forwarder_pane_state,
        );
    });

    // Spawn status bar
    let status_shutdown = shutdown.clone();
    let status_bytes_rx = bytes_rx.clone();
    let status_bytes_tx = bytes_tx.clone();
    let status_port_name = port_name.to_string();
    let status_baud = config.baud;
    let status_plain = opts.plain;
    let status_split_pane = split_pane.clone();
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
            status_split_pane,
        )
        .await;
    });

    // Spawn AI analysis consumer task
    let ai_shutdown = shutdown.clone();
    let ai_rb = rolling_buffer.clone();
    let ai_no_redact = opts.no_redact;
    let consumer_split_pane = split_pane.clone();
    let consumer_pane_state = ai_pane_state.clone();
    let ai_handle: tokio::task::JoinHandle<crate::ai::response_log::ResponseLog> =
        tokio::spawn(async move {
            let mut rx = rx_ai_trigger;
            let mut rlog = response_log;
            ai_consumer_loop(
                ai,
                ai_rb,
                &mut rx,
                &mut rlog,
                ai_no_redact,
                ai_shutdown,
                consumer_split_pane,
                consumer_pane_state,
            )
            .await;
            rlog
        });

    // Spawn auto-watch error scanner (if enabled)
    let autowatch_handle = if opts.ai_watch && ai_enabled {
        let aw_shutdown = shutdown.clone();
        let aw_rb = rolling_buffer.clone();
        let aw_tx = tx_ai_trigger.clone();
        Some(tokio::task::spawn_blocking(move || {
            let mut scanner = crate::ai::error_scanner::ErrorScanner::new();
            let mut last_len = 0usize;
            while !aw_shutdown.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(500));
                let snapshot = aw_rb.snapshot();
                for line in snapshot.iter().skip(last_len) {
                    if scanner.check(line) {
                        // Use try_send to avoid blocking the scanner thread
                        let _ = aw_tx.try_send(AiTrigger::AutoWatch);
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

    // Teardown split-pane renderer
    if let Some(ref sp) = split_pane {
        if let Ok(r) = sp.lock() {
            let _ = r.teardown();
        }
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
#[allow(clippy::too_many_arguments)]
async fn ai_consumer_loop(
    ai: AiSubsystem,
    rb: RollingBuffer,
    rx: &mut mpsc::Receiver<AiTrigger>,
    response_log: &mut crate::ai::response_log::ResponseLog,
    no_redact: bool,
    shutdown: Arc<AtomicBool>,
    split_pane: Option<Arc<Mutex<SplitPaneRenderer>>>,
    pane_state: Arc<Mutex<AiPaneState>>,
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

        // Set spinner active
        {
            let mut ps = pane_state.lock().unwrap();
            ps.set_spinner(true);
        }
        if let Some(ref sp) = split_pane {
            if let Ok(r) = sp.lock() {
                let ps = pane_state.lock().unwrap();
                let _ = r.draw_ai_pane(&ps);
            }
        }

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

        // Invoke kiro-cli (with automatic retry for older versions without
        // --trust-all-tools support)
        match invoker.invoke_with_fallback(&prompt).await {
            Ok(response) => {
                // Update pane state
                let now = format_time_now();
                {
                    let mut ps = pane_state.lock().unwrap();
                    ps.set_response(response.clone(), now);
                }

                // Render via split-pane if available, otherwise fallback to eprintln
                if let Some(ref sp) = split_pane {
                    if let Ok(r) = sp.lock() {
                        let ps = pane_state.lock().unwrap();
                        let _ = r.draw_ai_pane(&ps);
                    }
                } else {
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
                }

                // Save to response log
                if let Err(e) = response_log.append(trigger_name, question, &response) {
                    tracing::warn!("AI response log write failed: {e}");
                }
            }
            Err(e) => {
                // Show the REAL error in the pane so the user can act on it.
                // kiro-cli error messages are what the user needs to see
                // (e.g. "API key required", "not logged in", "timeout").
                // Redaction already scrubbed the outgoing prompt, so
                // stderr is only kiro-cli's own diagnostics — safe to show.
                let display_msg = e.to_string();
                {
                    let mut ps = pane_state.lock().unwrap();
                    ps.set_error(display_msg.clone());
                }
                if let Some(ref sp) = split_pane {
                    if let Ok(r) = sp.lock() {
                        let ps = pane_state.lock().unwrap();
                        let _ = r.draw_ai_pane(&ps);
                    }
                } else {
                    eprintln!("\x1b[31;1m⚠ {display_msg}\x1b[0m");
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn port_reader_loop(
    mut port: Box<dyn DuplexStream>,
    mut sink: StdoutSink,
    shutdown: Arc<AtomicBool>,
    err_tx: oneshot::Sender<MadPuttyError>,
    bytes_rx: Arc<AtomicU64>,
    palette: Palette,
    plain: bool,
    rolling_buffer: RollingBuffer,
    split_pane: Option<Arc<Mutex<SplitPaneRenderer>>>,
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

                // Colorized output — route through split-pane renderer if active
                if let Some(ref sp) = split_pane {
                    // Colorize into a buffer, then write via renderer
                    let mut color_buf = Vec::new();
                    if let Err(e) = colorizer.feed(&buf[..n], &mut color_buf) {
                        let _ = err_tx.send(MadPuttyError::PortIo(e));
                        return;
                    }
                    if let Ok(r) = sp.lock() {
                        if let Err(e) = r.write_log(&color_buf) {
                            let _ = err_tx.send(MadPuttyError::PortIo(e));
                            return;
                        }
                    }
                } else {
                    let mut stdout = std::io::stdout().lock();
                    if let Err(e) = colorizer.feed(&buf[..n], &mut stdout) {
                        let _ = err_tx.send(MadPuttyError::PortIo(e));
                        return;
                    }
                }
            }
            Err(e) if e.kind() == ErrorKind::TimedOut => {
                // Flush partial lines
                if !line_buf.is_empty() {
                    rolling_buffer.push(std::mem::take(&mut line_buf));
                }
                if let Some(ref sp) = split_pane {
                    let mut color_buf = Vec::new();
                    let _ = colorizer.flush(&mut color_buf);
                    if let Ok(r) = sp.lock() {
                        let _ = r.write_log(&color_buf);
                    }
                } else {
                    let mut stdout = std::io::stdout().lock();
                    let _ = colorizer.flush(&mut stdout);
                }
                continue;
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => {
                let _ = err_tx.send(MadPuttyError::PortIo(e));
                return;
            }
        }
    }
    if let Some(ref sp) = split_pane {
        let mut color_buf = Vec::new();
        let _ = colorizer.flush(&mut color_buf);
        if let Ok(r) = sp.lock() {
            let _ = r.write_log(&color_buf);
        }
    } else {
        let mut stdout = std::io::stdout().lock();
        let _ = colorizer.flush(&mut stdout);
    }
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

#[allow(clippy::too_many_arguments)]
fn input_forwarder_loop(
    tx_bytes: mpsc::UnboundedSender<Vec<u8>>,
    exit_tx: oneshot::Sender<()>,
    shutdown: Arc<AtomicBool>,
    echo: bool,
    ai_enabled: bool,
    tx_ai: mpsc::Sender<AiTrigger>,
    split_pane: Option<Arc<Mutex<SplitPaneRenderer>>>,
    pane_state: Arc<Mutex<AiPaneState>>,
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

        // Handle resize events for split-pane
        if let Event::Resize(w, h) = evt {
            if let Some(ref sp) = split_pane {
                if let Ok(mut r) = sp.lock() {
                    let _ = r.on_resize(w, h);
                    // Redraw AI pane after resize
                    let ps = pane_state.lock().unwrap();
                    let _ = r.draw_ai_pane(&ps);
                }
            }
            continue;
        }

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
                // Print a visible acknowledgment so user knows the hotkey fired.
                eprintln!("\x1b[33;1m[AI] Analyzing recent logs...\x1b[0m");
                let _ = tx_ai.blocking_send(AiTrigger::Analyze);
            }
            HotkeyAction::AskQuestion => {
                // Custom question input requires a text input widget (deferred).
                // For now this acts the same as Analyze.
                eprintln!("\x1b[33;1m[AI] Analyzing (custom questions not yet wired)...\x1b[0m");
                let _ = tx_ai.blocking_send(AiTrigger::Analyze);
            }
            HotkeyAction::ShowLastResponse => {
                // Show last response in the AI pane via modal (if split-pane active)
                let ps = pane_state.lock().unwrap();
                if ps.has_response() {
                    if let Some(ref sp) = split_pane {
                        // Redraw the pane to show the full response
                        if let Ok(r) = sp.lock() {
                            let _ = r.draw_ai_pane(&ps);
                        }
                    } else {
                        // Fallback: print to stderr
                        let yellow = "\x1b[33;1m";
                        let reset = "\x1b[0m";
                        let dim = "\x1b[2m";
                        eprintln!();
                        eprintln!("{yellow}─── 🤖 Last AI Response ───{reset}");
                        for line in ps.body.lines() {
                            eprintln!("{dim}{line}{reset}");
                        }
                        eprintln!("{yellow}───────────────────────────{reset}");
                        eprintln!();
                    }
                } else {
                    eprintln!("\x1b[33m[AI] No AI response yet — press Ctrl+A A first\x1b[0m");
                }
            }
            HotkeyAction::Continue => {}
        }
    }

    drop(tx_bytes);
}

#[allow(clippy::too_many_arguments)]
async fn status_line_loop(
    shutdown: Arc<AtomicBool>,
    bytes_rx: Arc<AtomicU64>,
    bytes_tx: Arc<AtomicU64>,
    port: String,
    baud: u32,
    started: Instant,
    split_pane: Option<Arc<Mutex<SplitPaneRenderer>>>,
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
        let status_text = format!(
            "{bg}{fg} ▌ {label}PORT {fg}{port}  {label}BAUD {fg}{baud}  {label}UP {fg}{elapsed_str}  {label}RX {fg}{}  {label}TX {fg}{}  {label}RATE {fg}{}/s {reset}",
            humanize_bytes(rx),
            humanize_bytes(tx),
            humanize_bytes(rate),
        );

        // Route status bar through split-pane renderer if active
        if let Some(ref sp) = split_pane {
            if let Ok(r) = sp.lock() {
                let _ = r.draw_status_bar(&status_text);
            }
        } else {
            eprint!("\r\x1b[2K{status_text}");
            let _ = std::io::stderr().flush();
        }
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

fn format_time_now() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let hour = (secs / 3600) % 24;
    let min = (secs / 60) % 60;
    let sec = secs % 60;
    format!("{hour:02}:{min:02}:{sec:02}")
}
