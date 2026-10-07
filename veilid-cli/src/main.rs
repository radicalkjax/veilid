#![recursion_limit = "256"]

use crate::{tools::*, ui::*};

use clap::{Parser, ValueEnum};
use flexi_logger::*;
use std::path::PathBuf;

mod cached_text_view;
mod client_api_connection;
mod command_processor;
mod cursive_ui;
mod interactive_ui;
mod io_read_write_ui;
mod log_viewer_ui;
mod peers_table_view;
mod settings;
mod tools;
mod ui;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum LogLevel {
    /// Turn on debug logging
    Debug,
    /// Turn on trace logging
    Trace,
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Veilid Console Client")]
struct CmdlineArgs {
    /// IPC socket to connect to
    #[arg(long, short = 'p', group = "connection")]
    ipc_path: Option<PathBuf>,
    /// Subnode index to use when connecting to IPC socket
    #[arg(short('n'), long, group = "connection")]
    subnode_index: Option<u16>,
    /// Address to connect to
    #[arg(long, short = 'a', group = "connection")]
    address: Option<String>,
    /// Wait for debugger to attach
    #[arg(long)]
    wait_for_debug: bool,
    /// Specify a configuration file to use
    #[arg(short = 'c', long, value_name = "FILE")]
    config_file: Option<PathBuf>,
    /// Log level for the CLI itself (not for the Veilid node)
    #[arg(long, value_enum)]
    cli_log_level: Option<LogLevel>,
    /// Text-based user interface
    #[arg(
        long,
        short = 't',
        default_missing_value = "true",
        num_args(0..=1),
        group = "execution_mode"
    )]
    tui: Option<bool>,
    /// evaluate the rest of the line as a cli command
    #[arg(long, short = 'e', group = "execution_mode")]
    evaluate: Option<String>,
    /// show log only
    #[arg(long, short = 'l', group = "execution_mode")]
    log: bool,
    /// read commands from file
    #[arg(
        long,
        short = 'f',
        group = "execution_mode",
        value_name = "COMMAND_FILE"
    )]
    command_file: Option<PathBuf>,
}

