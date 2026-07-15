use std::{ffi::OsString, sync::mpsc, time::Duration};

use clap::{Parser, Subcommand};
use forge_core::{ForgeError, Result, config::AppConfig};
use tokio::sync::watch;
use tracing_subscriber::EnvFilter;
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};

const SERVICE_NAME: &str = "ForgePulse";

#[derive(Debug, Parser)]
#[command(name = "forge-service", about = "ForgePulse local background service")]
struct Arguments {
    #[command(subcommand)]
    command: ServiceCommand,
}

#[derive(Debug, Subcommand)]
enum ServiceCommand {
    /// Run attached to the terminal for development and portable mode.
    Console,
    /// Enter the Windows Service Control Manager dispatcher.
    Service,
}

define_windows_service!(ffi_service_main, service_main);

fn main() -> Result<()> {
    initialize_logging()?;
    match Arguments::parse().command {
        ServiceCommand::Console => run_console(),
        ServiceCommand::Service => service_dispatcher::start(SERVICE_NAME, ffi_service_main)
            .map_err(|error| ForgeError::ServiceUnavailable(error.to_string())),
    }
}

fn initialize_logging() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .map_err(|error| ForgeError::ServiceUnavailable(error.to_string()))
}

fn run_console() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| ForgeError::ServiceUnavailable(error.to_string()))?;
    runtime.block_on(async {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let signal = tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                let _ignored = shutdown_tx.send(true);
            }
        });
        let result = forge_service::run(AppConfig::load_or_create()?, shutdown_rx).await;
        signal.abort();
        result
    })
}

fn service_main(_arguments: Vec<OsString>) {
    if let Err(error) = run_windows_service() {
        tracing::error!(%error, "Windows service stopped with an error");
    }
}

fn run_windows_service() -> Result<()> {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let status_handle =
        service_control_handler::register(SERVICE_NAME, move |control| match control {
            ServiceControl::Stop => {
                let _ignored = stop_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })
        .map_err(|error| ForgeError::ServiceUnavailable(error.to_string()))?;
    status_handle
        .set_service_status(service_status(
            ServiceState::Running,
            ServiceControlAccept::STOP,
        ))
        .map_err(|error| ForgeError::ServiceUnavailable(error.to_string()))?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| ForgeError::ServiceUnavailable(error.to_string()))?;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let _waiter = std::thread::Builder::new()
        .name("forge-service-stop".to_owned())
        .spawn(move || {
            if stop_rx.recv().is_ok() {
                let _ignored = shutdown_tx.send(true);
            }
        })
        .map_err(|error| ForgeError::ServiceUnavailable(error.to_string()))?;
    let result = runtime.block_on(forge_service::run(
        AppConfig::load_or_create()?,
        shutdown_rx,
    ));
    status_handle
        .set_service_status(service_status(
            ServiceState::Stopped,
            ServiceControlAccept::empty(),
        ))
        .map_err(|error| ForgeError::ServiceUnavailable(error.to_string()))?;
    result
}

fn service_status(state: ServiceState, accepted: ServiceControlAccept) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: accepted,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }
}
