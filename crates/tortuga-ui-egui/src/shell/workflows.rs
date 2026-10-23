//! Workflows in the shell: opening, editing, saving and running them (R-FLOW-1..4),
//! unsaved workflow edits kept across restarts (R-FLOW-5), and the runtime variables
//! dialog (R-TEST-5).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use egui::{Frame, Margin, Panel, Ui};
use futures_util::future::{AbortHandle, abortable};
use tortuga_core::workflow::{self, FlowEnd, Progress, RunOptions, Workflow};
use tortuga_core::{Item, data};

use super::{Guarded, Modal, TortugaUi, upload_path};
use crate::components::SidebarAction;
use crate::components::{
    ConfirmDialog, DialogResult, IterationView, NameDialog, RequestChoice, RunView,
    RuntimeVarsDialog, StepView, WorkflowAction, WorkflowContext, WorkflowEditorState,
    workflow_editor, workflow_results,
};
use crate::host::SavedWorkflowDraft;
use crate::state::{ItemKey, WorkflowKey};

/// A workflow open in the editor.
#[derive(Debug)]
pub(super) struct OpenWorkflow {
    draft: Workflow,
    /// The saved version, followed while the project is open, so unsaved edits are
    /// still known after it closes (R-FLOW-5).
    base: Option<Workflow>,
    st: WorkflowEditorState,
    run: Option<RunView>,
    run_id: u64,
    abort: Option<AbortHandle>,
}

/// Progress of a run, from the task to the UI.
#[derive(Debug)]
pub(super) struct WorkflowEvent {
    key: WorkflowKey,
    run_id: u64,
    kind: EventKind,
}

#[derive(Debug)]
enum EventKind {
    Started {
        iteration: usize,
        step: usize,
    },
    Step {
        iteration: usize,
        view: Box<StepView>,
    },
    /// Per iteration: steps skipped after a failure, and how a script ended it.
    Done {
        ends: Vec<(usize, Option<FlowEnd>)>,
    },
}

/// Workflow state of the shell.
#[derive(Debug, Default)]
pub(super) struct Workflows {
    /// The workflow shown instead of a request.
    pub(super) open: Option<WorkflowKey>,
    editors: HashMap<WorkflowKey, OpenWorkflow>,
    next_run: u64,
    /// Unsaved edits of workflows whose project is not open (restored from last time,
    /// or of another workspace), with the saved version they started from (R-FLOW-5).
    waiting: HashMap<WorkflowKey, (Workflow, Option<Workflow>)>,
}

impl TortugaUi {
    fn saved_workflow(&self, key: &WorkflowKey) -> Option<Workflow> {
        self.state.collection(&key.root)?.workflow(&key.id).cloned()
    }

    /// Shows a workflow in the main area (R-FLOW-2).
    pub(super) fn open_workflow(&mut self, key: WorkflowKey) {
        self.auto_save();
        if !self.workflows.editors.contains_key(&key) {
            let Some(saved) = self.saved_workflow(&key) else {
                return;
            };
            let st = WorkflowEditorState::new(&saved);
            self.workflows.editors.insert(
                key.clone(),
                OpenWorkflow {
                    base: Some(saved.clone()),
                    draft: saved,
                    st,
                    run: None,
                    run_id: 0,
                    abort: None,
                },
            );
        }
        self.workflows.open = Some(key);
    }

    pub(super) fn new_workflow_dialog(&mut self, root: PathBuf) {
        self.modal = Some(Modal::NewWorkflow(
            root,
            NameDialog::new(&t!("workflow-new-title"), &t!("create"), ""),
        ));
    }

    pub(super) fn create_workflow(&mut self, root: &Path, name: &str) {
        if let Some(id) = self.state.run(root, |c, s| c.new_workflow(s, name)) {
            self.open_workflow(WorkflowKey {
                root: root.to_owned(),
                id,
            });
        }
    }

    pub(super) fn rename_workflow_dialog(&mut self, key: WorkflowKey) {
        let name = self
            .saved_workflow(&key)
            .map(|w| w.name)
            .unwrap_or_default();
        self.modal = Some(Modal::RenameWorkflow(
            key,
            NameDialog::new(&t!("workflow-rename-title"), &t!("rename"), &name),
        ));
    }

