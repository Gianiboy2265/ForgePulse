use std::path::PathBuf;

use chrono::{DateTime, Utc};
use forge_analysis::Incident;
use forge_core::{Result, metrics::MetricSnapshot};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalReport {
    pub generated_at: DateTime<Utc>,
    pub title: String,
    pub overview: String,
    pub snapshots: Vec<MetricSnapshot>,
    pub incidents: Vec<Incident>,
    pub limitations: Vec<String>,
    pub recommendations: Vec<String>,
    pub rollback_state: String,
}

#[derive(Debug, Clone, Default)]
pub struct AnonymizationContext {
    pub user_profile: Option<PathBuf>,
    pub machine_name: Option<String>,
    pub network_names: Vec<String>,
    pub ip_addresses: Vec<String>,
    pub serial_numbers: Vec<String>,
}

impl AnonymizationContext {
    #[must_use]
    pub fn anonymize_text(&self, value: &str) -> String {
        let mut result = value.to_owned();
        if let Some(profile) = &self.user_profile {
            result =
                replace_case_insensitive(&result, &profile.to_string_lossy(), "<user-profile>");
        }
        if let Some(machine_name) = &self.machine_name {
            result = replace_case_insensitive(&result, machine_name, "<machine>");
        }
        for name in &self.network_names {
            result = replace_case_insensitive(&result, name, "<network>");
        }
        for address in &self.ip_addresses {
            result = result.replace(address, "<ip-address>");
        }
        for serial in &self.serial_numbers {
            result = replace_case_insensitive(&result, serial, "<serial>");
        }
        result
    }

    pub fn anonymize_report(&self, report: &LocalReport) -> Result<LocalReport> {
        let serialized = serde_json::to_string(report)?;
        let anonymized = self.anonymize_text(&serialized);
        Ok(serde_json::from_str(&anonymized)?)
    }
}

pub fn to_json(report: &LocalReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}

#[must_use]
pub fn to_markdown(report: &LocalReport) -> String {
    let mut output = format!(
        "# {}\n\nGenerated: {}\n\n{}\n\n## Incidents\n\n",
        report.title, report.generated_at, report.overview
    );
    if report.incidents.is_empty() {
        output.push_str("No incidents were recorded.\n");
    } else {
        for incident in &report.incidents {
            output.push_str(&format!(
                "- {:?}: {} (confidence {:.0}%)\n",
                incident.severity,
                incident.summary,
                incident.confidence * 100.0
            ));
        }
    }
    output.push_str("\n## Limitations\n\n");
    for limitation in &report.limitations {
        output.push_str(&format!("- {limitation}\n"));
    }
    output.push_str("\n## Rollback state\n\n");
    output.push_str(&report.rollback_state);
    output.push('\n');
    output
}

#[must_use]
pub fn to_html(report: &LocalReport) -> String {
    let incidents = report
        .incidents
        .iter()
        .map(|incident| {
            format!(
                "<li><strong>{:?}</strong>: {} <span>{:.0}% confidence</span></li>",
                incident.severity,
                escape_html(&incident.summary),
                incident.confidence * 100.0
            )
        })
        .collect::<String>();
    format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>{}</title>\
         <style>body{{font:16px system-ui;background:#10131a;color:#e8edf6;max-width:960px;\
         margin:auto;padding:40px}}span{{color:#8fa3bf}}</style><body><h1>{}</h1><p>{}</p>\
         <h2>Incidents</h2><ul>{}</ul><h2>Rollback state</h2><p>{}</p></body></html>",
        escape_html(&report.title),
        escape_html(&report.title),
        escape_html(&report.overview),
        incidents,
        escape_html(&report.rollback_state),
    )
}

fn replace_case_insensitive(haystack: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return haystack.to_owned();
    }
    match regex::RegexBuilder::new(&regex::escape(needle))
        .case_insensitive(true)
        .build()
    {
        Ok(pattern) => pattern.replace_all(haystack, replacement).into_owned(),
        Err(_) => haystack.to_owned(),
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_case_insensitively() {
        let context = AnonymizationContext {
            machine_name: Some("My-PC".to_owned()),
            ..AnonymizationContext::default()
        };
        assert_eq!(
            context.anonymize_text("MY-PC and my-pc"),
            "<machine> and <machine>"
        );
    }
}
