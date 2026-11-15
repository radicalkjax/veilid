use crate::logs::*;
use crate::server::*;
use crate::settings::Settings;
use crate::tools::*;
use crate::*;
use futures_util::StreamExt as _;
use signal_hook::consts::signal::*;
use stop_token::{future::FutureExt as StopFutureExt, StopSource, StopToken};
use veilid_core::tools::*;

#[cfg(feature = "rt-async-std")]
use signal_hook_async_std::Signals;
#[cfg(feature = "rt-tokio")]
use signal_hook_tokio::Signals;

async fn handle_signals(stop_token: StopToken) {
    let Ok(mut signals) = Signals::new([SIGHUP, SIGTERM, SIGINT, SIGQUIT]) else {
        return;
    };
    while let Ok(Some(signal)) = signals.next().timeout_at(stop_token.clone()).await {
        match signal {
            SIGHUP => {
                // XXX: reload configuration?
            }
            SIGTERM | SIGINT | SIGQUIT => {
                shutdown();
                break;
            }
            _ => {}
        }
    }
}

pub async fn run_veilid_server_with_signals(
    settings: Settings,
    server_mode: ServerMode,
    veilid_logs: Logs,
) -> EyreResult<()> {
    // Disarm SIGPIPE so writes to a broken pipe (e.g. `tee` exited on Ctrl-C)
    // return EPIPE instead of OS-terminating us
    let _ = signal_hook::flag::register(
        signal_hook::consts::signal::SIGPIPE,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );

    let stop_source = StopSource::new();
    let signals_task = spawn("signals", handle_signals(stop_source.token()));

    let res = run_veilid_server(settings, server_mode, veilid_logs).await;

    drop(stop_source);
    let _ = signals_task.await;

    res
}

#[instrument(level = "trace", skip_all, err)]
pub fn run_daemon(settings: Settings, _args: CmdlineArgs) -> EyreResult<()> {
    let daemon = {
        let mut daemon = daemonize::Daemonize::new();
        let s = settings.read();
        if let Some(pid_file) = s.daemon.pid_file.clone() {
            daemon = daemon.pid_file(pid_file.clone());
        }
        if let Some(chroot) = &s.daemon.chroot {
            daemon = daemon.chroot(chroot);
        }
        if let Some(working_directory) = &s.daemon.working_directory {
            daemon = daemon.working_directory(working_directory);
        }
        if let Some(user) = &s.daemon.user {
            daemon = daemon.user(user.as_str());
        }
        if let Some(group) = &s.daemon.group {
            daemon = daemon.group(group.as_str());
        }

        let stdout_file = if let Some(stdout_file) = &s.daemon.stdout_file {
            Some(std::fs::File::create(stdout_file).wrap_err("Failed to create stdio file")?)
        } else {
            None
        };
        if let Some(stderr_file) = &s.daemon.stderr_file {
            if Some(stderr_file) == s.daemon.stdout_file.as_ref() {
                // same output file for stderr and stdout
                daemon = daemon.stderr(
                    stdout_file
                        .as_ref()
                        .unwrap_or_log()
                        .try_clone()
                        .wrap_err("Failed to clone stdout file")?,
                );
            } else {
                daemon = daemon.stderr(
                    std::fs::File::create(stderr_file).wrap_err("Failed to create stderr file")?,
                );
            }
        }
        if let Some(stdout_file) = stdout_file {
            daemon = daemon.stdout(stdout_file);
        }

        daemon
    };

    // Daemonize
    daemon.start().wrap_err("Failed to daemonize")?;

    // Now, run the server
    block_on(async {
        // Init combined console/file logger
        let veilid_logs = Logs::setup(settings.clone())?;

        run_veilid_server_with_signals(settings, ServerMode::Normal, veilid_logs).await
    })
}