    /// Renames a workflow (R-FLOW-2). The file keeps its name; an open draft takes the
    /// new name and keeps its other edits.
    pub(super) fn rename_workflow(&mut self, key: &WorkflowKey, name: &str) {
        let Some(mut saved) = self.saved_workflow(key) else {
            return;
        };
        name.clone_into(&mut saved.name);
        if self
            .state
            .run(&key.root, |c, s| c.save_workflow(s, &saved))
            .is_some()
            && let Some(open) = self.workflows.editors.get_mut(key)
        {
            name.clone_into(&mut open.draft.name);
        }
    }

    pub(super) fn confirm_delete_workflow(&mut self, key: WorkflowKey) {
        let name = self
            .saved_workflow(&key)
            .map(|w| w.name)
            .unwrap_or_default();
        let dialog = ConfirmDialog {
            title: t!("workflow-delete-title"),
            message: t!("workflow-delete-message", name = name.as_str()),
            confirm_label: t!("delete"),
        };
        self.modal = Some(Modal::DeleteWorkflow(key, dialog));
    }

    pub(super) fn delete_workflow(&mut self, key: &WorkflowKey) {
        if self
            .state
            .run(&key.root, |c, s| c.delete_workflow(s, &key.id))
            .is_some()
        {
            self.close_workflow(key);
        }
    }

    fn close_workflow(&mut self, key: &WorkflowKey) {
        if let Some(open) = self.workflows.editors.remove(key)
            && let Some(abort) = open.abort
        {
            abort.abort();
        }
        if self.workflows.open.as_ref() == Some(key) {
            self.workflows.open = None;
        }
    }

    /// Steps follow a request or folder that moved (R-FLOW-1): on disk and in open drafts.
    pub(super) fn retarget_workflows(&mut self, root: &Path, old: &Path, new: &Path) {
        if old == new {
            return;
        }
        let affected = self
            .state
            .collection(root)
            .is_some_and(|c| c.workflows.iter().any(|w| w.clone().retarget(old, new)));
        if affected {
            self.state
                .run(root, |c, s| c.retarget_workflows(s, old, new));
        }
        for (key, open) in &mut self.workflows.editors {
            if key.root == root {
                open.draft.retarget(old, new);
            }
        }
    }

    /// Forgets open workflows whose project closed or whose file is gone. Unsaved edits
    /// of a closed project wait until it opens again (R-FLOW-5).
    pub(super) fn drop_missing_workflows(&mut self) {
        let gone: Vec<WorkflowKey> = self
            .workflows
            .editors
            .keys()
            .filter(|k| {
                self.state
                    .collection(&k.root)
                    .is_none_or(|c| c.workflow(&k.id).is_none())
            })
            .cloned()
            .collect();
        for (key, open) in &mut self.workflows.editors {
            let saved = self
                .state
                .collection(&key.root)
                .and_then(|c| c.workflow(&key.id));
            if saved.is_some() && open.base.as_ref() != saved {
                open.base = saved.cloned();
            }
        }
        for key in gone {
            if self.state.collection(&key.root).is_none()
                && let Some(open) = self.workflows.editors.get(&key)
                && open.base.as_ref() != Some(&open.draft)
            {
                let kept = (open.draft.clone(), open.base.clone());
                self.workflows.waiting.insert(key.clone(), kept);
            }
            self.close_workflow(&key);
        }
    }

    // ------------------------------------------------------------ unsaved edits (R-FLOW-5)

    /// Workflows with unsaved edits as they should be kept now, in a stable order.
    pub(super) fn workflow_drafts_snapshot(&self) -> Vec<SavedWorkflowDraft> {
        let open = self.workflows.editors.iter().filter_map(|(key, open)| {
            let base = self.saved_workflow(key);
            (base.as_ref() != Some(&open.draft)).then(|| (key, open.draft.clone(), base))
        });
        let waiting = self
            .workflows
            .waiting
            .iter()
            .map(|(key, (draft, base))| (key, draft.clone(), base.clone()));
        let mut drafts: Vec<SavedWorkflowDraft> = open
            .chain(waiting)
            .map(|(key, workflow, base)| SavedWorkflowDraft {
                root: key.root.clone(),
                id: key.id.clone(),
                workflow,
                base,
            })
            .collect();
        drafts.sort_by(|a, b| (&a.root, &a.id).cmp(&(&b.root, &b.id)));
        drafts
    }

