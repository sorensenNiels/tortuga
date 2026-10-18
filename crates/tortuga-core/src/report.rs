//! Reports of workflow runs for CI (R-CLI-3): `JUnit` XML and JSON.

use std::fmt::Write as _;

use serde::Serialize;

use crate::runner::StepSummary;
use crate::workflow::{Iteration, RunResult, Workflow};

/// The name of iteration `n` (from 0), with its data row if there is one.
fn iteration_name(workflow: &Workflow, n: usize, iteration: &Iteration, total: usize) -> String {
    match &iteration.row {
        Some(row) => {
            let values: Vec<String> = row.iter().map(|(k, v)| format!("{k}={v}")).collect();
            format!("{} (row {}: {})", workflow.name, n + 1, values.join(", "))
        }
        None if total > 1 => format!("{} (run {})", workflow.name, n + 1),
        None => workflow.name.clone(),
    }
}

fn seconds(ms: Option<u64>) -> String {
    #[expect(clippy::cast_precision_loss, reason = "milliseconds of a request")]
    let s = ms.unwrap_or(0) as f64 / 1000.0;
    format!("{s:.3}")
}

/// `JUnit` XML: a test suite per iteration; a test case per test, or one per step when it
/// has no tests; errors as `<error>`, skipped steps as `<skipped/>`.
pub fn junit(workflow: &Workflow, result: &RunResult) -> String {
    let mut suites = String::new();
    let (mut all_tests, mut all_failures, mut all_errors) = (0, 0, 0);
    let total = result.iterations.len();
    for (n, iteration) in result.iterations.iter().enumerate() {
        let mut cases = String::new();
        let (mut tests, mut failures, mut errors) = (0, 0, 0);
        for step in &iteration.steps {
            let summary = StepSummary::from(&step.outcome);
            let time = seconds(summary.elapsed_ms);
            let class = escape(&step.name);
            if step.outcome.tests.is_empty() {
                tests += 1;
                let _ = write!(
                    cases,
                    r#"    <testcase classname="{class}" name="request" time="{time}""#
                );
                match step.outcome.failure() {
                    Some(why) => {
                        failures += 1;
                        let _ = write!(
                            cases,
                            ">\n      <failure message=\"{}\"/>\n    </testcase>\n",
                            escape(&why)
                        );
                    }
                    None => cases.push_str("/>\n"),
                }
            }
            for test in &step.outcome.tests {
                tests += 1;
                let _ = write!(
                    cases,
                    r#"    <testcase classname="{class}" name="{}" time="{time}""#,
                    escape(&test.name)
                );
                if test.passed {
                    cases.push_str("/>\n");
                } else {
                    failures += 1;
                    let _ = write!(
                        cases,
                        ">\n      <failure message=\"{}\"/>\n    </testcase>\n",
                        escape(test.message.as_deref().unwrap_or("failed"))
                    );
                }
            }
            if !step.outcome.tests.is_empty() && !summary.errors.is_empty() {
                tests += 1;
                errors += 1;
                let _ = write!(
                    cases,
                    "    <testcase classname=\"{class}\" name=\"errors\" time=\"{time}\">\n      <error message=\"{}\"/>\n    </testcase>\n",
                    escape(&summary.errors.join("; "))
                );
            }
        }
        for step in workflow
            .steps
            .iter()
            .skip(iteration.steps.len())
            .take(iteration.skipped)
        {
            tests += 1;
            let name = step.name.clone().unwrap_or_else(|| step.request.clone());
            let _ = writeln!(
                cases,
                "    <testcase classname=\"{}\" name=\"request\">\n      <skipped message=\"an earlier step failed\"/>\n    </testcase>",
                escape(&name)
            );
        }
        let _ = write!(
            suites,
            "  <testsuite name=\"{}\" tests=\"{tests}\" failures=\"{failures}\" errors=\"{errors}\">\n{cases}  </testsuite>\n",
            escape(&iteration_name(workflow, n, iteration, total))
        );
        all_tests += tests;
        all_failures += failures;
        all_errors += errors;
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites name=\"{}\" tests=\"{all_tests}\" failures=\"{all_failures}\" errors=\"{all_errors}\">\n{suites}</testsuites>\n",
        escape(&workflow.name)
    )
}

