use anyhow::Result;
use serde_json::json;

use crate::model::DependencyGraph;
use crate::policy::CompatibilityReport;

pub struct JsonReporter;

impl JsonReporter {
    pub fn render(report: &CompatibilityReport, graph: &DependencyGraph) -> Result<String> {
        let packages: Vec<_> = graph.all_packages().into_iter().collect();
        let payload = json!({
            "targetOutboundLicense": report.outbound_license,
            "status": report.status.label(),
            "summary": report.summary,
            "findings": report.findings,
            "obligations": report.obligations,
            "packages": packages,
        });

        Ok(serde_json::to_string_pretty(&payload)?)
    }
}
