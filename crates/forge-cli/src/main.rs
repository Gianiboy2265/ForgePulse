use std::time::Duration;

use clap::{Parser, Subcommand};
use forge_core::{
    ForgeError, Result,
    config::ipc_key_path,
    ipc::{RequestCommand, ResponsePayload},
    metrics::MetricSnapshot,
};
use forge_security::AuthenticatedClient;
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(
    name = "forgepulse",
    version,
    about = "Local ForgePulse diagnostics client"
)]
struct Arguments {
    /// Print stable machine-readable JSON.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show background service and sampling health.
    Status,
    /// Show the latest process resource snapshot.
    Processes {
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u16).range(1..=1000))]
        limit: u16,
    },
    /// Stream live system snapshots until Ctrl+C.
    Monitor {
        #[arg(long, default_value_t = 2_000, value_parser = clap::value_parser!(u64).range(250..=60000))]
        interval_ms: u64,
    },
    /// Check IPC, collector, database, and rollback health.
    Doctor,
    /// Verify authenticated IPC connectivity.
    Ping,
}

#[tokio::main]
async fn main() -> Result<()> {
    let arguments = Arguments::parse();
    let client = AuthenticatedClient::from_key_file(&ipc_key_path()?)?;
    match arguments.command {
        Command::Status => {
            let response = client.request(RequestCommand::Status).await?;
            print_response(&response, arguments.json)
        }
        Command::Processes { limit } => {
            let response = client.request(RequestCommand::Processes { limit }).await?;
            print_response(&response, arguments.json)
        }
        Command::Monitor { interval_ms } => {
            monitor(&client, arguments.json, Duration::from_millis(interval_ms)).await
        }
        Command::Doctor => {
            let response = client.request(RequestCommand::Doctor).await?;
            print_response(&response, arguments.json)
        }
        Command::Ping => {
            let response = client.request(RequestCommand::Ping).await?;
            print_response(&response, arguments.json)
        }
    }
}

async fn monitor(client: &AuthenticatedClient, json: bool, cadence: Duration) -> Result<()> {
    let mut timer = tokio::time::interval(cadence);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = timer.tick() => {
                let response = client.request(RequestCommand::Snapshot).await?;
                if json {
                    print_json(&response)?;
                } else if let ResponsePayload::Snapshot(snapshot) = response {
                    print_snapshot_line(&snapshot);
                } else {
                    print_response(&response, false)?;
                }
            }
            signal = tokio::signal::ctrl_c() => {
                signal.map_err(|error| ForgeError::ServiceUnavailable(error.to_string()))?;
                return Ok(());
            }
        }
    }
}

fn print_response(response: &ResponsePayload, json: bool) -> Result<()> {
    if json {
        return print_json(response);
    }
    match response {
        ResponsePayload::Pong { service_time } => {
            println!("ForgePulse service responded at {service_time}")
        }
        ResponsePayload::Status(status) => {
            println!("ForgePulse service {}", status.service_version);
            println!("  Started: {}", status.started_at);
            println!(
                "  Latest sample: {}",
                status
                    .latest_sample_at
                    .map_or_else(|| "pending".to_owned(), |value| value.to_string())
            );
            println!(
                "  Samples: {} ({} dropped)",
                status.samples_collected, status.dropped_samples
            );
            println!("  Sampling interval: {} ms", status.sampling_interval_ms);
            println!(
                "  Database: {:.1} MiB",
                status.database_bytes as f64 / 1_048_576.0
            );
        }
        ResponsePayload::Snapshot(snapshot) => print_snapshot_line(snapshot),
        ResponsePayload::Processes(processes) => {
            println!("{:<7} {:>7} {:>10}  NAME", "PID", "CPU %", "RAM MiB");
            for process in processes {
                let cpu = process
                    .cpu_percent
                    .map_or_else(|| "--".to_owned(), |value| format!("{value:.1}"));
                println!(
                    "{:<7} {:>7} {:>10}  {}",
                    process.identity.pid,
                    cpu,
                    process.working_set_bytes.map_or_else(
                        || "--".to_owned(),
                        |value| format!("{:.1}", value as f64 / 1_048_576.0),
                    ),
                    process.executable_name
                );
            }
        }
        ResponsePayload::Doctor(report) => {
            println!(
                "Doctor: {}",
                if report.healthy {
                    "healthy"
                } else {
                    "attention required"
                }
            );
            for check in &report.checks {
                println!("  [{:?}] {}: {}", check.status, check.name, check.details);
            }
        }
        ResponsePayload::Error(error) => {
            return Err(ForgeError::ServiceUnavailable(format!(
                "{}: {}",
                error.code, error.message
            )));
        }
    }
    Ok(())
}

fn print_snapshot_line(snapshot: &MetricSnapshot) {
    let cpu = snapshot
        .cpu
        .total_percent
        .map_or_else(|| "warming up".to_owned(), |value| format!("{value:.1}%"));
    let used_memory = snapshot
        .memory
        .total_physical_bytes
        .saturating_sub(snapshot.memory.available_physical_bytes);
    println!(
        "{}  CPU {:>8}  RAM {:>5.1}/{:.1} GiB  processes {}  collection {:.2} ms",
        snapshot.captured_at.format("%H:%M:%S"),
        cpu,
        used_memory as f64 / 1_073_741_824.0,
        snapshot.memory.total_physical_bytes as f64 / 1_073_741_824.0,
        snapshot.processes.len(),
        snapshot.collection_duration_us as f64 / 1000.0
    );
}

fn print_json(value: &impl Serialize) -> Result<()> {
    println!("{}", serde_json::to_string(value)?);
    Ok(())
}
