//! `tortuga-cli diff`: what changed in a project's API between two git revisions, as a
//! Markdown summary for a pull request (R-CLI-7, D77).

mod revision;

use std::path::Path;

use tortuga_core::storage::{FsStorage, Storage};
use tortuga_core::{Collection, Error, api_diff};

use crate::{Args, BreakingChanges, ReportKind};

/// The revisions compared.
#[derive(Debug, PartialEq, Eq)]
enum Range {
    /// `<rev>`: that revision and the working tree.
    Worktree(String),
    /// `<from>..<to>`.
    Between(String, String),
    /// `<from>...<to>`: their merge base and `to`, as a pull request compares.
    MergeBase(String, String),
}

impl Range {
    fn parse(text: &str) -> Result<Self, String> {
        let side = |s: &str| {
            if s.is_empty() {
                "HEAD".to_owned()
            } else {
                s.to_owned()
            }
        };
        if let Some((a, b)) = text.split_once("...") {
            Ok(Self::MergeBase(side(a), side(b)))
        } else if let Some((a, b)) = text.split_once("..") {
            Ok(Self::Between(side(a), side(b)))
        } else if text.trim().is_empty() {
            Err("which revision? e.g. main, main..HEAD or origin/main...HEAD".into())
        } else {
            Ok(Self::Worktree(text.to_owned()))
        }
    }

    /// As the summary's heading names it.
    fn label(&self) -> String {
        match self {
            Self::Worktree(rev) => format!("{rev}..working tree"),
            Self::Between(a, b) => format!("{a}..{b}"),
            Self::MergeBase(a, b) => format!("{a}...{b}"),
        }
    }
}

/// Compares `args.project` at the revisions in `args.target` (default `HEAD`), prints
/// the summary and writes the report. `Ok(false)` when `--fail-on-breaking` is given
/// and a change may break clients.
pub fn diff(args: &Args) -> Result<bool, String> {
    let range = Range::parse(args.target.as_deref().unwrap_or("HEAD"))?;
    let project = std::fs::canonicalize(&args.project)
        .map_err(|e| format!("{}: {e}", args.project.display()))?;
    let repo = gix::discover(&project)
        .map_err(|e| format!("{} is not in a git repository: {e}", project.display()))?;
    let workdir = repo
        .workdir()
        .ok_or("the git repository has no working tree")?;
    let workdir = std::fs::canonicalize(workdir).map_err(|e| e.to_string())?;
    let at = |tree: gix::Tree<'_>| open(&revision::TreeStorage::new(tree, &workdir), &project);
    let (before, after) = match &range {
        Range::Worktree(rev) => (
            at(revision::tree(&repo, rev)?)?,
            open(&FsStorage, &project)?,
        ),
        Range::Between(a, b) => (
            at(revision::tree(&repo, a)?)?,
            at(revision::tree(&repo, b)?)?,
        ),
        Range::MergeBase(a, b) => (
            at(revision::merge_base(&repo, a, b)?)?,
            at(revision::tree(&repo, b)?)?,
        ),
    };
    let Some(name) = after
        .as_ref()
        .or(before.as_ref())
        .map(|c| c.name().to_owned())
    else {
        return Err(format!(
            "no Tortuga project in {} at {}",
            project.display(),
            range.label()
        ));
    };
    let changes = api_diff::diff(before.as_ref(), after.as_ref());
    print!("{}", api_diff::markdown(&changes, &name, &range.label()));
    if let Some((kind, out)) = &args.report {
        match kind {
            ReportKind::Json => {
                let text = serde_json::to_string_pretty(&serde_json::json!({
                    "project": name,
                    "revisions": range.label(),
                    "breaking": changes.breaking(),
                    "requests": changes.requests,
                }))
                .map_err(|e| e.to_string())?;
                std::fs::write(out, text).map_err(|e| format!("{}: {e}", out.display()))?;
            }
            ReportKind::Junit => return Err("diff writes --report json only".into()),
        }
    }
    Ok(args.breaking == BreakingChanges::Reported || changes.breaking() == 0)
}

/// The project at `root` in `storage`; `None` when there is none.
fn open(storage: &dyn Storage, root: &Path) -> Result<Option<Collection>, String> {
    match Collection::open(storage, root) {
        Ok(c) => Ok(Some(c)),
        Err(Error::NotACollection(_)) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::Range;

    #[test]
    fn revisions_are_read_as_git_reads_them() {
        // R-CLI-7
        let r = |a: &str, b: &str| (a.to_owned(), b.to_owned());
        assert_eq!(Range::parse("main"), Ok(Range::Worktree("main".into())));
        let (a, b) = r("main", "HEAD");
        assert_eq!(Range::parse("main..HEAD"), Ok(Range::Between(a, b)));
        let (a, b) = r("main", "HEAD");
        assert_eq!(Range::parse("main.."), Ok(Range::Between(a, b)));
        let (a, b) = r("origin/main", "feature/ü😀");
        assert_eq!(
            Range::parse("origin/main...feature/ü😀"),
            Ok(Range::MergeBase(a, b))
        );
        assert!(Range::parse(" ").is_err());
        assert_eq!(
            Range::parse("v1.2...").map(|r| r.label()),
            Ok("v1.2...HEAD".into())
        );
    }
}
