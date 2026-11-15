use crate::client_api_connection::*;
use crate::settings::Settings;
use crate::tools::*;
use crate::ui::*;
use indent::indent_all_by;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::SystemTime;
use veilid_tools::*;

#[derive(PartialEq, Clone)]
pub enum ConnectionState {
    Disconnected,
    ConnectedTCP(SocketAddr, SystemTime),
    ConnectingTCP(SocketAddr, SystemTime),
    ConnectedIPC(PathBuf, SystemTime),
    ConnectingIPC(PathBuf, SystemTime),
}
impl ConnectionState {
    pub fn is_disconnected(&self) -> bool {
        matches!(*self, Self::Disconnected)
    }
    pub fn is_connected(&self) -> bool {
        matches!(*self, Self::ConnectedTCP(_, _) | Self::ConnectedIPC(_, _))
    }
    pub fn is_retrying(&self) -> bool {
        matches!(*self, Self::ConnectingTCP(_, _) | Self::ConnectingIPC(_, _))
    }
    pub fn changed(&self, other: &Self) -> bool {
        match (self, other) {
            (ConnectionState::Disconnected, ConnectionState::Disconnected) => false,
            (
                ConnectionState::ConnectingTCP(self_socket_addr, _),
                ConnectionState::ConnectingTCP(other_socket_addr, _),
            ) => self_socket_addr != other_socket_addr,
            (
                ConnectionState::ConnectedTCP(self_socket_addr, _),
                ConnectionState::ConnectedTCP(other_socket_addr, _),
            ) => self_socket_addr != other_socket_addr,
            (
                ConnectionState::ConnectingIPC(self_path_buf, _),
                ConnectionState::ConnectingIPC(other_path_buf, _),
            ) => self_path_buf != other_path_buf,
            (
                ConnectionState::ConnectedIPC(self_path_buf, _),
                ConnectionState::ConnectedIPC(other_path_buf, _),
            ) => self_path_buf != other_path_buf,
            _ => true,
        }
    }
}

struct CommandProcessorInner {
    ui_sender: Box<dyn UISender>,
    capi: Option<ClientApiConnection>,
    reconnect: bool,
    finished: bool,
    autoconnect: bool,
    autoreconnect: bool,
    ipc_path: Option<PathBuf>,
    network_addr: Option<SocketAddr>,
    connection_waker: Eventual,
    last_call_id: Option<u64>,
    enable_app_messages: bool,
}

#[derive(Clone)]
pub struct CommandProcessor {
    inner: Arc<Mutex<CommandProcessorInner>>,
    settings: Arc<Settings>,
}