    /// Takes back the unsaved workflow edits of last time.
    pub(super) fn restore_workflow_drafts(&mut self, saved: &[SavedWorkflowDraft]) {
        for d in saved {
            let key = WorkflowKey {
                root: d.root.clone(),
                id: d.id.clone(),
            };
            let mut workflow = d.workflow.clone();
            workflow.id.clone_from(&d.id);
            let base = d.base.clone().map(|mut b| {
                b.id.clone_from(&d.id);
                b
            });
            self.workflows.waiting.insert(key, (workflow, base));
        }
        self.check_waiting_workflows();
    }

    /// Opens waiting edits once their project is open. If the file changed on disk
    /// meanwhile the edits are kept and the user is told (Revert takes the file); if
    /// it is gone, they are dropped with a notice; edits equal to the file are dropped.
    pub(super) fn check_waiting_workflows(&mut self) {
        let ready: Vec<WorkflowKey> = self
            .workflows
            .waiting
            .keys()
            .filter(|k| self.state.collection(&k.root).is_some())
            .cloned()
            .collect();
        let mut notes = Vec::new();
        for key in ready {
            let Some((draft, base)) = self.workflows.waiting.remove(&key) else {
                continue;
            };
            let Some(saved) = self.saved_workflow(&key) else {
                notes.push(t!("workflow-deleted-drafts-dropped", name = &draft.name));
                continue;
            };
            if saved == draft {
                continue;
            }
            if base.as_ref() != Some(&saved) {
                notes.push(format!(
                    "The workflow “{}” changed on disk while it had unsaved edits. \
                     Your edits are kept; Revert takes the file's version.",
                    draft.name
                ));
            }
            let st = WorkflowEditorState::new(&draft);
            self.workflows.editors.insert(
                key,
                OpenWorkflow {
                    draft,
                    base: Some(saved),
                    st,
                    run: None,
                    run_id: 0,
                    abort: None,
                },
            );
        }
        if !notes.is_empty() {
            self.state.notify(notes.join("\n"));
        }
    }

    /// Workflows with unsaved edits, for the sidebar's dot (R-FLOW-5).
    pub(super) fn dirty_workflows(&self) -> HashSet<WorkflowKey> {
        self.workflows
            .editors
            .keys()
            .filter(|k| self.workflow_dirty(k))
            .cloned()
            .collect()
    }

    pub(super) fn workflow_dirty(&self, key: &WorkflowKey) -> bool {
        let saved = self.saved_workflow(key);
        self.workflows
            .editors
            .get(key)
            .is_some_and(|o| saved.as_ref() != Some(&o.draft))
    }

    /// Saves the open workflow (Ctrl+S).
    pub(super) fn save_open_workflow(&mut self) {
        let Some(key) = self.workflows.open.clone() else {
            return;
        };
        let Some(draft) = self.workflows.editors.get(&key).map(|o| o.draft.clone()) else {
            return;
        };
        self.state.run(&key.root, |c, s| c.save_workflow(s, &draft));
    }

    /// The main area while a workflow is open: the editor above, results below.
    pub(super) fn workflow_page(&mut self, ui: &mut Ui) {
        let Some(key) = self.workflows.open.clone() else {
            return;
        };
        let Some(c) = self.state.collection(&key.root) else {
            self.workflows.open = None;
            return;
        };
        let requests = request_choices(c);
        let read_only = c.read_only;
        let dirty = self.workflow_dirty(&key);
        let can_pick_files = self.state.host.can_pick_folders();
        let Some(open) = self.workflows.editors.get_mut(&key) else {
            return;
        };
        let running = open.run.as_ref().is_some_and(|r| r.running);
        let action = Panel::top("workflow-editor")
            .frame(Frame::new().inner_margin(Margin {
                bottom: 12,
                ..Margin::ZERO
            }))
            .resizable(true)
            .default_size(320.0)
            .size_range(160.0..=f32::INFINITY)
            .show(ui, |ui| {
                ui.set_min_height(ui.available_height());
                let cx = WorkflowContext {
                    requests: &requests,
                    read_only,
                    dirty,
                    running,
                    can_pick_files,
                };
                workflow_editor(ui, &mut open.draft, &mut open.st, cx)
            })
            .inner;
        ui.add_space(10.0);
        workflow_results(ui, open.run.as_ref(), &mut open.st);
        if let Some(action) = action {
            self.on_workflow_action(&key, action, ui.ctx());
        }
    }

