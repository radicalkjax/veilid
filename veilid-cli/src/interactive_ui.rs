use crate::command_processor::*;
use crate::cursive_ui::CursiveUI;
use crate::settings::*;
use crate::tools::*;
use crate::ui::*;

use console::{style, Term};
use flexi_logger::writers::LogWriter;
use flexi_logger::DeferredNow;
use futures_util::AsyncWriteExt as _;
use indent::indent_by;
use rustyline_async::SharedWriter;
use rustyline_async::{Readline, ReadlineError, ReadlineEvent};
use std::time::SystemTime;
use stop_token::future::FutureExt as StopTokenFutureExt;

pub type InteractiveUICallback = Box<dyn FnMut() + Send>;

pub struct InteractiveUIInner {
    cmdproc: Option<CommandProcessor>,
    stdout: Option<SharedWriter>,
    error: Option<String>,
    done: Option<StopSource>,
    connection_state_receiver: flume::Receiver<ConnectionState>,
    log_enabled: bool,
    enable_color: bool,
}

#[derive(Clone)]
pub struct InteractiveUI {
    inner: Arc<Mutex<InteractiveUIInner>>,
    _settings: Arc<Settings>,
}

impl InteractiveUI {
    pub fn new(settings: &Settings) -> (Self, InteractiveUISender) {
        let (cssender, csreceiver) = flume::unbounded::<ConnectionState>();

        let term = Term::stdout();
        let enable_color = console::colors_enabled() && term.features().colors_supported();

        // Create the UI object
        let this = Self {
            inner: Arc::new(Mutex::new(InteractiveUIInner {
                cmdproc: None,
                stdout: None,
                error: None,
                done: Some(StopSource::new()),
                connection_state_receiver: csreceiver,
                log_enabled: true,
                enable_color,
            })),
            _settings: Arc::new(settings.clone()),
        };

        let ui_sender = InteractiveUISender {
            inner: this.inner.clone(),
            connection_state_sender: cssender,
        };

        (this, ui_sender)
    }