impl CommandProcessor {
    pub fn new(ui_sender: Box<dyn UISender>, settings: &Settings) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CommandProcessorInner {
                ui_sender,
                capi: None,
                reconnect: settings.autoreconnect,
                finished: false,
                autoconnect: settings.autoconnect,
                autoreconnect: settings.autoreconnect,
                ipc_path: None,
                network_addr: None,
                connection_waker: Eventual::new(),
                last_call_id: None,
                enable_app_messages: false,
            })),
            settings: Arc::new(settings.clone()),
        }
    }
    pub fn set_client_api_connection(&self, capi: ClientApiConnection) {
        self.inner.lock().capi = Some(capi);
    }
    fn inner(&self) -> MutexGuard<'_, CommandProcessorInner> {
        self.inner.lock()
    }
    fn inner_mut(&self) -> MutexGuard<'_, CommandProcessorInner> {
        self.inner.lock()
    }
    fn ui_sender(&self) -> Box<dyn UISender> {
        self.inner.lock().ui_sender.clone_uisender()
    }
    fn capi(&self) -> ClientApiConnection {
        self.inner.lock().capi.as_ref().unwrap_or_log().clone()
    }

    pub fn word_split(line: &str) -> (String, Option<String>) {
        let trimmed = line.trim();
        if let Some(p) = trimmed.find(char::is_whitespace) {
            let first = trimmed[0..p].to_owned();
            let rest = trimmed[p..].trim_start().to_owned();
            (first, Some(rest))
        } else {
            (trimmed.to_owned(), None)
        }
    }

    pub fn cancel_command(&self) {
        trace!("CommandProcessor::cancel_command");
        let capi = self.capi();
        capi.cancel_all();
    }

    pub fn cmd_help(&self, rest: Option<String>, callback: UICallback) -> Result<(), String> {
        trace!("CommandProcessor::cmd_help");
        let capi = self.capi();
        let ui = self.ui_sender();
        let help_args = rest
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let help_command = help_args
            .as_deref()
            .map(|s| format!("help {}", s))
            .unwrap_or_else(|| "help".to_owned());

        let default_ipc_path = self
            .settings
            .ipc_path
            .as_ref()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_else(|| "[not set]".to_string());
        let default_address = self
            .settings
            .address
            .as_ref()
            .map(|x| x.name.clone())
            .unwrap_or_else(|| "[not set]".to_string());

        spawn_detached_local("cmd help", async move {
            if let Some(opt_rest) = help_args.as_deref() {
                let opt_help_out = match opt_rest {
                    "exit" | "quit" => Some(
                        r#"Exit the client (quit is also accepted)
                    
Usage: exit
       quit

Exit the client. Does not shut down the server.
'quit' is an alias for 'exit'."#.to_string(),
                    ),
                    "connect" => Some(format!(
                        r#"Connect to a Veilid node

Usage: connect [location]
                        
Arguments:
    [no argument]
        Connect to the default IPC path or network address specified in the configuration file
    [subnode_index]
        IPC path subnode index to connect to.
        The default subnode index is 0. 
    [ipc_path]
        A path to the IPC socket spawned by veilid-server, including the subnode index filename.
    [address][:port]
        An IP address or hostname and port number for the veilid-server,
        This is NOT used to connect to the veilid-server protocol port.
    
    The default IPC path is: {}/0
    The default network address is: {}

Examples:
    connect 0
    connect /var/run/veilid-server/ipc/0
    connect 127.0.0.1:5959
    connect [::1]:5959
"#,
                        default_ipc_path, default_address
                    )),
                    "disconnect" => Some(
                        r#"Disconnect the client from the Veilid node

Usage: disconnect

Does not shut down the server or quit the client.
"#.to_string(),
                    ),
                    "shutdown" => Some(
                        r#"Shut the server down

Usage: shutdown

The client will disconnect automatically.
Does not quit the client.
"#.to_string(),
                    ),
                    "log" => Some(
                        r#"Change the log level for a tracing layer

Usage: log [layer] <directives>

Arguments:
    <layer>
        One of: 'all', 'terminal', 'system', 'api', 'file', 'otlp'
        Default is 'api'
    <directives> 
        Directives are in RUST_LOG format and can use facility names or tags (groups) starting with '#'.
        The directives list may not have spaces in it.
        Common tags include:
            #veilid - all veilid log facility
            #common - the most commonly used debugging facilities
            #verbose - extra verbose logging facilities
            #enabled - the set of currently enabled log facilities
        Refer to the veilid-core source code for the full list of facilities and tags.
        If a log level is specifed without a full directive, the default facility of #enabled will be used.
Examples: 
    log debug
        - set the log level for all enabled log facilities on the api layer to 'debug'
    log all rpc=trace,#common=debug
        - sets the log level for the 'rpc' facility to 'trace', and
    log api #veilid=off
    log terminal #verbose=debug
    log #enabled=debug
"#.to_string(),
                    ),
                    "enable" | "disable" => Some(
                        r#"Set or unset a flag

Usage: enable [flag]
       disable [flag]

Arguments:
    [flag]
        Flag name for which to set the boolean value.
        Flags include: 
            app_messages
                Displays ALL app messages received by the server in the client output pane.
                This is off by default to avoid unauthenticated spam from remote nodes.
"#.to_string(),
                    ),
                    _ => None,
                };

                if let Some(help_out) = opt_help_out {
                    ui.add_node_event(Level::Info, &help_out);
                    ui.send_callback(callback);
                    return;
                }
            }

            let client_help = r#"Client Commands:
  exit        Exit the client (quit is also accepted)
  connect     Connect to a Veilid node
  disconnect  Disconnect the client from the Veilid node
  shutdown    Shut the server down
  log         Change the log level for a tracing layer
  enable      Set a flag
  disable     Unset a flag"#
                .to_string();

            let (out, level) = match capi.server_debug(help_command).await {
                Err(ServerDebugError::NotConnected) => (
                    format!("{}\nServer is not connected", client_help),
                    Level::Info,
                ),
                Err(ServerDebugError::Error(e)) => {
                    (format!("Error getting server help: {}", e), Level::Error)
                }
                Ok(v) => (
                    if help_args.is_some() {
                        v
                    } else {
                        format!("{}\nServer Commands:\n{}", client_help, indent_all_by(2, v))
                    },
                    Level::Info,
                ),
            };

            ui.add_node_event(level, &out);
            ui.send_callback(callback);
        });
        Ok(())
    }

    pub fn cmd_exit(&self, callback: UICallback) -> Result<(), String> {
        trace!("CommandProcessor::cmd_exit");
        let ui = self.ui_sender();
        ui.send_callback(callback);
        ui.quit();
        Ok(())
    }

    pub fn cmd_shutdown(&self, callback: UICallback) -> Result<(), String> {
        trace!("CommandProcessor::cmd_shutdown");
        let capi = self.capi();
        let ui = self.ui_sender();
        spawn_detached_local("cmd shutdown", async move {
            if let Err(e) = capi.server_shutdown().await {
                error!("Server command 'shutdown' failed to execute: {}", e);
            }
            ui.send_callback(callback);
        });
        Ok(())
    }

    pub fn cmd_disconnect(&self, callback: UICallback) -> Result<(), String> {
        trace!("CommandProcessor::cmd_disconnect");
        let capi = self.capi();
        let ui = self.ui_sender();
        spawn_detached_local("cmd disconnect", async move {
            capi.disconnect();
            ui.send_callback(callback);
        });
        Ok(())
    }

    pub fn cmd_connect(&self, rest: Option<String>, callback: UICallback) -> Result<(), String> {
        trace!("CommandProcessor::cmd_connect");
        let capi = self.capi();
        let ui = self.ui_sender();

        let this = self.clone();
        spawn_detached_local("cmd connect", async move {
            capi.disconnect();

            if let Some(Ok(subnode_index)) = rest.as_ref().map(|r| u16::from_str(r.as_str())) {
                match this
                    .settings
                    .resolve_ipc_path(this.get_ipc_path(), Some(subnode_index))
                {
                    Ok(Some(ipc_path)) => {
                        this.set_ipc_path(ipc_path);
                    }
                    Ok(None) => {
                        if let Some(r) = rest {
                            ui.add_node_event(
                                Level::Error,
                                &format!("No IPC path found for {}", r),
                            );
                        }
                    }
                    Err(e) => {
                        ui.add_node_event(Level::Error, &e);
                    }
                }
            } else if let Ok(ipc_path) = this
                .settings
                .resolve_ipc_path(rest.clone().map(|r| r.into()), None)
            {
                if let Some(ipc_path) = ipc_path {
                    this.set_ipc_path(ipc_path);
                } else if let Some(r) = rest {
                    ui.add_node_event(Level::Error, &format!("No IPC path found for {}", r));
                }
            } else if let Ok(network_address) =
                this.settings.resolve_network_address(rest.clone(), 0)
            {
                if let Some(addr) = network_address {
                    this.set_network_address(addr);
                } else if let Some(r) = rest {
                    ui.add_node_event(Level::Error, &format!("Invalid network address: {}", r));
                }
            } else if let Some(r) = rest {
                ui.add_node_event(Level::Error, &format!("Invalid connection string: {}", r));
            }

            this.start_connection();
            ui.send_callback(callback);
        });

        Ok(())
    }

    pub fn cmd_debug(&self, command_line: String, callback: UICallback) -> Result<(), String> {
        trace!("CommandProcessor::cmd_debug");
        let capi = self.capi();
        let ui = self.ui_sender();
        spawn_detached_local("cmd debug", async move {
            match capi.server_debug(command_line).await {
                Ok(output) => {
                    ui.add_node_event(Level::Info, &output);
                    ui.send_callback(callback);
                }
                Err(ServerDebugError::NotConnected) => {
                    ui.add_node_event(
                        Level::Warn,
                        "Server is not connected. Use 'connect' to connect to a server.",
                    );
                    ui.send_callback(callback);
                }
                Err(ServerDebugError::Error(e)) => {
                    ui.add_node_event(Level::Error, &e);
                    ui.send_callback(callback);
                }
            }
        });
        Ok(())
    }

    pub fn cmd_change_log_level(
        &self,
        rest: Option<String>,
        callback: UICallback,
    ) -> Result<(), String> {
        trace!("CommandProcessor::cmd_change_log_level");
        let capi = self.capi();
        let ui = self.ui_sender();
        spawn_detached_local("cmd change_log_level", async move {
            let (first, rest) = Self::word_split(&rest.unwrap_or_default());

            // Apply default layer if not specified
            let (layer, directives) = if let Some(rest) = rest {
                (first, rest)
            } else {
                ("api".to_string(), first)
            };

            // Apply directive shortcuts
            let directives = match directives.to_ascii_lowercase().as_str() {
                "off" => "#enabled=off".to_string(),
                "trace" => "#unspecified=trace".to_string(),
                "debug" => "#unspecified=debug".to_string(),
                "info" => "#unspecified=info".to_string(),
                "warn" => "#unspecified=warn".to_string(),
                "error" => "#unspecified=error".to_string(),
                d => d.to_string(),
            };

            match capi.server_change_log_level(layer, directives).await {
                Ok(()) => {
                    ui.add_node_event(Level::Info, "Applied log directives");
                    ui.send_callback(callback);
                }
                Err(e) => {
                    ui.add_node_event(Level::Error, &e);
                    ui.send_callback(callback);
                }
            }
        });
        Ok(())
    }

    pub fn cmd_change_log_ignore(
        &self,
        rest: Option<String>,
        callback: UICallback,
    ) -> Result<(), String> {
        trace!("CommandProcessor::cmd_change_log_ignore");
        let capi = self.capi();
        let ui = self.ui_sender();
        spawn_detached_local("cmd change_log_ignoe", async move {
            let (layer, rest) = Self::word_split(&rest.unwrap_or_default());
            let log_ignore = rest.unwrap_or_default();

            match capi
                .server_change_log_ignore(layer, log_ignore.clone())
                .await
            {
                Ok(()) => {
                    ui.add_node_event(Level::Info, "Applied log ignore");
                    ui.send_callback(callback);
                }
                Err(e) => {
                    ui.add_node_event(Level::Error, &e);
                    ui.send_callback(callback);
                }
            }
        });
        Ok(())
    }

    pub fn cmd_enable(&self, rest: Option<String>, callback: UICallback) -> Result<(), String> {
        trace!("CommandProcessor::cmd_enable");

        let ui = self.ui_sender();
        let this = self.clone();
        spawn_detached_local("cmd enable", async move {
            let flag = rest.clone().unwrap_or_default();
            match flag.as_str() {
                "app_messages" => {
                    this.inner.lock().enable_app_messages = true;
                    ui.add_node_event(Level::Info, &format!("flag enabled: {}", flag));
                    ui.send_callback(callback);
                }
                _ => {
                    ui.add_node_event(Level::Error, &format!("unknown flag: {}", flag));
                    ui.send_callback(callback);
                }
            }
        });
        Ok(())
    }

    pub fn cmd_disable(&self, rest: Option<String>, callback: UICallback) -> Result<(), String> {
        trace!("CommandProcessor::cmd_disable");

        let ui = self.ui_sender();
        let this = self.clone();
        spawn_detached_local("cmd disable", async move {
            let flag = rest.clone().unwrap_or_default();
            match flag.as_str() {
                "app_messages" => {
                    this.inner.lock().enable_app_messages = false;
                    ui.add_node_event(Level::Info, &format!("flag disabled: {}", flag));
                    ui.send_callback(callback);
                }
                _ => {
                    ui.add_node_event(Level::Error, &format!("unknown flag: {}", flag));
                    ui.send_callback(callback);
                }
            }
        });
        Ok(())
    }

    pub fn run_command(&self, command_line: &str, callback: UICallback) -> Result<(), String> {
        //
        let (cmd, rest) = Self::word_split(command_line);
        match cmd.as_str() {
            "help" => self.cmd_help(rest, callback),
            "exit" => self.cmd_exit(callback),
            "quit" => self.cmd_exit(callback),
            "disconnect" => self.cmd_disconnect(callback),
            "connect" => self.cmd_connect(rest, callback),
            "shutdown" => self.cmd_shutdown(callback),
            "log" | "change_log_level" => self.cmd_change_log_level(rest, callback),
            "change_log_ignore" => self.cmd_change_log_ignore(rest, callback),
            "enable" => self.cmd_enable(rest, callback),
            "disable" => self.cmd_disable(rest, callback),
            _ => self.cmd_debug(command_line.to_owned(), callback),
        }
    }

    async fn connect_ipc_and_run(&self, first: bool, ipc_path: PathBuf) -> bool {
        // IPC
        if first {
            info!("Connecting to server at {}", ipc_path.to_string_lossy());
            self.set_connection_state(ConnectionState::ConnectingIPC(
                ipc_path.clone(),
                SystemTime::now(),
            ));
        } else {
            debug!("Retrying connection to {}", ipc_path.to_string_lossy());
        }
        let capi = self.capi();

        // Connect to the Client API IPC socket and run the connection if successsful
        let res = capi.handle_ipc_connection(ipc_path.clone()).await;

        // Connection finished
        match res {
            Ok(()) => {
                info!(
                    "Connection to server at {} terminated normally",
                    ipc_path.to_string_lossy()
                );
                return false;
            }
            Err(ClientApiConnectionError::ConnectionLost(e)) => {
                info!("Connection to server lost: {}", e);
                if !self.inner().autoreconnect {
                    return false;
                }
            }
            Err(ClientApiConnectionError::ConnectionFailed(e)) => {
                info!("Connection to server failed: {}", e);
                if !self.inner().autoreconnect {
                    return false;
                }
            }
        }

        self.set_connection_state(ConnectionState::ConnectingIPC(ipc_path, SystemTime::now()));

        true
    }

    async fn connect_network_and_run(&self, first: bool, network_addr: SocketAddr) -> bool {
        // TCP
        if first {
            info!("Connecting to server at {}", network_addr);
            self.set_connection_state(ConnectionState::ConnectingTCP(
                network_addr,
                SystemTime::now(),
            ));
        } else {
            debug!("Retrying connection to {}", network_addr);
        }
        let capi = self.capi();

        // Connect to the Client API TCP socket and run the connection if successsful
        let res = capi.handle_tcp_connection(network_addr).await;

        // Connection finished
        match res {
            Ok(()) => {
                info!(
                    "Connection to server at {} terminated normally",
                    network_addr
                );
                return false;
            }
            Err(ClientApiConnectionError::ConnectionLost(e)) => {
                if !self.inner().autoreconnect {
                    info!("Connection to server lost: {}", e);
                    return false;
                }
            }
            Err(ClientApiConnectionError::ConnectionFailed(e)) => {
                info!("Connection to server failed: {}", e);
                return false;
            }
        }

        self.set_connection_state(ConnectionState::ConnectingTCP(
            network_addr,
            SystemTime::now(),
        ));

        true
    }

    pub async fn connection_manager(&self) {
        // Connect until we're done
        while !self.inner_mut().finished {
            // Wait for connection request
            if !self.inner().autoconnect {
                let waker = self.inner_mut().connection_waker.instance_clone(());
                waker.await;
            } else {
                self.inner_mut().autoconnect = false;
            }
            self.inner_mut().connection_waker.reset();
            // Loop while we want to keep the connection
            let mut first = true;
            while self.inner().reconnect {
                let (ipc_path_opt, network_addr_opt) = {
                    let inner = self.inner();
                    (inner.ipc_path.clone(), inner.network_addr)
                };

                if let Some(ipc_path) = ipc_path_opt {
                    // Try the IPC socket if enabled
                    if !self.connect_ipc_and_run(first, ipc_path).await {
                        break;
                    }
                } else if let Some(network_addr) = network_addr_opt {
                    // Try the TCP socket if enabled
                    if !self.connect_network_and_run(first, network_addr).await {
                        break;
                    }
                } else {
                    // No connection to try, just bail out of this for now
                    break;
                }

                debug!("Connection lost, retrying in 2 seconds");
                {
                    let waker = self.inner_mut().connection_waker.instance_clone(());
                    let _ = timeout(2000, waker).await;
                }
                self.inner_mut().connection_waker.reset();
                first = false;
            }
            info!("Disconnected.");
            self.set_connection_state(ConnectionState::Disconnected);
            self.inner_mut().reconnect = true;
        }
    }

    // called by ui
    ////////////////////////////////////////////
    pub fn set_ipc_path(&self, ipc_path: PathBuf) {
        let mut inner = self.inner_mut();
        inner.ipc_path = Some(ipc_path);
        inner.network_addr = None;
    }
    pub fn set_network_address(&self, network_addr: SocketAddr) {
        let mut inner = self.inner_mut();
        inner.ipc_path = None;
        inner.network_addr = Some(network_addr);
    }
    pub fn get_ipc_path(&self) -> Option<PathBuf> {
        self.inner().ipc_path.clone()
    }
    pub fn get_network_address(&self) -> Option<SocketAddr> {
        self.inner().network_addr
    }

    // called by client_api_connection
    // calls into ui
    ////////////////////////////////////////////

    pub fn log_message(&self, log_level: Level, message: &str) {
        self.inner().ui_sender.add_log_event(log_level, message);
    }

    pub fn update_attachment(&self, attachment: &json::JsonValue) {
        self.inner_mut().ui_sender.set_attachment_state(
            attachment["state"].as_str().unwrap_or_default(),
            attachment["public_internet_ready"]
                .as_bool()
                .unwrap_or_default(),
            attachment["local_network_ready"]
                .as_bool()
                .unwrap_or_default(),
        );
    }

    pub fn update_network_status(&self, network: &json::JsonValue) {
        self.inner_mut().ui_sender.set_network_status(
            network["started"].as_bool().unwrap_or_default(),
            json_str_u64(&network["bps_down"]),
            json_str_u64(&network["bps_up"]),
            network["peers"]
                .members()
                .cloned()
                .collect::<Vec<json::JsonValue>>(),
        );
    }
    pub fn update_config(&self, config: &json::JsonValue) {
        self.inner_mut().ui_sender.set_config(&config["config"])
    }
    pub fn update_route(&self, route: &json::JsonValue) {
        let mut out = String::new();
        if !route["dead_routes"].is_empty() {
            out.push_str(&format!("Dead routes: {:?}", route["dead_routes"]));
        }
        if !route["dead_routes"].is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!(
                "Dead remote routes: {:?}",
                route["dead_remote_routes"]
            ));
        }
        if !out.is_empty() {
            self.inner().ui_sender.add_node_event(Level::Info, &out);
        }
    }
    pub fn update_value_change(&self, value_change: &json::JsonValue) {
        let data = json_str_vec_u8(&value_change["value"]["data"]);
        let (datastr, truncated) = Self::print_json_str_vec_u8(&data);

        let out = format!(
            "Value change: key={} subkeys={} count={} value.seq={} value.writer={} value.data={}{}",
            value_change["key"].dump(),
            value_change["subkeys"].dump(),
            value_change["count"].dump(),
            value_change["value"]["seq"].dump(),
            value_change["value"]["writer"].dump(),
            datastr,
            if truncated { "..." } else { "" }
        );
        self.inner().ui_sender.add_node_event(Level::Info, &out);
    }

    pub fn update_log(&self, log: &json::JsonValue) {
        let log_level =
            Level::from_str(log["log_level"].as_str().unwrap_or("error")).unwrap_or(Level::Error);
        self.inner().ui_sender.add_log_event(
            log_level,
            &format!(
                "{}: {}{}",
                log["log_level"].as_str().unwrap_or("???"),
                log["message"].as_str().unwrap_or("???"),
                if let Some(bt) = log["backtrace"].as_str() {
                    format!("\nBacktrace:\n{}", bt)
                } else {
                    "".to_owned()
                }
            ),
        );
    }

    fn print_json_str_vec_u8(message: &[u8]) -> (String, bool) {
        // check if message body is ascii printable
        let mut printable = true;
        for c in message {
            if *c < 32 || *c > 126 {
                printable = false;
            }
        }

        let (message, truncated) = if message.len() > 64 {
            (&message[0..64], true)
        } else {
            (message, false)
        };

        let strmsg = if printable {
            format!("\"{}\"", String::from_utf8_lossy(message))
        } else {
            hex::encode(message)
        };

        (strmsg, truncated)
    }

    pub fn update_app_message(&self, msg: &json::JsonValue) {
        if !self.inner.lock().enable_app_messages {
            return;
        }

        let message = json_str_vec_u8(&msg["message"]);
        let (strmsg, truncated) = Self::print_json_str_vec_u8(&message);

        self.inner().ui_sender.add_node_event(
            Level::Info,
            &format!(
                "AppMessage ({:?}): {}{}",
                msg["sender"],
                strmsg,
                if truncated { "..." } else { "" }
            ),
        );
    }

    pub fn update_app_call(&self, call: &json::JsonValue) {
        if !self.inner.lock().enable_app_messages {
            return;
        }

        let message = json_str_vec_u8(&call["message"]);

        // check if message body is ascii printable
        let mut printable = true;
        for c in &message {
            if *c < 32 || *c > 126 {
                printable = false;
            }
        }

        let (message, truncated) = if message.len() > 64 {
            (&message[0..64], true)
        } else {
            (&message[..], false)
        };

        let strmsg = if printable {
            format!("\"{}\"", String::from_utf8_lossy(message))
        } else {
            hex::encode(message)
        };

        let id = json_str_u64(&call["call_id"]);

        self.inner().ui_sender.add_node_event(
            Level::Info,
            &format!(
                "AppCall ({:?}) id = {:016x} : {}{}",
                call["sender"],
                id,
                strmsg,
                if truncated { "..." } else { "" }
            ),
        );

        self.inner_mut().last_call_id = Some(id);
    }

    pub fn update_shutdown(&self) {
        // Do nothing with this, we'll process shutdown when rpc connection closes
    }

    // called by client_api_connection
    // calls into ui
    ////////////////////////////////////////////
    pub fn set_connection_state(&self, state: ConnectionState) {
        self.inner_mut().ui_sender.set_connection_state(state);
    }
    // called by ui
    ////////////////////////////////////////////
    pub fn start_connection(&self) {
        self.inner_mut().reconnect = true;
        drop(self.inner_mut().connection_waker.resolve());
    }
    // pub fn stop_connection(&self) {
    //     self.inner_mut().reconnect = false;
    //     let mut capi = self.capi().clone();
    //     spawn_detached(async move {
    //         capi.disconnect().await;
    //     });
    // }
    pub fn cancel_reconnect(&self) {
        self.inner_mut().reconnect = false;
        drop(self.inner_mut().connection_waker.resolve());
    }
    pub fn quit(&self) {
        self.inner_mut().finished = true;
        self.inner_mut().reconnect = false;
        drop(self.inner_mut().connection_waker.resolve());
    }

    // called by ui
    // calls into client_api_connection
    ////////////////////////////////////////////
    pub fn attach(&self) {
        let capi = self.capi();

        spawn_detached_local("attach", async move {
            if let Err(e) = capi.server_attach().await {
                error!("Server command 'attach' failed to execute: {}", e);
            }
        });
    }

    pub fn detach(&self) {
        let capi = self.capi();

        spawn_detached_local("detach", async move {
            if let Err(e) = capi.server_detach().await {
                error!("Server command 'detach' failed to execute: {}", e);
            }
        });
    }
}