#[derive(Serialize)]
struct JsonReport<'a> {
    workflow: &'a str,
    passed: bool,
    iterations: Vec<JsonIteration<'a>>,
}

#[derive(Serialize)]
struct JsonIteration<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    row: Option<&'a std::collections::BTreeMap<String, String>>,
    passed: bool,
    skipped: usize,
    steps: Vec<JsonStep<'a>>,
}

#[derive(Serialize)]
struct JsonStep<'a> {
    name: &'a str,
    request: &'a str,
    #[serde(flatten)]
    summary: StepSummary,
}

/// The run as JSON (R-CLI-3).
pub fn json(workflow: &Workflow, result: &RunResult) -> String {
    let report = JsonReport {
        workflow: &workflow.name,
        passed: result.passed(),
        iterations: result
            .iterations
            .iter()
            .map(|i| JsonIteration {
                row: i.row.as_ref(),
                passed: !i.failed(),
                skipped: i.skipped,
                steps: i
                    .steps
                    .iter()
                    .map(|s| JsonStep {
                        name: &s.name,
                        request: &s.request,
                        summary: StepSummary::from(&s.outcome),
                    })
                    .collect(),
            })
            .collect(),
    };
    serde_json::to_string_pretty(&report).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' => out.push_str("&#10;"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::model::Response;
    use crate::runner::StepOutcome;
    use crate::script::TestResult;
    use crate::workflow::{Step, StepResult};

    fn outcome(status: u16, tests: &[(&str, bool)]) -> StepOutcome {
        StepOutcome {
            response: Some(Ok(Response {
                status,
                status_text: "OK".into(),
                headers: Vec::new(),
                body: Vec::new(),
                elapsed: Duration::from_millis(12),
            })),
            tests: tests
                .iter()
                .map(|(name, passed)| TestResult {
                    name: (*name).into(),
                    passed: *passed,
                    message: (!passed).then(|| "Expected 1 to be <2>".into()),
                })
                .collect(),
            ..StepOutcome::default()
        }
    }

    fn sample() -> (Workflow, RunResult) {
        let wf = Workflow {
            name: "Users & co".into(),
            steps: vec![
                Step::new("a.toml".as_ref()),
                Step::new("b.toml".as_ref()),
                Step::new("c.toml".as_ref()),
            ],
            ..Workflow::default()
        };
        let result = RunResult {
            iterations: vec![Iteration {
                row: Some([("user".to_owned(), "ann".to_owned())].into()),
                steps: vec![
                    StepResult {
                        name: "Log in".into(),
                        request: "a.toml".into(),
                        outcome: outcome(200, &[]),
                    },
                    StepResult {
                        name: "Get".into(),
                        request: "b.toml".into(),
                        outcome: outcome(200, &[("ok", true), ("name", false)]),
                    },
                ],
                skipped: 1,
            }],
        };
        (wf, result)
    }

    #[test]
    fn junit_report() {
        let (wf, result) = sample();
        assert_eq!(
            junit(&wf, &result),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="Users &amp; co" tests="4" failures="1" errors="0">
  <testsuite name="Users &amp; co (row 1: user=ann)" tests="4" failures="1" errors="0">
    <testcase classname="Log in" name="request" time="0.012"/>
    <testcase classname="Get" name="ok" time="0.012"/>
    <testcase classname="Get" name="name" time="0.012">
      <failure message="Expected 1 to be &lt;2&gt;"/>
    </testcase>
    <testcase classname="c.toml" name="request">
      <skipped message="an earlier step failed"/>
    </testcase>
  </testsuite>
</testsuites>
"#
        );
    }

    #[test]
    fn json_report() {
        let (wf, result) = sample();
        let value: serde_json::Value = serde_json::from_str(&json(&wf, &result)).expect("json");
        assert_eq!(value["passed"], false);
        assert_eq!(value["iterations"][0]["row"]["user"], "ann");
        assert_eq!(value["iterations"][0]["skipped"], 1);
        assert_eq!(value["iterations"][0]["steps"][1]["status"], 200);
        assert_eq!(
            value["iterations"][0]["steps"][1]["tests"][1]["passed"],
            false
        );
    }
}
