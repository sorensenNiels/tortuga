//! "New WFS service…" (R-GEO-1): the endpoint, version and project server, the layers
//! the service's capabilities list, and which of them to create requests for.

use std::path::PathBuf;

use egui::{Context, RichText, ScrollArea, TextEdit, Ui};
use futures_util::future::AbortHandle;
use tortuga_core::Auth;
use tortuga_core::geoserver::{Capabilities, DEFAULT_SERVER, Endpoint, Version};
use tortuga_core::vars::Origin;

use crate::components::{
    DialogResult, VarHelp, actions, auth_fields, describe_auth, dialog, field_label, finish,
    kind_picker,
};
use crate::theme;
use tortuga_core::format::names_match;

/// What the user asked of the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WfsAction {
    /// Send GetCapabilities to the endpoint.
    Fetch,
    /// Write the requests for the chosen layers.
    Create,
}

/// The endpoint and version a fetch was for: the list goes stale when either changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FetchedFor {
    pub endpoint: String,
    pub version: Version,
}

/// Where fetching the capabilities is.
#[derive(Debug)]
pub(crate) enum Fetch {
    NotYet,
    Fetching {
        id: u64,
        abort: AbortHandle,
    },
    Failed(String),
    Fetched {
        caps: Capabilities,
        /// Ticked or not, per feature type.
        chosen: Vec<bool>,
        what: FetchedFor,
    },
}

/// Which project server the requests use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ServerPick {
    /// One the project has, by name.
    Existing(String),
    /// A new one, named in the dialog.
    New,
}

/// What the dialog shows about the project.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WfsView<'a> {
    /// The names of the project's servers.
    pub servers: &'a [String],
    /// The chosen environment's name, if one is chosen (for the whole workspace).
    pub environment: Option<&'a str>,
    /// Whether the project has its own environment of that name.
    pub own_environment: bool,
    /// The URL the picked server has now in the chosen environment.
    pub server_url: Option<&'a str>,
    /// The auth the service folder would inherit, and where from.
    pub inherited: Option<(&'a Auth, &'a Origin)>,
    /// Variables the inherited auth uses that are not defined (R-VAR-5).
    pub inherited_undefined: &'a [String],
    /// What the auth fields can tell about the variables they use.
    pub help: &'a VarHelp,
}

#[derive(Debug)]
pub(crate) struct WfsDialog {
    pub root: PathBuf,
    /// The folder the requests go in; empty for the project itself.
    pub parent: PathBuf,
    title: String,
    pub endpoint: String,
    /// Where the server's URL ends, if moved from the default split (D81).
    pub base: Option<String>,
    pub version: Version,
    pub server: ServerPick,
    /// The name of a new server.
    pub new_server: String,
    /// The service folder's auth, also sent to fetch the layers; `None` inherits.
    pub auth: Option<Auth>,
    pub fetch: Fetch,
    filter: String,
    /// The layer list's height last frame.
    list_height: f32,
}

impl WfsDialog {
    /// `servers`: the names of the project's servers. One called "GeoServer" is picked;
    /// otherwise a new one of that name.
    pub fn new(root: PathBuf, parent: PathBuf, place: &str, servers: &[String]) -> Self {
        let server = servers
            .iter()
            .find(|n| names_match(n, DEFAULT_SERVER))
            .map_or(ServerPick::New, |n| ServerPick::Existing(n.clone()));
        Self {
            root,
            parent,
            title: t!("wfs-title", place = place),
            endpoint: String::new(),
            base: None,
            version: Version::default(),
            server,
            new_server: DEFAULT_SERVER.to_owned(),
            auth: None,
            fetch: Fetch::NotYet,
            filter: String::new(),
            list_height: 0.0,
        }
    }

    /// What a fetch now would be for.
    pub fn current(&self) -> FetchedFor {
        FetchedFor {
            endpoint: self.endpoint.trim().to_owned(),
            version: self.version,
        }
    }

    /// The endpoint split into the server's URL and the requests' path: where the user
    /// moved the split, else the default (D81).
    pub fn split(&self) -> Option<Endpoint> {
        let chosen = self.base.as_ref().and_then(|base| {
            Endpoint::splits(&self.endpoint)
                .into_iter()
                .find(|s| s.base == *base)
        });
        chosen.or_else(|| Endpoint::parse(&self.endpoint))
    }

