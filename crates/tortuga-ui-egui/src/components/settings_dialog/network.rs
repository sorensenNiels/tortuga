//! The Requests page (R-HTTP-7, R-HTTP-8) and the Network page: certificates
//! (R-HTTP-16, R-SET-3) and the proxy (R-HTTP-20, D54).

use std::path::PathBuf;

use egui::{TextEdit, Ui};
use tortuga_core::{ClientSettings, ProxyMode, check_proxy_url};

use super::layout::{About, Rows, group};
use crate::components::request_editor::parse_timeout;
use crate::theme;

/// Redirects, cookies and the timeout.
pub(super) fn requests_page(ui: &mut Ui, d: &mut ClientSettings, timeout_text: &mut String) {
    group(ui, &t!("settings-group-sending"), None, |rows| {
        switch_row(
            rows,
            &t!("defaults-follow-redirects"),
            About::default(),
            &mut d.follow_redirects,
        );
        switch_row(
            rows,
            &t!("defaults-cookies"),
            About::default(),
            &mut d.send_cookies,
        );
        rows.row(
            &t!("settings-default-timeout"),
            About::both(&t!("settings-timeout-about"), &t!("settings-requests-hint")),
            |ui, name| {
                if ui
                    .add(TextEdit::singleline(timeout_text).desired_width(100.0))
                    .labelled_by(name)
                    .changed()
                    && let Some(ms) = parse_timeout(timeout_text)
                {
                    d.default_timeout_ms = ms;
                }
            },
        );
    });
}

/// Certificates and the proxy, with the texts of the hosts not to verify (R-SET-3) and
/// of the hosts that bypass a manual proxy (R-HTTP-20), one per line. Returns whether
/// "Add…" was clicked for a CA certificate.
pub(super) fn network_page(
    ui: &mut Ui,
    d: &mut ClientSettings,
    hosts_text: &mut String,
    no_proxy_text: &mut String,
    can_pick_files: bool,
) -> bool {
    let add = group(ui, &t!("settings-group-certificates"), None, |rows| {
        switch_row(
            rows,
            &t!("settings-verify-tls"),
            About::default(),
            &mut d.verify_tls,
        );
        if d.verify_tls {
            insecure_hosts(rows, d, hosts_text);
        } else {
            rows.full_row(|ui| theme::warning(ui, t!("settings-verify-tls-off")));
        }
        ca_certificates(rows, &mut d.ca_certificates, can_pick_files)
    });
    group(ui, &t!("settings-proxy"), None, |rows| {
        proxy(rows, d, no_proxy_text);
    });
    add
}

/// A row with a switch for `on`.
pub(super) fn switch_row(rows: &mut Rows<'_>, name: &str, about: About<'_>, on: &mut bool) -> bool {
    rows.row(name, about, |ui, label| {
        // As tall as a button, so it lines up with the name like other controls.
        ui.horizontal(|ui| {
            ui.set_min_height(ui.spacing().interact_size.y);
            theme::switch(ui, on).labelled_by(label).changed()
        })
        .inner
    })
}

/// Hosts whose certificates are not verified, on this machine only (R-SET-3).
fn insecure_hosts(rows: &mut Rows<'_>, d: &mut ClientSettings, text: &mut String) {
    rows.row(
        &t!("settings-insecure-hosts"),
        About::both(
            &t!("settings-insecure-hosts-about"),
            &t!("settings-insecure-hosts-hint"),
        ),
        |ui, name| {
            let edit = TextEdit::multiline(text)
                .id_salt("insecure-hosts")
                .font(egui::TextStyle::Monospace)
                .hint_text("localhost\n*.dev.internal")
                .desired_rows(2)
                .desired_width(ui.available_width());
            if ui.add(edit).labelled_by(name).changed() {
                d.insecure_hosts = lines(text);
            }
            if !d.insecure_hosts.is_empty() {
                theme::warning(ui, t!("settings-insecure-hosts-warning"));
            }
        },
    );
}

/// The non-empty lines of `text`, trimmed.
fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// PEM files trusted besides the system's certificates (R-HTTP-16). Returns whether
/// "Add…" was clicked.
fn ca_certificates(rows: &mut Rows<'_>, files: &mut Vec<PathBuf>, can_pick_files: bool) -> bool {
    rows.row(
        &t!("settings-ca-certificates"),
        About::both(&t!("settings-ca-about"), &t!("settings-ca-hint")),
        |ui, _| {
            let mut remove = None;
            for (i, path) in files.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui
                        .add(theme::icon_button(ui, "🗙"))
                        .on_hover_text(t!("settings-ca-remove"))
                        .clicked()
                    {
                        remove = Some(i);
                    }
                    ui.add(egui::Label::new(path.display().to_string()).truncate())
                        .on_hover_text(path.display().to_string());
                });
            }
            if let Some(i) = remove {
                files.remove(i);
            }
            can_pick_files && ui.button(t!("settings-add")).clicked()
        },
    )
}

/// Which proxy requests go through (R-HTTP-20, D54).
fn proxy(rows: &mut Rows<'_>, d: &mut ClientSettings, no_proxy_text: &mut String) {
    let about = match d.proxy {
        ProxyMode::System => t!("settings-proxy-system-hint"),
        ProxyMode::None => t!("settings-proxy-none-hint"),
        ProxyMode::Manual => String::new(),
    };
    let about = if about.is_empty() {
        About::default()
    } else {
        About::short(&about)
    };
    rows.row(&t!("settings-proxy-use"), about, |ui, _| {
        theme::segmented(
            ui,
            &mut d.proxy,
            &[
                (ProxyMode::System, t!("settings-proxy-system")),
                (ProxyMode::None, t!("none")),
                (ProxyMode::Manual, t!("settings-proxy-manual")),
            ],
        );
    });
    if d.proxy != ProxyMode::Manual {
        return;
    }
    rows.row(&t!("settings-proxy-url"), About::default(), |ui, name| {
        ui.add(
            TextEdit::singleline(&mut d.proxy_url)
                .id_salt("proxy-url")
                .hint_text("http://127.0.0.1:8080")
                .desired_width(ui.available_width()),
        )
        .labelled_by(name);
        if let Err(e) = check_proxy_url(&d.proxy_url) {
            theme::error(ui, tortuga_core::error::message(&e));
        }
    });
    rows.row(
        &t!("settings-no-proxy"),
        About::both(
            &t!("settings-no-proxy-about"),
            &t!("settings-no-proxy-hint"),
        ),
        |ui, name| {
            let edit = TextEdit::multiline(no_proxy_text)
                .id_salt("no-proxy")
                .font(egui::TextStyle::Monospace)
                .hint_text("localhost\n*.internal")
                .desired_rows(2)
                .desired_width(ui.available_width());
            if ui.add(edit).labelled_by(name).changed() {
                d.no_proxy = lines(no_proxy_text);
            }
        },
    );
}
