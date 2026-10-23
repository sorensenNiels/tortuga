//! "Send to every environment" (R-SRV-10): the shown request, sent at once to each of
//! its project's environments with that environment's values, secrets and cookies, but
//! without scripts, captures or history, so comparing never changes anything. The
//! results come back over an [`Inbox`] as they arrive.

use std::sync::Arc;

use futures_util::future::{AbortHandle, abortable};
use tortuga_core::{Response, vars};

use super::{Guarded, Inbox, Modal, TortugaUi};
use crate::components::{ConfirmDialog, FanoutResult, FanoutRow};
use crate::format::sentence;
use crate::state::ItemKey;

/// A request sent to every environment, and what came back so far.
#[derive(Debug)]
pub(super) struct Fanout {
    pub key: ItemKey,
    /// "GET Get order".
    pub title: String,
    pub rows: Vec<FanoutRow>,
    id: u64,
    aborts: Vec<AbortHandle>,
}

impl Drop for Fanout {
    fn drop(&mut self) {
        // Closing the dialog stops what is still on its way.
        for abort in &self.aborts {
            abort.abort();
        }
    }
}

/// One environment's answer.
#[derive(Debug)]
pub(super) struct FanoutDone {
    id: u64,
    index: usize,
    result: Result<Response, String>,
}

/// Where the answers arrive.
pub(super) type FanoutInbox = Inbox<FanoutDone>;

impl TortugaUi {
    /// Sends the shown request to every environment of its project; to a protected one,
    /// a method other than GET, HEAD or OPTIONS asks first (R-SRV-9).
    pub(super) fn send_everywhere(&mut self, egui_ctx: &egui::Context) {
        let Some(key) = self.selection.clone() else {
            return self
                .state
                .inform("Save the request in a project to send it to its environments.");
        };
        let environments = self.state.environments_of(&key.root);
        if environments.is_empty() {
            return self
                .state
                .inform("This project has no environments to send to.");
        }
        let method = self.current.method;
        let protected: Vec<String> = environments
            .iter()
            .filter(|e| e.protected)
            .map(|e| format!("“{}”", e.name))
            .collect();
        if method.is_safe() || protected.is_empty() {
            return self.start_fanout(egui_ctx);
        }
        let dialog = ConfirmDialog {
            title: format!("Send {} to every environment?", method.as_str()),
            message: format!(
                "That includes the protected {}: this request may change their data.",
                protected.join(" and ")
            ),
            confirm_label: "Send".into(),
        };
        self.modal = Some(Modal::Protected(Guarded::SendEverywhere, dialog));
    }

    /// Sends the shown request to every environment, without asking.
    pub(super) fn start_fanout(&mut self, egui_ctx: &egui::Context) {
        let Some(key) = self.selection.clone() else {
            return;
        };
        let environments = self.state.environments_of(&key.root);
        self.next_send_id += 1;
        let id = self.next_send_id;
        let jar = self.state.cookie_jar(Some(&key.root));
        let mut rows = Vec::new();
        let mut aborts = Vec::new();
        for (index, env) in environments.into_iter().enumerate() {
            let ctx = self
                .state
                .context_in(&key.root, Some(&key.path), Some(&env), false);
            let result = match vars::prepare(&self.current, &ctx) {
                Err(e) => FanoutResult::Failed(sentence(&e.to_string())),
                Ok(prepared) => {
                    let (client, jar, tx) = (
                        self.state.client.clone(),
                        jar.clone(),
                        self.fanout_results.sender(),
                    );
                    let repaint = egui_ctx.clone();
                    let (task, abort) = abortable(async move {
                        let result = client
                            .execute_with_cookies(&prepared, Some(&jar))
                            .await
                            .map_err(|e| sentence(&e.to_string()));
                        // The receiver only goes away with the app.
                        let _ = tx.send(FanoutDone { id, index, result });
                        repaint.request_repaint();
                    });
                    self.state.host.spawn(Box::pin(async move {
                        // `Err`: the dialog was closed before the answer came.
                        let _ = task.await;
                    }));
                    aborts.push(abort);
                    FanoutResult::Sending
                }
            };
            rows.push(FanoutRow {
                environment: env.name,
                color: env.color,
                result,
                shape: Vec::new(),
            });
        }
        let title = format!("{} {}", self.current.method.as_str(), self.current.name);
        self.fanout = Some(Fanout {
            key,
            title,
            rows,
            id,
            aborts,
        });
        self.modal = Some(Modal::Fanout);
    }

    /// Takes the answers that arrived, and compares the shapes again (R-SRV-10).
    pub(super) fn collect_fanout(&mut self) {
        let mut arrived = false;
        while let Some(done) = self.fanout_results.try_recv() {
            let Some(fanout) = self.fanout.as_mut().filter(|f| f.id == done.id) else {
                continue;
            };
            if let Some(row) = fanout.rows.get_mut(done.index) {
                row.result = match done.result {
                    Ok(r) => FanoutResult::Done(Arc::new(r)),
                    Err(e) => FanoutResult::Failed(e),
                };
                arrived = true;
            }
        }
        if arrived {
            self.state
                .save_cookies(self.fanout.as_ref().map(|f| f.key.root.as_path()));
            if let Some(fanout) = self.fanout.as_mut() {
                compare_shapes(&mut fanout.rows);
            }
        }
    }
}