    /// The name of the server the requests use.
    pub fn server_name(&self) -> &str {
        match &self.server {
            ServerPick::Existing(name) => name,
            ServerPick::New => self.new_server.trim(),
        }
    }

    /// Why Create can't use the server: a new one without a name, or with the name of
    /// one the project has.
    fn server_problem(&self, servers: &[String]) -> Option<String> {
        match &self.server {
            ServerPick::Existing(_) => None,
            ServerPick::New if self.new_server.trim().is_empty() => {
                Some(t!("wfs-server-needs-name"))
            }
            ServerPick::New => servers
                .iter()
                .find(|n| names_match(n, &self.new_server))
                .map(|n| t!("wfs-server-exists", server = n.as_str())),
        }
    }

    /// The capabilities and the names of the chosen layers, if the list is up to date.
    pub fn chosen(&self) -> Option<(&Capabilities, Vec<usize>)> {
        match &self.fetch {
            Fetch::Fetched { caps, chosen, what } if *what == self.current() => Some((
                caps,
                chosen
                    .iter()
                    .enumerate()
                    .filter_map(|(i, on)| on.then_some(i))
                    .collect(),
            )),
            _ => None,
        }
    }

    pub fn show(
        &mut self,
        ctx: &Context,
        view: WfsView<'_>,
    ) -> (DialogResult<()>, Option<WfsAction>) {
        let mut action = None;
        let result = dialog(ctx, "wfs", &self.title.clone(), 620.0, false, |ui| {
            theme::hint(ui, t!("wfs-hint"));
            if self.fields(ui, view.servers) {
                action = Some(WfsAction::Fetch);
            }
            self.server_line(ui, view);
            self.auth(ui, view);
            ui.add_space(8.0);
            if self.layers(ui) {
                action = Some(WfsAction::Fetch);
            }
            let ready = self.chosen().is_some() && self.server_problem(view.servers).is_none();
            let label = match self.chosen() {
                Some((_, chosen)) => t!("wfs-create", count = chosen.len()),
                None => t!("create"),
            };
            let (cancel, confirm) = actions(ui, theme::primary_button(ui, label), ready);
            if confirm {
                action = Some(WfsAction::Create);
            }
            finish(cancel, None)
        });
        (result, action)
    }

