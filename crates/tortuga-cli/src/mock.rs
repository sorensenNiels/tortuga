//! The project's mock from the command line (R-MOCK-5): `tortuga-cli mock` serves it,
//! `tortuga-cli run --mock` runs against it.

use std::sync::{Arc, RwLock};

use tortuga_core::Collection;
use tortuga_core::mock::server::{self, RunningMock};
use tortuga_core::mock::{self, Hit, MockApi, Outcome};

use crate::{Args, count, open_project};

/// Serves the mock of `args.project` until the process is stopped.
pub fn serve(args: &Args) -> Result<(), String> {
    let (collection, environment, _) = open_project(args)?;
    let api = MockApi::new(&collection, environment.as_deref());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let examples = api.example_count();
        let running = server::start(
            args.port.unwrap_or(mock::DEFAULT_PORT),
            Arc::new(RwLock::new(api)),
            |hit| println!("{}", hit_line(&hit)),
        )
        .await
        .map_err(|e| e.to_string())?;
        println!(
            "Mock of {} on {} ({} with examples). Stop it with Ctrl+C.",
            collection.name(),
            running.base_url(),
            count(examples, "request")
        );
        // Serves until the process is stopped.
        std::future::pending::<()>().await;
        Ok(())
    })
}

/// Starts the mock of `collection` on `port` (`0`: a free one) and returns the project
/// with the mock's environment added, which gets the variables of `environment`.
pub async fn start_for_run(
    mut collection: Collection,
    environment: Option<&str>,
    port: u16,
) -> Result<(RunningMock, Collection), String> {
    let api = MockApi::new(&collection, environment);
    let running = server::start(port, Arc::new(RwLock::new(api)), |hit| {
        // A request the mock can't answer fails its step with a 404; this says why.
        if let Outcome::NoMatch(reason) = &hit.outcome {
            println!("  mock: {} {}: {reason}", hit.method, hit.target);
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    let mut env = mock::environment(&collection, &running.base_url());
    if let Some(chosen) = environment.and_then(|id| collection.environment(id)) {
        env.variables.clone_from(&chosen.variables);
    }
    collection.environments.push(env);
    Ok((running, collection))
}

/// "200 GET /users/7 → Get user / found".
fn hit_line(hit: &Hit) -> String {
    let answered = match &hit.outcome {
        Outcome::Example {
            request_name,
            example,
            ..
        } => format!("{request_name} / {example}"),
        Outcome::NoMatch(reason) => format!("no match: {reason}"),
    };
    format!("{} {} {} → {answered}", hit.status, hit.method, hit.target)
}