fn main() -> Result<(), String> {
    // Start async
    block_on(async move {
        // Get command line options
        let default_config_path = settings::Settings::get_default_config_path();
        let args = CmdlineArgs::parse();

        if args.wait_for_debug {
            veilid_tools::wait_until_debugger_attached(None)
                .expect_or_log("debugger state not implemented on this platform");
        }

        // Attempt to load configuration
        let settings_path = args.config_file.unwrap_or(default_config_path);
        let settings_path = if settings_path.exists() {
            Some(settings_path.into_os_string())
        } else {
            None
        };

        let mut settings = settings::Settings::new(settings_path.as_deref())
            .map_err(|e| format!("configuration is invalid: {}", e))?;

        // Set config from command line
        if let Some(LogLevel::Debug) = args.cli_log_level {
            settings.logging.level = settings::LogLevel::Debug;
            settings.logging.terminal.enabled = true;
        }
        if let Some(LogLevel::Trace) = args.cli_log_level {
            settings.logging.level = settings::LogLevel::Trace;
            settings.logging.terminal.enabled = true;
        }

        // If we are running in interactive mode disable some things
        let mut enable_cursive = false;
        if args.tui.unwrap_or(settings.interface.tui) {
            // If we're using the cursive UI, don't print logs to the terminal
            settings.logging.terminal.enabled = false;
            enable_cursive = true;
        }

        // Create UI object
        let (mut ui, uisender) = if enable_cursive {
            let (ui, uisender) = cursive_ui::CursiveUI::new(&settings);
            (
                Box::new(ui) as Box<dyn UI>,
                Box::new(uisender) as Box<dyn UISender>,
            )
        } else if let Some(command_file) = args.command_file {
            cfg_if! {
                if #[cfg(feature="rt-async-std")] {
                    let (in_obj, out_obj) =
                        if command_file.to_string_lossy() == "-" {
                            (Box::pin(async_std::io::stdin()) as Pin<Box<dyn futures_util::AsyncRead + Send>>, async_std::io::stdout())
                        } else {
                            let f = match async_std::fs::File::open(command_file).await {
                                Ok(v) => v,
                                Err(e) => {
                                    return Err(e.to_string());
                                }
                            };
                            (Box::pin(f) as Pin<Box<dyn futures_util::AsyncRead + Send>>, async_std::io::stdout())
                        };
                } else if #[cfg(feature="rt-tokio")] {
                    use tokio_util::compat::{TokioAsyncWriteCompatExt, TokioAsyncReadCompatExt};
                    let (in_obj, out_obj) =
                        if command_file.to_string_lossy() == "-" {
                            (Box::pin(tokio::io::stdin().compat()) as Pin<Box<dyn futures_util::AsyncRead + Send>>, tokio::io::stdout().compat_write())
                        } else {
                            let f = match tokio::fs::File::open(command_file).await {
                                Ok(v) => v,
                                Err(e) => {
                                    return Err(e.to_string());
                                }
                            };
                            (Box::pin(f.compat()) as Pin<Box<dyn futures_util::AsyncRead + Send>>, tokio::io::stdout().compat_write())
                        };
                } else {
                    compile_error!("needs executor implementation");
                }
            }

            let (ui, uisender) = io_read_write_ui::IOReadWriteUI::new(&settings, in_obj, out_obj);
            (
                Box::new(ui) as Box<dyn UI>,
                Box::new(uisender) as Box<dyn UISender>,
            )
        } else if let Some(evaluate) = args.evaluate {
            cfg_if! {
                if #[cfg(feature="rt-async-std")] {
                    let in_str = format!("{}\n", evaluate);
                    let (in_obj, out_obj) = (futures_util::io::Cursor::new(in_str), async_std::io::stdout());
                } else if #[cfg(feature="rt-tokio")] {
                    use tokio_util::compat::{TokioAsyncWriteCompatExt};
                    let in_str = format!("{}\n", evaluate);
                    let (in_obj, out_obj) = (futures_util::io::Cursor::new(in_str), tokio::io::stdout().compat_write());
                } else {
                    compile_error!("needs executor implementation");
                }
            }

            let (ui, uisender) = io_read_write_ui::IOReadWriteUI::new(&settings, in_obj, out_obj);
            (
                Box::new(ui) as Box<dyn UI>,
                Box::new(uisender) as Box<dyn UISender>,
            )
        } else if args.log {
            let (ui, uisender) = log_viewer_ui::LogViewerUI::new(&settings);
            (
                Box::new(ui) as Box<dyn UI>,
                Box::new(uisender) as Box<dyn UISender>,
            )
        } else {
            let (ui, uisender) = interactive_ui::InteractiveUI::new(&settings);
            (
                Box::new(ui) as Box<dyn UI>,
                Box::new(uisender) as Box<dyn UISender>,
            )
        };

        // Set up loggers
        {
            let mut specbuilder = LogSpecBuilder::new();
            specbuilder.default(settings.logging.level.into());
            specbuilder.module("cursive", LevelFilter::Off);
            specbuilder.module("cursive_core", LevelFilter::Off);
            specbuilder.module("tokio_util", LevelFilter::Off);
            specbuilder.module("mio", LevelFilter::Off);
            specbuilder.module("async_std", LevelFilter::Off);
            specbuilder.module("async_io", LevelFilter::Off);
            specbuilder.module("polling", LevelFilter::Off);

            let logger = Logger::with(specbuilder.build());

            if settings.logging.terminal.enabled {
                if settings.logging.file.enabled {
                    std::fs::create_dir_all(settings.logging.file.directory.clone())
                        .map_err(map_to_string)?;
                    logger
                        .log_to_file_and_writer(
                            FileSpec::default()
                                .directory(settings.logging.file.directory.clone())
                                .suppress_timestamp(),
                            uisender.as_logwriter().unwrap_or_log(),
                        )
                        .o_append(settings.logging.file.append)
                        .start()
                        .expect_or_log("failed to initialize logger!");
                } else {
                    logger
                        .log_to_writer(uisender.as_logwriter().unwrap_or_log())
                        .start()
                        .expect_or_log("failed to initialize logger!");
                }
            } else if settings.logging.file.enabled {
                std::fs::create_dir_all(settings.logging.file.directory.clone())
                    .map_err(map_to_string)?;
                logger
                    .log_to_file(
                        FileSpec::default()
                            .directory(settings.logging.file.directory.clone())
                            .suppress_timestamp(),
                    )
                    .o_append(settings.logging.file.append)
                    .start()
                    .expect_or_log("failed to initialize logger!");
            }
        }

        // Get client address
        let mut client_api_ipc_path = None;
        let mut client_api_network_addresses = None;

        if args.ipc_path.is_some() {
            client_api_ipc_path = settings.resolve_ipc_path(args.ipc_path, None)?;
        } else if let Some(subnode_index) = args.subnode_index {
            client_api_ipc_path = settings.resolve_ipc_path(None, Some(subnode_index))?;
        } else if args.address.is_some() {
            client_api_network_addresses = settings.resolve_network_address(args.address, 0)?;
        } else {
            if let Ok(opt_ipc_path) = settings.resolve_ipc_path(None, None) {
                client_api_ipc_path = opt_ipc_path;
            }
            if client_api_ipc_path.is_none() {
                if let Ok(opt_network_address) = settings.resolve_network_address(None, 0) {
                    client_api_network_addresses = opt_network_address;
                }
            }
        }

        // Create command processor
        debug!("Creating Command Processor ");
        let comproc = command_processor::CommandProcessor::new(uisender, &settings);

        ui.set_command_processor(comproc.clone());

        // Create client api client side
        info!("Starting API connection");
        let capi = client_api_connection::ClientApiConnection::new(comproc.clone());

        // Save client api in command processor
        comproc.set_client_api_connection(capi.clone());

        // Keep a connection to the server
        if let Some(p) = client_api_ipc_path {
            comproc.set_ipc_path(p);
        } else if let Some(a) = client_api_network_addresses {
            comproc.set_network_address(a);
        }

        let comproc2 = comproc.clone();
        let connection_future = comproc.connection_manager();

        // Start UI
        let ui_future = async move {
            ui.run_async().await;

            // When UI quits, close connection and command processor cleanly
            comproc2.quit();
            capi.disconnect();
        };

        cfg_if! {
            if #[cfg(feature="rt-async-std")] {
                use async_std::prelude::*;
                // Wait for ui and connection to complete
                let _  = ui_future.join(connection_future).await;
            } else if #[cfg(feature="rt-tokio")] {
                // Wait for ui and connection to complete
                let _ = tokio::join!(ui_future, connection_future);
            } else {
                compile_error!("needs executor implementation");
            }
        }
        Ok(())
    })
}
