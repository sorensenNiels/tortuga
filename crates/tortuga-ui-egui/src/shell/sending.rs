//! Sending the selected request off the UI thread, cancelling it, and the JSON filter
//! of its response (R-HTTP-7, R-HTTP-13, R-TEST-1).

use std::sync::Arc;

use futures_util::future::abortable;
use tortuga_core::vars;

use super::{FilterDone, FilterJob, Finished, InFlight, Selection, TortugaUi};
use crate::components::{Outcome, ResponseView, TestsView};
use crate::format::sentence;

impl TortugaUi {
    /// The selection whose request is being sent, if any.
    pub(super) fn sending(&self) -> Option<&Selection> {
        self.in_flight.as_ref().map(|f| &f.key)
    }

    pub(super) fn send(&mut self, egui_ctx: &egui::Context) {
        if let Some(f) = self.in_flight.take() {
            f.abort.abort();
        }
        let key = self.selection.clone();
        let ctx = self.state.request_context(key.as_ref());
        self.response.reset();
        self.forget_past(&key);
        if let Some(sent) = self.sent.get_mut(&key) {
            sent.tests = None;
        }
        // Undefined variables etc. show at once, unless a pre-request script may define them.
        if self.current.scripts.pre_request.trim().is_empty()
            && let Err(e) = vars::prepare(&self.current, &ctx)
        {
            self.sent.entry(key).or_default().outcome =
                Some(Outcome::Error(sentence(&e.to_string())));
            return;
        }
        self.next_send_id += 1;
        let id = self.next_send_id;
        let client = self.state.client.clone();
        let jar = self
            .state
            .cookie_jar(key.as_ref().map(|k| k.root.as_path()));
        let tx = self.finished.sender();
        let repaint = egui_ctx.clone();
        // For the history (R-HIST-1): the request as in the editor, not as resolved.
        let sent_at = tortuga_core::history::now_ms();
        let entry_id = tortuga_core::history::new_id(sent_at, id);
        let path = key.as_ref().map(|k| k.path.clone());
        let request = self.current.clone();
        let breakpoints = self
            .sent
            .get(&key)
            .map(|s| s.breakpoints.clone())
            .unwrap_or_default();
        let environment = ctx.environment.clone();
        let (task, abort) = abortable(async move {
            // The pre-request script, sending, captures and tests (R-TEST-1..4).
            let mut step = tortuga_core::runner::run_step(
                &client,
                Some(&jar),
                &request,
                &ctx,
                tortuga_core::runner::Extra {
                    breakpoints: Some(&breakpoints),
                    // The design is checked on every send (R-MOCK-8, D47).
                    check_examples: true,
                    ..tortuga_core::runner::Extra::default()
                },
            )
            .await;
            // Off the UI thread: copying up to 1 MB of body for the history.
            let for_history = match &step.response {
                Some(Ok(r)) => Ok(r),
                Some(Err(e)) => Err(e.clone()),
                None => Err(step.errors.first().cloned().unwrap_or_default()),
            };
            let entry = tortuga_core::history::Entry::new(
                entry_id,
                sent_at,
                path,
                &request,
                environment,
                for_history,
            );
            let tests = TestsView::from_step(&step).map(|t| TestsView {
                ran: request.scripts.clone(),
                ..t
            });
            // Prepared here, off the UI thread: pretty-printing big bodies takes a while (R-NF-2).
            let outcome = Outcome::from_step(step.response.take(), &step.errors);
            // The receiver only goes away with the app.
            let _ = tx.send(Finished {
                id,
                outcome,
                entry: Some(entry),
                tests,
            });
            repaint.request_repaint();
        });
        self.state.host.spawn(Box::pin(async move {
            // `Err` means it was cancelled (R-HTTP-7); dropping the future closed the connection.
            let _ = task.await;
        }));
        self.in_flight = Some(InFlight { key, id, abort });
    }

    pub(super) fn cancel(&mut self) {
        if let Some(f) = self.in_flight.take() {
            f.abort.abort();
            // A redirect may already have set cookies.
            self.state
                .save_cookies(f.key.as_ref().map(|k| k.root.as_path()));
            self.sent.entry(f.key).or_default().outcome = Some(Outcome::Cancelled);
        }
    }

    /// Filters the shown JSON response in the background whenever the response or the
    /// filter changes (R-HTTP-13). A result that arrives for an older filter is dropped.
    pub(super) fn update_filter(
        &mut self,
        view: Option<&Arc<ResponseView>>,
        expr: &str,
        ctx: &egui::Context,
    ) {
        while let Some(done) = self.filter_results.try_recv() {
            if let Some(job) = self.filter_job.as_mut().filter(|j| j.id == done.id) {
                job.result = Some(done.result);
            }
        }
        let Some(view) = view.filter(|_| !expr.is_empty()) else {
            if let Some(job) = self.filter_job.take() {
                job.abort.abort();
            }
            return;
        };
        if self
            .filter_job
            .as_ref()
            .is_some_and(|j| Arc::ptr_eq(&j.view, view) && j.expr == expr)
        {
            return;
        }
        if let Some(job) = self.filter_job.take() {
            job.abort.abort();
        }
        self.next_send_id += 1;
        let id = self.next_send_id;
        let (task_view, task_expr) = (Arc::clone(view), expr.to_owned());
        let tx = self.filter_results.sender();
        let repaint = ctx.clone();
        let (task, abort) = abortable(async move {
            let result = crate::components::filter_body(&task_view, &task_expr);
            // The receiver only goes away with the app.
            let _ = tx.send(FilterDone { id, result });
            repaint.request_repaint();
        });
        self.state.host.spawn(Box::pin(async move {
            let _ = task.await;
        }));
        self.filter_job = Some(FilterJob {
            view: Arc::clone(view),
            expr: expr.to_owned(),
            id,
            result: None,
            abort,
        });
    }

    pub(super) fn collect_finished(&mut self) {
        while let Some(done) = self.finished.try_recv() {
            if let Some(f) = self.in_flight.take_if(|f| f.id == done.id) {
                if f.key == self.selection {
                    self.response.reset();
                }
                self.state
                    .save_cookies(f.key.as_ref().map(|k| k.root.as_path()));
                if let Some(entry) = done.entry {
                    self.record_history(f.key.as_ref().map(|k| k.root.as_path()), entry);
                }
                if let Some(tests) = done.tests {
                    self.state
                        .apply_runtime(f.key.as_ref().map(|k| k.root.as_path()), &tests.vars);
                    self.sent.entry(f.key.clone()).or_default().tests = Some(tests);
                }
                self.sent.entry(f.key).or_default().outcome = Some(done.outcome);
            }
        }
    }
}
