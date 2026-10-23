//! The Cookies tab: what the response's `Set-Cookie` headers set (R-CK-6).

use egui::{Grid, Label, RichText, ScrollArea, Ui};
use tortuga_core::{Expiry, ResponseCookies, SetCookie};

use super::ResponseAction;
use crate::{format, theme};

/// The longest a browser keeps a cookie: 400 days.
const MAX_AGE_SECS: u64 = 400 * 24 * 3600;

/// Whether the response panel has a Cookies tab, and which jar the request uses (R-CK-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookiesTab {
    /// The request does not send and store cookies, so there is nothing to look into.
    Hidden,
    /// The scratch request's jar, kept until the app closes.
    Scratch,
    /// A project's jar, which the cookie viewer shows (R-CK-5).
    Project,
}

/// The cookies the response set, and a way to the jar they went into.
pub(super) fn cookies_tab(
    ui: &mut Ui,
    cookies: &ResponseCookies,
    tab: CookiesTab,
    now_ms: u64,
    offset_minutes: i32,
) -> Option<ResponseAction> {
    let mut action = None;
    ScrollArea::vertical()
        .id_salt("response-cookies")
        .auto_shrink(false)
        .show(ui, |ui| {
            if cookies.cookies.is_empty() {
                theme::hint(ui, "This response set no cookies.");
            } else {
                cookie_grid(ui, &cookies.cookies, now_ms, offset_minutes);
            }
            let p = theme::palette(ui);
            for value in &cookies.unreadable {
                ui.add(
                    Label::new(
                        RichText::new(format!("Could not read Set-Cookie: {value}")).color(p.fail),
                    )
                    .wrap(),
                );
            }
            ui.add_space(8.0);
            match tab {
                CookiesTab::Project => {
                    ui.horizontal_wrapped(|ui| {
                        theme::hint(
                            ui,
                            "Stored in the project's cookie jar unless their domain or path \
                             does not match the request.",
                        );
                        if ui.add(theme::ghost_button("Open the jar…")).clicked() {
                            action = Some(ResponseAction::ShowCookieJar);
                        }
                    });
                }
                CookiesTab::Scratch => theme::hint(
                    ui,
                    "Stored in the scratch cookie jar until the app closes, unless their \
                     domain or path does not match the request.",
                ),
                CookiesTab::Hidden => {}
            }
        });
    action
}

fn cookie_grid(ui: &mut Ui, cookies: &[SetCookie], now_ms: u64, offset_minutes: i32) {
    let p = theme::palette(ui);
    Grid::new("response-cookies-grid")
        .num_columns(5)
        .spacing([16.0, 6.0])
        .striped(true)
        .show(ui, |ui| {
            for heading in ["Name", "Value", "Domain and path", "Expires", ""] {
                theme::section_label(ui, heading);
            }
            ui.end_row();
            for c in cookies {
                ui.label(RichText::new(&c.name).monospace());
                let shown = format::truncate(&c.value, 60).0;
                ui.label(RichText::new(shown).monospace().color(p.muted))
                    .on_hover_text(&c.value);
                let (place, hover) = place(c);
                ui.label(place).on_hover_text(hover);
                ui.label(expiry(c.expires, now_ms, offset_minutes));
                ui.label(RichText::new(flags(c)).small().color(p.muted));
                ui.end_row();
            }
        });
}

/// Where the cookie is sent, as `*.domain/path`, and what that means.
fn place(c: &SetCookie) -> (String, String) {
    let path = c.path.as_deref().filter(|p| *p != "/").unwrap_or_default();
    match &c.domain {
        Some(domain) => {
            let domain = domain.trim_start_matches('.');
            (
                format!("*.{domain}{path}"),
                format!("Sent to {domain} and its subdomains{}", under(c)),
            )
        }
        None => (
            format!("This host{path}"),
            format!("Sent only to the host that answered{}", under(c)),
        ),
    }
}

fn under(c: &SetCookie) -> String {
    match c.path.as_deref() {
        Some(path) => format!(", for paths under {path}"),
        None => ", for paths under the request's folder".into(),
    }
}

fn expiry(e: Expiry, now_ms: u64, offset_minutes: i32) -> String {
    match e {
        Expiry::Session => "Session".into(),
        Expiry::MaxAge(secs) if secs <= 0 => "Removes it".into(),
        Expiry::At(ms) if ms <= now_ms => "Removes it".into(),
        Expiry::MaxAge(secs) => {
            // Browsers keep a cookie at most 400 days (RFC 6265bis); this also keeps the
            // date in range.
            let capped = u64::try_from(secs).unwrap_or(0).min(MAX_AGE_SECS);
            let ms = now_ms.saturating_add(capped * 1000);
            format!("{} (Max-Age {secs} s)", at(ms, now_ms, offset_minutes))
        }
        Expiry::At(ms) => at(ms, now_ms, offset_minutes),
    }
}

fn at(ms: u64, now_ms: u64, offset_minutes: i32) -> String {
    format!(
        "{} {}",
        format::day_label(ms, now_ms, offset_minutes),
        format::clock(ms, offset_minutes)
    )
}

fn flags(c: &SetCookie) -> String {
    let same_site = c.same_site.as_ref().map(|s| format!("SameSite={s}"));
    [
        c.secure.then(|| "Secure".to_owned()),
        c.http_only.then(|| "HttpOnly".to_owned()),
        same_site,
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie() -> SetCookie {
        SetCookie {
            name: "session".into(),
            value: "abc".into(),
            domain: None,
            path: None,
            expires: Expiry::Session,
            secure: false,
            http_only: false,
            same_site: None,
        }
    }

    /// R-CK-6
    #[test]
    fn places_expiry_and_flags_read_as_sent() {
        assert_eq!(place(&cookie()).0, "This host");
        let c = SetCookie {
            domain: Some(".example.test".into()),
            path: Some("/api".into()),
            secure: true,
            http_only: true,
            same_site: Some("Lax".into()),
            ..cookie()
        };
        assert_eq!(place(&c).0, "*.example.test/api");
        assert!(place(&c).1.contains("for paths under /api"));
        assert_eq!(flags(&c), "Secure · HttpOnly · SameSite=Lax");
        assert_eq!(flags(&cookie()), "");

        let now = 1_700_000_000_000;
        assert_eq!(expiry(Expiry::Session, now, 0), "Session");
        assert_eq!(expiry(Expiry::MaxAge(0), now, 0), "Removes it");
        assert_eq!(expiry(Expiry::At(now - 1), now, 0), "Removes it");
        assert!(expiry(Expiry::MaxAge(60), now, 0).ends_with("(Max-Age 60 s)"));
        assert!(expiry(Expiry::MaxAge(i64::MAX), now, 0).contains("Max-Age"));
    }
}