    fn on_workflow_action(
        &mut self,
        key: &WorkflowKey,
        action: WorkflowAction,
        ctx: &egui::Context,
    ) {
        match action {
            WorkflowAction::Save => self.save_open_workflow(),
            WorkflowAction::Revert => {
                if let (Some(saved), Some(open)) = (
                    self.saved_workflow(key),
                    self.workflows.editors.get_mut(key),
                ) {
                    open.st = WorkflowEditorState::new(&saved);
                    open.draft = saved;
                }
            }
            WorkflowAction::Run => self.run_workflow(key, ctx),
            WorkflowAction::Cancel => {
                if let Some(open) = self.workflows.editors.get_mut(key) {
                    if let Some(abort) = open.abort.take() {
                        abort.abort();
                    }
                    if let Some(run) = &mut open.run {
                        run.running = false;
                        run.current = None;
                        run.cancelled = true;
                    }
                }
            }
            WorkflowAction::ChooseData => {
                if let Some(path) = self.state.host.pick_any_file(&t!("workflow-choose-data"))
                    && let Some(open) = self.workflows.editors.get_mut(key)
                {
                    open.draft.data = Some(upload_path(&path, Some(&key.root)));
                }
            }
            WorkflowAction::OpenRequest(path) => {
                self.workflows.open = None;
                self.select(Some(ItemKey::new(&key.root, path)));
            }
        }
    }

    /// Runs the open draft of a workflow in the background (R-FLOW-3, R-FLOW-4).
    /// Runs a workflow; against a protected environment it asks first when a step may
    /// change data (R-SRV-9).
    pub(super) fn run_workflow(&mut self, key: &WorkflowKey, egui_ctx: &egui::Context) {
        self.open_workflow(key.clone());
        let draft = self.workflows.editors.get(key).map(|o| &o.draft);
        let writes = match (draft, self.state.collection(&key.root)) {
            (Some(draft), Some(c)) => draft.writes(c),
            _ => Vec::new(),
        };
        let env = self.state.protected_environment(&key.root);
        let (Some(env), false) = (env, writes.is_empty()) else {
            return self.start_workflow(key, egui_ctx);
        };
        let name = draft.map_or_else(String::new, |d| d.name.clone());
        let dialog = ConfirmDialog {
            title: t!(
                "workflow-protected-title",
                name = name.as_str(),
                env = env.as_str()
            ),
            message: t!(
                "workflow-protected-message",
                env = env.as_str(),
                steps = writes.join(", ")
            ),
            confirm_label: t!("workflow-run"),
        };
        self.modal = Some(Modal::Protected(Guarded::RunWorkflow(key.clone()), dialog));
    }