    pub async fn command_loop(&self) {
        let (mut readline, mut stdout) =
            match Readline::new("> ".to_owned()).map_err(|e| e.to_string()) {
                Ok(v) => v,
                Err(e) => {
                    println!("Error: {:?}", e);
                    return;
                }
            };

        let (connection_state_receiver, done) = {
            let inner = self.inner.lock();
            (
                inner.connection_state_receiver.clone(),
                inner.done.as_ref().unwrap_or_log().token(),
            )
        };

        self.inner.lock().stdout = Some(stdout.clone());

        CursiveUI::set_start_time();

        let _ = stdout
            .write_all(
                format!(
                    r#" _   ____________   _______ 
| | / / __/  _/ /  /  _/ _ \
| |/ / _/_/ // /___/ // // /
|___/___/___/____/___/____/
--==/ {} {} /==--

"#,
                    env!("CARGO_PKG_NAME"),
                    env!("CARGO_PKG_VERSION")
                )
                .as_bytes(),
            )
            .await
            .map(drop);

        // Wait for connection to be established
        let done2 = done.clone();
        let self2 = self.clone();
        let mut stdout2 = stdout.clone();

        let connection_state_jh = spawn("connection state handler", async move {
            let mut prev_state = ConnectionState::Disconnected;
            loop {
                let next_state = match connection_state_receiver
                    .recv_async()
                    .timeout_at(done2.clone())
                    .await
                {
                    Ok(Ok(state)) => state,
                    Ok(Err(e)) => {
                        // Error getting connection state
                        eprintln!("Error: {:?}", e);
                        self2.inner.lock().done.take();
                        break;
                    }
                    Err(_) => {
                        // stop token
                        break;
                    }
                };
                let changed = prev_state.changed(&next_state);
                prev_state = next_state.clone();

                if !changed {
                    continue;
                }

                let res = match next_state {
                    ConnectionState::ConnectedTCP(_sa, st) => {
                        let tstr = st
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|n| human_timestamp(n.as_micros() as u64))
                            .unwrap_or_else(|_| "???".to_string());

                        stdout2
                            .write_all(format!("\r{}: Connected\n", tstr).as_bytes())
                            .await
                            .map(drop)
                    }
                    ConnectionState::ConnectedIPC(_pb, st) => {
                        let tstr = st
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|n| human_timestamp(n.as_micros() as u64))
                            .unwrap_or_else(|_| "???".to_string());

                        stdout2
                            .write_all(format!("\r{}: Connected\n", tstr).as_bytes())
                            .await
                            .map(drop)
                    }
                    ConnectionState::ConnectingTCP(sa, st) => {
                        let tstr = st
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|n| human_timestamp(n.as_micros() as u64))
                            .unwrap_or_else(|_| "???".to_string());
                        stdout2
                            .write_all(
                                format!("\r{}: Connecting to TCP({})\n", tstr, sa).as_bytes(),
                            )
                            .await
                            .map(drop)
                    }

                    ConnectionState::ConnectingIPC(pb, st) => {
                        let tstr = st
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|n| human_timestamp(n.as_micros() as u64))
                            .unwrap_or_else(|_| "???".to_string());
                        stdout2
                            .write_all(
                                format!(
                                    "\r{}: Connecting to IPC({})\n",
                                    tstr,
                                    pb.to_string_lossy()
                                )
                                .as_bytes(),
                            )
                            .await
                            .map(drop)
                    }
                    ConnectionState::Disconnected => {
                        let st = SystemTime::now();
                        let tstr = st
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|n| human_timestamp(n.as_micros() as u64))
                            .unwrap_or_else(|_| "???".to_string());

                        stdout2
                            .write_all(format!("\r{}: Disconnected\n", tstr).as_bytes())
                            .await
                            .map(drop)
                    }
                };

                if let Err(e) = res {
                    eprintln!("Error: {:?}", e);
                    self2.inner.lock().done.take();
                    break;
                }
            }
        });

        loop {
            if let Some(e) = self.inner.lock().error.clone() {
                eprintln!("Error: {:?}", e);
                break;
            }

            match readline.readline().timeout_at(done.clone()).await {
                Ok(Ok(ReadlineEvent::Line(line))) => {
                    let line = line.trim();

                    if !line.is_empty() {
                        readline.add_history_entry(line.to_string());
                    }

                    if line == "clear" {
                        if let Err(e) = readline.clear() {
                            eprintln!("Error: {:?}", e);
                        }
                    } else if line == "hide" {
                        self.inner.lock().log_enabled = false;
                    } else if line == "show" {
                        self.inner.lock().log_enabled = true;
                    } else if !line.is_empty() {
                        let (cmd, rest) = CommandProcessor::word_split(line);

                        let mut run_command = true;
                        if cmd == "help" {
                            let opt_help = if let Some(rest) = rest.as_deref() {
                                match rest {
                                    "clear" => {
                                        run_command = false;
                                        Some(
                                            r#"Clear the screen

Usage: clear
"#
                                            .to_string(),
                                        )
                                    }
                                    "show" => {
                                        run_command = false;
                                        Some(
                                        r#"Turn on viewing the log without changing the log level for the node

Usage: show
"#
                                        .to_string(),
                                    )
                                    }
                                    "hide" => {
                                        run_command = false;
                                        Some(
                                        r#"Turn off viewing the log without changing the log level for the node

Usage: hide
"#
                                        .to_string(),
                                    )
                                    }
                                    _ => None,
                                }
                            } else {
                                Some(
                                    r#"
Interactive Mode Commands:
  clear       Clear the screen
  show        Turn on viewing the log without changing the log level for the node
  hide        Turn off viewing the log without changing the log level for the node
"#
                                    .to_string(),
                                )
                            };

                            if let Some(opt_help) = opt_help {
                                if let Err(e) = stdout.write_all(opt_help.as_bytes()).await {
                                    eprintln!("Error: {:?}", e);
                                    break;
                                }
                            }
                        }

                        if run_command {
                            let cmdproc = self.inner.lock().cmdproc.clone();
                            if let Some(cmdproc) = &cmdproc {
                                if let Err(e) = cmdproc.run_command(
                                    line,
                                    UICallback::Interactive(Box::new({
                                        //let mut stdout = stdout.clone();
                                        move || {
                                            // if let Err(e) = writeln!(stdout) {
                                            //     eprintln!("Error: {:?}", e);
                                            // }
                                        }
                                    })),
                                ) {
                                    if let Err(e) =
                                        stdout.write_all(format!("Error: {}", e).as_bytes()).await
                                    {
                                        eprintln!("Error: {:?}", e);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(Ok(ReadlineEvent::Interrupted)) => {
                    break;
                }
                Ok(Ok(ReadlineEvent::Eof)) => {
                    break;
                }
                Ok(Err(ReadlineError::Closed)) => {}
                Ok(Err(ReadlineError::IO(e))) => {
                    eprintln!("IO Error: {:?}", e);
                    break;
                }
                Err(_) => {
                    break;
                }
            }
        }
        let _ = readline.flush();

        // Drop the stopper if we just broke out
        let _ = self.inner.lock().done.take();

        connection_state_jh.await;
    }
}

impl UI for InteractiveUI {
    fn set_command_processor(&mut self, cmdproc: CommandProcessor) {
        let mut inner = self.inner.lock();
        inner.cmdproc = Some(cmdproc);
    }
    fn run_async(&mut self) -> Pin<Box<dyn core::future::Future<Output = ()>>> {
        let this = self.clone();
        Box::pin(async move {
            this.command_loop().await;
        })
    }
}

//////////////////////////////////////////////////////////////////////////////

#[derive(Clone)]
pub struct InteractiveUISender {
    inner: Arc<Mutex<InteractiveUIInner>>,
    connection_state_sender: flume::Sender<ConnectionState>,
}

impl UISender for InteractiveUISender {
    fn clone_uisender(&self) -> Box<dyn UISender> {
        Box::new(InteractiveUISender {
            inner: self.inner.clone(),
            connection_state_sender: self.connection_state_sender.clone(),
        })
    }
    fn as_logwriter(&self) -> Option<Box<dyn LogWriter>> {
        Some(Box::new(self.clone()) as Box<dyn LogWriter>)
    }

    fn display_string_dialog(&self, title: &str, text: &str, close_cb: UICallback) {
        use std::io::Write;

        let Some(mut stdout) = self.inner.lock().stdout.clone() else {
            return;
        };
        if let Err(e) = writeln!(stdout, "{}: {}", title, text) {
            self.inner.lock().error = Some(e.to_string());
        }
        if let UICallback::Interactive(mut close_cb) = close_cb {
            close_cb()
        }
    }

    fn quit(&self) {
        self.inner.lock().done.take();
    }

    fn send_callback(&self, callback: UICallback) {
        if let UICallback::Interactive(mut callback) = callback {
            callback();
        }
    }
    fn set_attachment_state(
        &mut self,
        _state: &str,
        _public_internet_ready: bool,
        _local_network_ready: bool,
    ) {
        //
    }
    fn set_network_status(
        &mut self,
        _started: bool,
        _bps_down: u64,
        _bps_up: u64,
        mut _peers: Vec<json::JsonValue>,
    ) {
        //
    }
    fn set_config(&mut self, _config: &json::JsonValue) {
        //
    }
    fn set_connection_state(&mut self, state: ConnectionState) {
        if let Err(e) = self.connection_state_sender.send(state) {
            eprintln!("Error: {:?}", e);
            self.inner.lock().done.take();
        }
    }

    fn add_node_event(&self, _log_color: Level, event: &str) {
        use std::io::Write;

        let Some(mut stdout) = self.inner.lock().stdout.clone() else {
            return;
        };
        if let Err(e) = writeln!(stdout, "{}", event) {
            self.inner.lock().error = Some(e.to_string());
        }
    }
    fn add_log_event(&self, log_color: Level, event: &str) {
        use std::io::Write;

        let (enable_color, mut stdout) = {
            let inner = self.inner.lock();
            if !inner.log_enabled {
                return;
            }
            let Some(stdout) = inner.stdout.clone() else {
                return;
            };

            (inner.enable_color, stdout)
        };

        let log_line = format!(
            "{}: {}",
            CursiveUI::cli_ts(CursiveUI::get_start_time()),
            event
        );
        if enable_color {
            let log_line = match log_color {
                Level::Error => style(log_line).red().bright().to_string(),
                Level::Warn => style(log_line).yellow().bright().to_string(),
                Level::Info => log_line,
                Level::Debug => style(log_line).green().bright().to_string(),
                Level::Trace => style(log_line).blue().bright().to_string(),
            };
            if let Err(e) = writeln!(stdout, "{}", log_line) {
                eprintln!("Error: {:?}", e);
                self.inner.lock().done.take();
            }
        } else {
            println!("{}", log_line);
        }
    }
}

impl LogWriter for InteractiveUISender {
    fn write(&self, _now: &mut DeferredNow, record: &Record) -> std::io::Result<()> {
        let mut lines = String::new();
        let mut indent = 0;

        let ts_prefix = format!(
            "[veilid-cli] {} ",
            CursiveUI::cli_ts(CursiveUI::get_start_time()),
        );
        indent += ts_prefix.len();
        lines += &ts_prefix;

        let filestr = format!(
            "{}:{} ",
            record.file().unwrap_or("(unnamed)"),
            record.line().unwrap_or(0),
        );
        indent += filestr.len();
        lines += &filestr;

        let levstr = format!("{}: ", record.level());
        indent += levstr.len();
        lines += &levstr;

        let args = format!("{}", &record.args());
        lines += &indent_by(indent, args);

        print!("{}\r\n", lines);

        Ok(())
    }

    fn flush(&self) -> std::io::Result<()> {
        // we are not buffering
        Ok(())
    }

    fn max_log_level(&self) -> log::LevelFilter {
        log::LevelFilter::max()
    }
}
