//! `tortuga-cli load` (R-FLOW-15, D87): a workflow run by many virtual users at once,
//! with each step's sends, failures and response times.

use std::fmt::Write as _;
use std::time::Duration;

use std::time::Instant;
use tortuga_core::workflow::{self, LoadLimit, LoadOptions, LoadReport, MAX_USERS};

use crate::{Args, Prepared, ProtectedRuns, prepare};

/// `--users`, `--duration`, `--iterations` and `--ramp-up` as given.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LoadArgs {
    users: Option<u32>,
    duration_s: Option<u64>,
    iterations: Option<u32>,
    ramp_up_s: Option<u64>,
}

impl LoadArgs {
    /// Takes the value of one of the load options.
    pub fn set(&mut self, option: &str, value: &str) -> Result<(), String> {
        let number: u64 = value
            .parse()
            .map_err(|_| format!("{option} needs a whole number, not '{value}'"))?;
        let small = || u32::try_from(number).map_err(|_| format!("{option} is too large"));
        match option {
            "--users" => self.users = Some(small()?),
            "--duration" => self.duration_s = Some(number),
            "--iterations" => self.iterations = Some(small()?),
            "--ramp-up" => self.ramp_up_s = Some(number),
            other => return Err(format!("unknown option '{other}'")),
        }
        Ok(())
    }

    fn options(&self) -> Result<LoadOptions, String> {
        let users = self.users.ok_or("load needs --users N")?;
        if !(1..=MAX_USERS).contains(&users) {
            return Err(format!("--users must be 1 to {MAX_USERS}"));
        }
        let limit = match (self.duration_s, self.iterations) {
            (Some(s), None) if s > 0 => LoadLimit::Duration(Duration::from_secs(s)),
            (None, Some(n)) if n > 0 => LoadLimit::Iterations(n),
            _ => return Err("load needs one of --duration S or --iterations N (above 0)".into()),
        };
        Ok(LoadOptions {
            users,
            limit,
            ramp_up: Duration::from_secs(self.ramp_up_s.unwrap_or(0)),
        })
    }
}

/// Load-tests the workflow `args` names. `Ok(true)` when no send failed.
pub fn load(args: &Args) -> Result<bool, String> {
    if args.target.is_none() {
        return Err("which workflow? load needs <project> <workflow>".into());
    }
    let load = args.load.options()?;
    // Never load that may change data on a protected environment, not even with
    // --allow-protected (R-FLOW-15, R-SRV-9).
    let Prepared {
        collection,
        workflow,
        options,
        client,
        runtime,
        mock: _mock,
    } = prepare(args, ProtectedRuns::Refused)?;
    let limit = match load.limit {
        LoadLimit::Duration(d) => format!("for {} s", d.as_secs()),
        LoadLimit::Iterations(n) => format!("{n} iterations each"),
    };
    println!("Load test: {} users, {limit}", load.users);
    let mut shown = Instant::now();
    let report = runtime.block_on(workflow::load(
        &client,
        &collection,
        &workflow,
        &options,
        load,
        |so_far| {
            // A line a second, so a long test shows that it is going.
            if shown.elapsed() >= Duration::from_secs(1) {
                shown = Instant::now();
                let (sends, failed) = so_far.sends();
                println!(
                    "  {:.0} s: {sends} sends, {failed} failed",
                    so_far.elapsed.as_secs_f64()
                );
            }
        },
    ));
    print!("{}", table(&report));
    Ok(report.sends().1 == 0)
}

/// The report as a table, a line per step, and the totals.
fn table(report: &LoadReport) -> String {
    let ms = |v: Option<u64>| v.map_or_else(|| "-".to_owned(), |v| format!("{v} ms"));
    let width = report
        .steps
        .iter()
        .map(|s| s.name.chars().count())
        .max()
        .unwrap_or(0)
        .max(4);
    let mut out = format!(
        "\n{:<width$}  {:>7}  {:>7}  {:>9}  {:>9}  {:>9}\n",
        "Step", "Sends", "Failed", "Median", "p95", "Max"
    );
    for s in &report.steps {
        let _ = writeln!(
            out,
            "{:<width$}  {:>7}  {:>7}  {:>9}  {:>9}  {:>9}",
            s.name,
            s.sends,
            s.failures,
            ms(s.median_ms()),
            ms(s.p95_ms()),
            ms(s.max_ms())
        );
    }
    let (sends, failed) = report.sends();
    #[expect(clippy::cast_precision_loss, reason = "counts of sends are small")]
    let rate = if sends == 0 {
        0.0
    } else {
        failed as f64 * 100.0 / sends as f64
    };
    let _ = writeln!(
        out,
        "\n{sends} sends in {:.1} s ({:.1}/s), {failed} failed ({rate:.1} %), {} iterations",
        report.elapsed.as_secs_f64(),
        report.sends_per_second(),
        report.iterations
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R-FLOW-15: users and exactly one of duration or iterations are needed.
    #[test]
    fn load_options_are_checked() {
        let mut a = LoadArgs::default();
        assert!(a.options().is_err(), "no users");
        a.set("--users", "5").expect("users");
        assert!(a.options().is_err(), "no limit");
        a.set("--duration", "30").expect("duration");
        a.set("--ramp-up", "10").expect("ramp-up");
        let o = a.options().expect("options");
        assert_eq!(o.limit, LoadLimit::Duration(Duration::from_secs(30)));
        assert_eq!(o.ramp_up, Duration::from_secs(10));
        a.set("--iterations", "3").expect("iterations");
        assert!(a.options().is_err(), "both limits");
        assert!(a.set("--users", "lots").is_err());
        let mut many = LoadArgs::default();
        many.set("--users", "101").expect("parses");
        many.set("--iterations", "1").expect("parses");
        assert!(many.options().is_err(), "more than 100 users");
    }

    /// R-FLOW-15: the table lines up names of any length, also non-ASCII ones.
    #[test]
    fn the_report_is_a_table() {
        let report = LoadReport::default();
        let text = table(&report);
        assert!(text.contains("Step"), "{text}");
        assert!(
            text.contains("0 sends in 0.0 s (0.0/s), 0 failed (0.0 %)"),
            "{text}"
        );
    }
}