    /// Endpoint, version and server name. Returns whether Enter in the endpoint asked to
    /// fetch.
    fn fields(&mut self, ui: &mut Ui, servers: &[String]) -> bool {
        let id = field_label(ui, &t!("wfs-endpoint"));
        let endpoint = ui
            .add(
                TextEdit::singleline(&mut self.endpoint)
                    .id_salt("wfs-endpoint")
                    .hint_text("https://maps.example.com/geoserver/wfs")
                    .desired_width(f32::INFINITY),
            )
            .labelled_by(id);
        let enter = endpoint.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        self.split_field(ui);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                let id = field_label(ui, &t!("wfs-version"));
                egui::ComboBox::from_id_salt("wfs-version")
                    .selected_text(self.version.as_str())
                    .show_ui(ui, |ui| {
                        for v in Version::ALL {
                            ui.selectable_value(&mut self.version, v, v.as_str());
                        }
                    })
                    .response
                    .labelled_by(id);
            });
            ui.add_space(16.0);
            ui.vertical(|ui| self.server_field(ui, servers));
        });
        enter && !self.endpoint.trim().is_empty()
    }

    /// Where the server's URL ends: after the host, a segment of the path, or the whole
    /// endpoint (D81).
    fn split_field(&mut self, ui: &mut Ui) {
        let splits = Endpoint::splits(&self.endpoint);
        let Some(current) = self.split() else {
            return;
        };
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let id = field_label(ui, &t!("wfs-server-url"));
            egui::ComboBox::from_id_salt("wfs-server-url")
                .selected_text(current.base.as_str())
                .width(ui.available_width().min(420.0))
                .show_ui(ui, |ui| {
                    for split in &splits {
                        if ui
                            .selectable_label(split.base == current.base, split.base.as_str())
                            .clicked()
                        {
                            self.base = Some(split.base.clone());
                        }
                    }
                })
                .response
                .labelled_by(id)
                .on_hover_text(t!("wfs-server-url-hint"));
        });
    }

    /// The project server: one the project has, or a new one with a name.
    fn server_field(&mut self, ui: &mut Ui, servers: &[String]) {
        let id = field_label(ui, &t!("wfs-server"));
        let shown = match &self.server {
            ServerPick::Existing(name) => name.clone(),
            ServerPick::New => t!("wfs-server-new"),
        };
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("wfs-server")
                .selected_text(shown)
                .width(180.0)
                .show_ui(ui, |ui| {
                    for name in servers {
                        ui.selectable_value(
                            &mut self.server,
                            ServerPick::Existing(name.clone()),
                            name.as_str(),
                        );
                    }
                    ui.selectable_value(&mut self.server, ServerPick::New, t!("wfs-server-new"));
                })
                .response
                .labelled_by(id)
                .on_hover_text(t!("wfs-server-hint"));
            if self.server == ServerPick::New {
                ui.add(
                    TextEdit::singleline(&mut self.new_server)
                        .id_salt("wfs-new-server")
                        .hint_text(t!("wfs-server-name"))
                        .desired_width(160.0),
                );
            }
        });
    }

    /// What Create does with the server's URL in the chosen environment, and where the
    /// requests go.
    fn server_line(&self, ui: &mut Ui, view: WfsView<'_>) {
        ui.add_space(4.0);
        if let Some(problem) = self.server_problem(view.servers) {
            theme::warning(ui, problem);
            return;
        }
        let Some(endpoint) = self.split() else {
            return;
        };
        let server = self.server_name();
        let base = endpoint.base.as_str();
        match view.environment {
            Some(env) => match view.server_url {
                Some(now) if now.trim_end_matches('/') == base => theme::hint(
                    ui,
                    t!(
                        "wfs-server-same",
                        environment = env,
                        server = server,
                        base = base
                    ),
                ),
                Some(now) => theme::warning(
                    ui,
                    t!(
                        "wfs-server-changes",
                        environment = env,
                        server = server,
                        now = now,
                        base = base
                    ),
                ),
                None => theme::hint(
                    ui,
                    t!(
                        "wfs-server-set",
                        environment = env,
                        server = server,
                        base = base
                    ),
                ),
            },
            None => theme::hint(
                ui,
                t!("wfs-server-no-environment", server = server, base = base),
            ),
        }
        if let Some(env) = view.environment
            && !view.own_environment
        {
            theme::hint(ui, t!("wfs-server-own-environment", environment = env));
        }
        let path = if endpoint.path.is_empty() {
            "/"
        } else {
            endpoint.path.as_str()
        };
        theme::hint(ui, t!("wfs-server-path", path = path, server = server));
    }

    /// The service folder's auth, which fetching the layers sends too (R-GEO-1).
    fn auth(&mut self, ui: &mut Ui, view: WfsView<'_>) {
        ui.add_space(8.0);
        theme::section_label(ui, &t!("wfs-auth"));
        let from = view
            .inherited
            .map(|(_, origin)| crate::format::origin(origin));
        let inherit = view
            .inherited
            .zip(from.as_deref())
            .map(|((_, origin), from)| {
                if *origin == Origin::Project {
                    t!("auth-inherit-project")
                } else {
                    t!("auth-inherit-from", origin = from)
                }
            });
        kind_picker(ui, &mut self.auth, inherit.as_deref());
        ui.add_space(4.0);
        match (self.auth.as_mut(), view.inherited.zip(from)) {
            (Some(auth), _) => {
                auth_fields(
                    ui,
                    "wfs-auth",
                    auth,
                    view.help,
                    Some(&t!("defaults-file-folder")),
                );
            }
            (None, Some(((auth, _), from))) => {
                theme::hint(
                    ui,
                    t!(
                        "wfs-auth-inherited",
                        origin = from.as_str(),
                        auth = describe_auth(auth)
                    ),
                );
                if !view.inherited_undefined.is_empty() {
                    let names = view
                        .inherited_undefined
                        .iter()
                        .map(|n| format!("{{{{{n}}}}}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    theme::warning(ui, t!("wfs-auth-undefined", names = names));
                }
            }
            (None, None) => theme::hint(ui, t!("wfs-auth-none")),
        }
    }

    /// The Fetch button and the list of layers. Returns whether Fetch was clicked.
    fn layers(&mut self, ui: &mut Ui) -> bool {
        let stale = !matches!(&self.fetch, Fetch::Fetched { what, .. } if *what == self.current());
        let mut fetch = false;
        ui.horizontal(|ui| {
            let can = !self.endpoint.trim().is_empty();
            let fetching = matches!(self.fetch, Fetch::Fetching { .. });
            let label = if matches!(self.fetch, Fetch::Fetched { .. }) {
                t!("wfs-fetch-again")
            } else {
                t!("wfs-fetch")
            };
            let button = if stale {
                theme::primary_button(ui, label)
            } else {
                egui::Button::new(label)
            };
            if ui.add_enabled(can && !fetching, button).clicked() {
                fetch = true;
            }
            if fetching {
                ui.spinner();
                theme::hint(ui, t!("wfs-fetching"));
            }
        });
        let current = self.current();
        let filter = &mut self.filter;
        match &mut self.fetch {
            Fetch::Failed(message) => theme::error(ui, message.as_str()),
            Fetch::Fetched { caps, chosen, what } => {
                if *what != current {
                    theme::warning(ui, t!("wfs-stale"));
                }
                ui.add_space(6.0);
                layer_list(ui, caps, chosen, filter, &mut self.list_height);
            }
            Fetch::NotYet | Fetch::Fetching { .. } => {}
        }
        fetch
    }
}

/// The service's layers with a checkbox each, a filter box and All/None.
fn layer_list(
    ui: &mut Ui,
    caps: &Capabilities,
    chosen: &mut [bool],
    filter: &mut String,
    last_height: &mut f32,
) {
    let p = theme::palette(ui);
    let n = chosen.iter().filter(|c| **c).count();
    ui.horizontal(|ui| {
        theme::section_label(
            ui,
            &t!(
                "wfs-layers",
                title = caps.title.as_str(),
                chosen = n,
                count = caps.feature_types.len()
            ),
        );
    });
    if caps.feature_types.is_empty() {
        theme::hint(ui, t!("wfs-no-layers"));
        return;
    }
    let needle = filter.trim().to_lowercase();
    let shown: Vec<usize> = caps
        .feature_types
        .iter()
        .enumerate()
        .filter(|(_, ft)| {
            needle.is_empty()
                || ft.name.to_lowercase().contains(&needle)
                || ft.title.to_lowercase().contains(&needle)
        })
        .map(|(i, _)| i)
        .collect();
    ui.horizontal(|ui| {
        ui.add(
            TextEdit::singleline(filter)
                .id_salt("wfs-filter")
                .hint_text(t!("wfs-filter"))
                .desired_width(220.0),
        );
        if ui.add(theme::ghost_button(t!("wfs-all"))).clicked() {
            for &i in &shown {
                chosen[i] = true;
            }
        }
        if ui.add(theme::ghost_button(t!("wfs-none"))).clicked() {
            for &i in &shown {
                chosen[i] = false;
            }
        }
    });
    let output = ScrollArea::vertical()
        .id_salt("wfs-layer-list")
        .auto_shrink([false, true])
        .max_height(280.0)
        .min_scrolled_height(last_height.min(280.0))
        .show(ui, |ui| {
            for &i in &shown {
                let ft = &caps.feature_types[i];
                let row = ui.horizontal(|ui| {
                    ui.checkbox(&mut chosen[i], theme::semibold(&ft.name));
                    if !ft.title.is_empty() && ft.title != ft.name {
                        ui.label(RichText::new(&ft.title).color(p.muted));
                    }
                    if !ft.default_crs.is_empty() {
                        ui.label(RichText::new(&ft.default_crs).small().color(p.muted));
                    }
                });
                if !ft.abstract_text.is_empty() {
                    row.response.on_hover_text(&ft.abstract_text);
                }
            }
        });
    *last_height = output.content_size.y;
}
