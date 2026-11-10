//! `tortuga-cli check`: a project's mistakes and committed credentials, without sending
//! anything (R-CLI-5, R-CLI-6).

use tortuga_core::Collection;
use tortuga_core::check::Finding;
use tortuga_core::storage::FsStorage;
use tortuga_core::workspace::WorkspaceFile;

use crate::{Args, ReportKind};

/// Checks `args.project`, prints each finding and writes the report. `Ok(true)` when
/// there are none.
pub fn check(args: &Args) -> Result<bool, String> {
    let collection = Collection::open(&FsStorage, &args.project).map_err(|e| e.to_string())?;
    let workspace = match &args.workspace {
        None => WorkspaceFile::default(),
        Some(path) => crate::read_workspace(path)?,
    };
    let findings = tortuga_core::check::check(
        &FsStorage,
        &collection,
        &workspace,
        args.workspace.as_deref(),
    );
    for f in &findings {
        println!("{f}");
    }
    println!("{}", summary(collection.name(), &findings));
    if let Some((kind, out)) = &args.report {
        match kind {
            ReportKind::Json => {
                let text = serde_json::to_string_pretty(&serde_json::json!({
                    "project": collection.name(),
                    "findings": findings,
                }))
                .map_err(|e| e.to_string())?;
                std::fs::write(out, text).map_err(|e| format!("{}: {e}", out.display()))?;
            }
            ReportKind::Junit => return Err("check writes --report json only".into()),
        }
    }
    Ok(findings.is_empty())
}

fn summary(project: &str, findings: &[Finding]) -> String {
    match findings.len() {
        0 => format!("{project}: no findings"),
        1 => format!("{project}: 1 finding"),
        n => format!("{project}: {n} findings"),
    }
}