/// Each answer's shape against the first environment's answer.
fn compare_shapes(rows: &mut [FanoutRow]) {
    let baseline = rows.iter().find_map(|r| match &r.result {
        FanoutResult::Done(response) => Some(Arc::clone(response)),
        FanoutResult::Sending | FanoutResult::Failed(_) => None,
    });
    for row in rows {
        row.shape = match (&baseline, &row.result) {
            (Some(base), FanoutResult::Done(response)) if !Arc::ptr_eq(base, response) => {
                tortuga_core::changes::shape_changes(base, response)
            }
            _ => Vec::new(),
        };
    }
}

#[cfg(test)]
mod tests {
    use tortuga_core::format::Environment;
    use tortuga_core::{HttpMethod, KeyValue};

    use super::*;

    fn json(body: &str) -> Response {
        Response {
            status: 200,
            status_text: "OK".into(),
            headers: vec![KeyValue::new("Content-Type", "application/json")],
            body: body.as_bytes().to_vec(),
            elapsed: std::time::Duration::from_millis(4),
        }
    }

    /// Two environments, one of them protected; "Staging" lacks `{{host}}`.
    fn environments(ui: &mut TortugaUi, root: &std::path::Path) {
        let dev = Environment {
            id: "dev".into(),
            name: "Dev".into(),
            variables: [("host".to_owned(), "dev.test".to_owned())].into(),
            ..Environment::default()
        };
        let staging = Environment {
            id: "staging".into(),
            name: "Staging ü😀".into(),
            protected: true,
            ..Environment::default()
        };
        ui.state
            .run(root, |c, s| c.save_config(s, &c.manifest, &[dev, staging]));
        ui.current.url = "https://{{host}}/orders".into();
    }

    /// R-SRV-10: one row per environment; one that can't be prepared fails at once, the
    /// others are sent; the answers fill the rows and their shapes are compared.
    #[test]
    fn a_request_goes_to_every_environment() {
        let (mut ui, root, _) = crate::shell::tests::setup("fanout");
        environments(&mut ui, &root);
        let ctx = egui::Context::default();
        ui.send_everywhere(&ctx);
        assert!(
            matches!(ui.modal, Some(Modal::Fanout)),
            "a GET goes at once"
        );
        let fanout = ui.fanout.as_ref().expect("sent");
        assert_eq!(fanout.rows.len(), 2);
        assert!(matches!(fanout.rows[0].result, FanoutResult::Sending));
        let FanoutResult::Failed(why) = &fanout.rows[1].result else {
            panic!("undefined {{host}}")
        };
        assert!(why.contains("host"), "{why}");

        let tx = ui.fanout_results.sender();
        let id = fanout.id;
        tx.send(FanoutDone {
            id,
            index: 0,
            result: Ok(json(r#"{"id": 1}"#)),
        })
        .expect("send");
        tx.send(FanoutDone {
            id: id + 100,
            index: 0,
            result: Err("an older fan-out".into()),
        })
        .expect("send");
        ui.collect_fanout();
        let rows = &ui.fanout.as_ref().expect("still open").rows;
        assert!(
            matches!(rows[0].result, FanoutResult::Done(_)),
            "the old answer is dropped"
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-SRV-10: the first answer is what the others' shapes are compared with.
    #[test]
    fn shapes_are_compared_with_the_first_answer() {
        let row = |result| FanoutRow {
            environment: "E".into(),
            color: None,
            result,
            shape: Vec::new(),
        };
        let mut rows = vec![
            row(FanoutResult::Failed("down".into())),
            row(FanoutResult::Done(Arc::new(json(
                r#"{"id": 1, "legacy": true}"#,
            )))),
            row(FanoutResult::Done(Arc::new(json(
                r#"{"id": 2, "discount": 0.1}"#,
            )))),
        ];
        compare_shapes(&mut rows);
        assert!(rows[0].shape.is_empty() && rows[1].shape.is_empty());
        let at: Vec<&str> = rows[2].shape.iter().map(|c| c.at.as_str()).collect();
        assert_eq!(at, ["$.legacy", "$.discount"]);
    }

    /// R-SRV-9, R-SRV-10: with a protected environment among them, a POST asks first.
    #[test]
    fn writes_to_every_environment_ask_when_one_is_protected() {
        let (mut ui, root, _) = crate::shell::tests::setup("fanout-protected");
        environments(&mut ui, &root);
        ui.current.method = HttpMethod::Post;
        ui.send_everywhere(&egui::Context::default());
        let Some(Modal::Protected(Guarded::SendEverywhere, d)) = &ui.modal else {
            panic!("asks first")
        };
        assert!(d.message.contains("“Staging ü😀”"), "{}", d.message);
        assert!(ui.fanout.is_none(), "nothing sent yet");
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