    /// Runs a workflow, without asking.
    pub(super) fn start_workflow(&mut self, key: &WorkflowKey, egui_ctx: &egui::Context) {
        self.open_workflow(key.clone());
        let Some((collection, environment)) = self.state.collection_for_run(&key.root) else {
            return;
        };
        let Some(draft) = self.workflows.editors.get(key).map(|o| o.draft.clone()) else {
            return;
        };
        let rows = match &draft.data {
            None => Vec::new(),
            Some(file) => {
                let path = key.root.join(file);
                let read = self
                    .state
                    .storage()
                    .read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|text| data::parse(file, &text));
                match read {
                    Ok(rows) => rows,
                    Err(e) => {
                        self.state.notify(t!(
                            "workflow-data-unreadable",
                            file = file.as_str(),
                            error = e
                        ));
                        return;
                    }
                }
            }
        };
        // Secret values are those of the project's own environment, also on the mock.
        let secrets = self
            .state
            .secrets
            .get(&key.root)
            .map(|s| s.for_environment(self.state.active_environment(&key.root).as_deref()))
            .unwrap_or_default();
        let options = RunOptions {
            environment,
            secrets,
            overrides: BTreeMap::new(),
            workspace: self.state.workspace_values(),
            workspace_defaults: self.state.workspace().defaults.clone(),
            rows,
            // Workflows pass and fail as in CI, where checking is opt-in (R-MOCK-8, D47).
            check_examples: false,
        };
        self.workflows.next_run += 1;
        let run_id = self.workflows.next_run;
        let client = self.state.client.clone();
        let tx = self.workflow_events.sender();
        let repaint = egui_ctx.clone();
        let event_key = key.clone();
        let iterations_planned = options.rows.len().max(1);
        let (task, abort) = abortable(async move {
            let send = |kind| {
                // The receiver only goes away with the app.
                let _ = tx.send(WorkflowEvent {
                    key: event_key.clone(),
                    run_id,
                    kind,
                });
                repaint.request_repaint();
            };
            let result = workflow::run(&client, &collection, &draft, &options, |p| match p {
                Progress::Started { iteration, step } => {
                    send(EventKind::Started { iteration, step });
                }
                Progress::Finished { iteration, step } => send(EventKind::Step {
                    iteration,
                    view: Box::new(StepView::new(step)),
                }),
            })
            .await;
            send(EventKind::Done {
                ends: result
                    .iterations
                    .iter()
                    .map(|i| (i.skipped, i.ended))
                    .collect(),
            });
        });
        self.state.host.spawn(Box::pin(async move {
            let _ = task.await;
        }));
        if let Some(open) = self.workflows.editors.get_mut(key) {
            if let Some(old) = open.abort.replace(abort) {
                old.abort();
            }
            open.run_id = run_id;
            open.st.selected_result = None;
            open.run = Some(RunView {
                running: true,
                steps_per_iteration: open.draft.steps.len(),
                iterations_planned,
                ..RunView::default()
            });
        }
    }

    /// Takes in progress from running workflows.
    pub(super) fn collect_workflow_events(&mut self) {
        while let Some(event) = self.workflow_events.try_recv() {
            let Some(open) = self.workflows.editors.get_mut(&event.key) else {
                continue;
            };
            if open.run_id != event.run_id {
                continue;
            }
            // A cancelled run may still have events on the way.
            let Some(run) = open.run.as_mut().filter(|r| r.running) else {
                continue;
            };
            match event.kind {
                EventKind::Started { iteration, step } => {
                    while run.iterations.len() <= iteration {
                        run.iterations.push(IterationView::default());
                    }
                    run.current = Some((iteration, step));
                }
                EventKind::Step { iteration, view } => {
                    if let Some(it) = run.iterations.get_mut(iteration) {
                        it.steps.push(*view);
                    }
                }
                EventKind::Done { ends } => {
                    for (it, (skipped, ended)) in run.iterations.iter_mut().zip(ends) {
                        it.skipped = skipped;
                        it.ended = ended;
                    }
                    run.running = false;
                    run.current = None;
                    open.abort = None;
                }
            }
        }
    }

    /// Sidebar actions about workflows and runtime variables.
    pub(super) fn on_workflow_sidebar(&mut self, action: SidebarAction, ctx: &egui::Context) {
        match action {
            SidebarAction::NewWorkflow(root) => self.new_workflow_dialog(root),
            SidebarAction::SelectWorkflow(key) => self.open_workflow(key),
            SidebarAction::RunWorkflow(key) => self.run_workflow(&key, ctx),
            SidebarAction::RenameWorkflow(key) => self.rename_workflow_dialog(key),
            SidebarAction::DeleteWorkflow(key) => self.confirm_delete_workflow(key),
            SidebarAction::RuntimeVariables(root) => self.show_runtime_vars(Some(root)),
            _ => {}
        }
    }

    /// Dialogs about workflows and runtime variables. Returns whether it stays open.
    pub(super) fn show_workflow_modal(&mut self, ctx: &egui::Context, modal: &mut Modal) -> bool {
        match modal {
            Modal::NewWorkflow(root, d) => match d.show(ctx) {
                DialogResult::Done(name) => {
                    let root = root.clone();
                    self.create_workflow(&root, &name);
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::RenameWorkflow(key, d) => match d.show(ctx) {
                DialogResult::Done(name) => {
                    let key = key.clone();
                    self.rename_workflow(&key, &name);
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::DeleteWorkflow(key, d) => match d.show(ctx) {
                DialogResult::Done(()) => {
                    let key = key.clone();
                    self.delete_workflow(&key);
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::RuntimeVars(d) => {
                let d = d.clone();
                self.show_runtime_vars_dialog(ctx, &d)
            }
            _ => false,
        }
    }

    // ------------------------------------------------------------ runtime variables (R-TEST-5)

    pub(super) fn show_runtime_vars(&mut self, root: Option<PathBuf>) {
        let place = root
            .as_ref()
            .and_then(|r| self.state.collection(r))
            .map_or_else(|| t!("scratch-request"), |c| c.name().to_owned());
        self.modal = Some(Modal::RuntimeVars(RuntimeVarsDialog::new(root, &place)));
    }

    pub(super) fn show_runtime_vars_dialog(
        &mut self,
        ctx: &egui::Context,
        d: &RuntimeVarsDialog,
    ) -> bool {
        let vars = self.state.runtime_vars(d.root.as_deref()).cloned();
        match d.show(ctx, vars.as_ref()) {
            DialogResult::Done(()) => {
                self.state.clear_runtime(d.root.as_deref());
                false
            }
            result => result == DialogResult::Open,
        }
    }
}

/// The project's requests, in tree order, for "Add step".
fn request_choices(c: &tortuga_core::Collection) -> Vec<RequestChoice> {
    fn walk(items: &[Item], place: &str, out: &mut Vec<RequestChoice>) {
        for item in items {
            match item {
                Item::Request(r) => out.push(RequestChoice {
                    path: r.path.clone(),
                    method: r.request.method,
                    name: r.request.name.clone(),
                    place: place.to_owned(),
                }),
                Item::Folder(f) => {
                    let sub = if place.is_empty() {
                        f.name.clone()
                    } else {
                        format!("{place} › {}", f.name)
                    };
                    walk(&f.items, &sub, out);
                }
                Item::Broken(_) => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&c.items, "", &mut out);
    out
}

#[cfg(test)]
mod tests {
    use tortuga_core::storage::FsStorage;
    use tortuga_core::workflow::Step;
    use tortuga_core::{Collection, HttpMethod, Request};

    use super::*;
    use crate::test_host::TestHost;

    /// R-FLOW-2, R-TEST-5: a workflow is created, edited, renamed with its edits kept,
    /// saved and deleted; runtime variables are listed per project and cleared.
    #[test]
    fn workflows_are_created_renamed_saved_and_deleted() {
        let root =
            std::env::temp_dir().join(format!("tortuga-ui-workflows-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let c = Collection::create(&FsStorage, &root, "C").expect("create");
        let manifest = tortuga_core::Manifest {
            variables: [("token".to_owned(), "from-project".to_owned())].into(),
            ..c.manifest.clone()
        };
        c.save_config(&FsStorage, &manifest, &c.environments)
            .expect("config");
        let mut request = Request::new(HttpMethod::Get, "https://a.test/{{token}}/{{fresh}}");
        request.name = "Get".into();
        let path = c
            .new_request(&FsStorage, Path::new(""), &request)
            .expect("request");
        let mut ui = TortugaUi::new(Box::new(TestHost {
            open: vec![root.clone()],
            ..TestHost::default()
        }));

        ui.create_workflow(&root, "Smoke test");
        let key = ui.workflows.open.clone().expect("opened");
        assert_eq!(key.id, "smoke-test");
        let open = ui.workflows.editors.get_mut(&key).expect("editor");
        open.draft.steps.push(Step::new(&path));
        assert!(ui.workflow_dirty(&key));

        ui.rename_workflow(&key, "Smoke");
        assert_eq!(ui.saved_workflow(&key).expect("saved").name, "Smoke");
        assert_eq!(ui.saved_workflow(&key).expect("saved").steps, []);
        let draft = &ui.workflows.editors[&key].draft;
        assert_eq!((draft.name.as_str(), draft.steps.len()), ("Smoke", 1));

        ui.save_open_workflow();
        assert!(!ui.workflow_dirty(&key));
        let text =
            std::fs::read_to_string(root.join("workflows").join("smoke-test.toml")).expect("file");
        assert!(text.contains("name = \"Smoke\"") && text.contains("request = \"get.toml\""));

        // R-TEST-5: runtime values win over the project's and count as defined.
        let item = ItemKey::new(&root, path.clone());
        let undefined = |ui: &TortugaUi| {
            tortuga_core::vars::undefined_in(&request, &ui.state.request_context(Some(&item)))
        };
        assert_eq!(undefined(&ui), ["fresh"]);
        let changes = [
            ("token".to_owned(), Some("abc".to_owned())),
            ("fresh".to_owned(), Some("1".to_owned())),
        ]
        .into();
        ui.state.apply_runtime(Some(&root), &changes);
        let ctx = ui.state.request_context(Some(&item));
        assert_eq!(ctx.variables.get("token").map(String::as_str), Some("abc"));
        assert_eq!(undefined(&ui), Vec::<String>::new());
        ui.state.clear_runtime(Some(&root));
        assert!(ui.state.runtime_vars(Some(&root)).is_none());
        assert_eq!(undefined(&ui), ["fresh"]);

        // R-FLOW-3: Cancel on the page stops a run.
        ui.workflows.editors.get_mut(&key).expect("editor").run = Some(RunView {
            running: true,
            steps_per_iteration: 1,
            iterations_planned: 1,
            ..RunView::default()
        });
        let shared = std::rc::Rc::new(std::cell::RefCell::new(ui));
        {
            let ui = std::rc::Rc::clone(&shared);
            let mut h = crate::test_host::harness(move |egui_ui| ui.borrow_mut().ui(egui_ui));
            h.run_steps(3);
            egui_kittest::kittest::Queryable::get_by_label(&h, "Cancel").click();
            h.run_steps(3);
        }
        let Ok(cell) = std::rc::Rc::try_unwrap(shared) else {
            panic!("harness dropped")
        };
        let mut ui = cell.into_inner();
        let run = ui.workflows.editors[&key].run.as_ref().expect("run");
        assert!(run.cancelled && !run.running);

        ui.delete_workflow(&key);
        assert!(ui.workflows.open.is_none());
        assert!(!root.join("workflows").join("smoke-test.toml").exists());
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-FLOW-5: unsaved workflow edits survive a restart; a file changed meanwhile
    /// keeps the edits with a notice, a deleted one drops them with a notice.
    #[test]
    fn unsaved_workflow_edits_survive_a_restart() {
        let root =
            std::env::temp_dir().join(format!("tortuga-ui-workflow-drafts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let c = Collection::create(&FsStorage, &root, "C").expect("create");
        let request = Request::new(HttpMethod::Get, "https://a.test/");
        let path = c
            .new_request(&FsStorage, Path::new(""), &request)
            .expect("request");
        let drafts = std::rc::Rc::new(std::cell::RefCell::new(crate::host::SavedDrafts::default()));
        let start = || {
            TortugaUi::new(Box::new(TestHost {
                open: vec![root.clone()],
                drafts: std::rc::Rc::clone(&drafts),
                ..TestHost::default()
            }))
        };
        let edit = |ui: &mut TortugaUi, key: &WorkflowKey| {
            ui.open_workflow(key.clone());
            let open = ui.workflows.editors.get_mut(key).expect("editor");
            open.draft.steps.push(Step::new(&path));
        };

        let mut ui = start();
        ui.create_workflow(&root, "Smoke");
        ui.create_workflow(&root, "Gone");
        let smoke = WorkflowKey {
            root: root.clone(),
            id: "smoke".into(),
        };
        let gone = WorkflowKey {
            root: root.clone(),
            id: "gone".into(),
        };
        edit(&mut ui, &smoke);
        edit(&mut ui, &gone);
        assert_eq!(ui.dirty_workflows().len(), 2);
        ui.on_exit();
        assert_eq!(drafts.borrow().workflows.len(), 2);

        // Unchanged on disk: the edits come back as they were, without a notice.
        let mut ui = start();
        assert!(ui.workflow_dirty(&smoke) && ui.workflow_dirty(&gone));
        assert_eq!(ui.workflows.editors[&smoke].draft.steps.len(), 1);
        assert_eq!(ui.workflows.editors[&smoke].draft.id, "smoke");
        assert!(ui.state.notice.is_none());
        ui.on_exit();

        // While closed, one file changes and the other is deleted.
        let mut changed = c_workflow(&root, "smoke");
        changed.continue_on_failure = true;
        let c = Collection::open(&FsStorage, &root).expect("open");
        c.save_workflow(&FsStorage, &changed).expect("save");
        c.delete_workflow(&FsStorage, "gone").expect("delete");
        let mut ui = start();
        let open = &ui.workflows.editors[&smoke];
        assert_eq!(open.draft.steps.len(), 1);
        assert!(!open.draft.continue_on_failure);
        assert!(!ui.workflows.editors.contains_key(&gone));
        let notice = ui.state.notice.clone().expect("notice");
        assert!(notice.contains("“Smoke” changed on disk"), "{notice}");
        assert!(notice.contains("“Gone” was deleted"), "{notice}");

        // Saving makes it clean; nothing is kept for next time.
        ui.open_workflow(smoke.clone());
        ui.save_open_workflow();
        ui.on_exit();
        assert_eq!(drafts.borrow().workflows, []);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    fn c_workflow(root: &Path, id: &str) -> Workflow {
        Collection::open(&FsStorage, root)
            .expect("open")
            .workflow(id)
            .expect("workflow")
            .clone()
    }

    /// R-SET-11: the open workflow is saved like a request.
    #[test]
    fn workflow_edits_are_saved_too() {
        let (mut ui, root, key) = crate::shell::tests::setup("auto-save-workflow");
        ui.state.update_settings(|s| s.editing.auto_save = true);
        ui.create_workflow(&root, "Smoke");
        let wf = ui.workflows.open.clone().expect("opened");
        let open = ui.workflows.editors.get_mut(&wf).expect("editor");
        open.draft.steps.push(Step::new(&key.path));
        crate::shell::auto_save::tests::run(&mut ui, 1.5, false);
        assert!(!ui.workflow_dirty(&wf));
        let text =
            std::fs::read_to_string(root.join("workflows").join("smoke.toml")).expect("file");
        assert!(text.contains("[[steps]]"), "{text}");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// Marks the project's only environment protected, and makes it the active one.
    fn protect(ui: &mut TortugaUi, root: &Path) {
        use tortuga_core::format::Environment;
        let production = Environment {
            id: "production".into(),
            name: "Production".into(),
            protected: true,
            ..Environment::default()
        };
        ui.state
            .run(root, |c, s| c.save_config(s, &c.manifest, &[production]));
    }

    /// R-SRV-9: to a protected environment a POST waits for a yes, a GET goes at once;
    /// a workflow with a step that writes asks before it runs.
    #[test]
    fn protected_environments_ask_before_writes() {
        let (mut ui, root, key) = crate::shell::tests::setup("protected-send");
        protect(&mut ui, &root);
        assert_eq!(
            ui.state.protected_environment(&root).as_deref(),
            Some("Production")
        );
        let ctx = egui::Context::default();
        ui.send(&ctx);
        assert!(ui.modal.is_none() && ui.in_flight.is_some(), "a GET goes");

        ui.in_flight = None;
        ui.current.method = HttpMethod::Delete;
        ui.send(&ctx);
        let Some(crate::shell::Modal::Protected(crate::shell::Guarded::Send, d)) = &ui.modal else {
            panic!("asks first: {:?}", ui.modal.is_some())
        };
        assert_eq!(d.title, "Send DELETE to “Production”?");
        assert!(ui.in_flight.is_none(), "nothing sent yet");

        ui.modal = None;
        ui.current.method = HttpMethod::Get;
        ui.save();
        let wf = {
            ui.create_workflow(&root, "Smoke");
            ui.workflows.open.clone().expect("opened")
        };
        ui.run_workflow(&wf, &ctx);
        assert!(ui.modal.is_none(), "no steps that write: no question");
        let mut post = Request::new(HttpMethod::Post, "https://a.test/orders");
        post.name = "Create order".into();
        let path = ui
            .state
            .run(&root, |c, s| c.new_request(s, Path::new(""), &post))
            .expect("request");
        let c = Collection::open(&FsStorage, &root).expect("open");
        let mut saved = c.workflow(&wf.id).expect("workflow").clone();
        saved.steps = vec![Step::new(&path)];
        ui.state.run(&root, |c, s| c.save_workflow(s, &saved));
        ui.workflows.editors.clear();
        ui.run_workflow(&wf, &ctx);
        let Some(crate::shell::Modal::Protected(crate::shell::Guarded::RunWorkflow(_), d)) =
            &ui.modal
        else {
            panic!("asks before the run")
        };
        assert!(d.message.contains("POST Create order"), "{}", d.message);
        let _ = key;
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
