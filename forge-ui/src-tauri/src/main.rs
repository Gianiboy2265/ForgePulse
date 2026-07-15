#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use forge_core::{
    config::ipc_key_path,
    ipc::{DoctorReport, RequestCommand, ResponsePayload, ServiceStatus},
    metrics::MetricSnapshot,
};
use forge_security::AuthenticatedClient;

#[tauri::command]
async fn service_status() -> Result<ServiceStatus, String> {
    match request(RequestCommand::Status).await? {
        ResponsePayload::Status(status) => Ok(status),
        response => Err(unexpected_response(&response)),
    }
}

#[tauri::command]
async fn metric_snapshot() -> Result<MetricSnapshot, String> {
    match request(RequestCommand::Snapshot).await? {
        ResponsePayload::Snapshot(snapshot) => Ok(snapshot),
        response => Err(unexpected_response(&response)),
    }
}

#[tauri::command]
async fn doctor_report() -> Result<DoctorReport, String> {
    match request(RequestCommand::Doctor).await? {
        ResponsePayload::Doctor(report) => Ok(report),
        response => Err(unexpected_response(&response)),
    }
}

async fn request(command: RequestCommand) -> Result<ResponsePayload, String> {
    let key_path = ipc_key_path().map_err(|error| error.to_string())?;
    let client =
        AuthenticatedClient::from_key_file(&key_path).map_err(|error| error.to_string())?;
    client
        .request(command)
        .await
        .map_err(|error| error.to_string())
}

fn unexpected_response(response: &ResponsePayload) -> String {
    match response {
        ResponsePayload::Error(error) => format!("{}: {}", error.code, error.message),
        _ => "the service returned an unexpected response type".to_owned(),
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            service_status,
            metric_snapshot,
            doctor_report
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| eprintln!("ForgePulse UI failed: {error}"));
}
