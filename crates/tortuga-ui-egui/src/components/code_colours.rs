//! Colours for JSON and XML bodies, in the request editor and the response (R-HTTP-24).

use std::ops::Range;

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Ui};
use tortuga_core::highlight::{self, Language, Token};

use crate::theme::{self, Palette};

/// Bodies longer than this are not coloured while editing: they would be scanned on every
/// frame.
const MAX_COLOURED: usize = 200_000;

/// The colour of `token`.
pub fn colour(p: &Palette, token: Token) -> Color32 {
    match token {
        Token::Key => p.blue,
        Token::String => p.ok,
        Token::Number => p.redirect,
        Token::Literal | Token::Attribute => p.purple,
        Token::Punctuation | Token::Comment => p.muted,
        Token::Variable => p.accent,
    }
}

/// `text` laid out for a code field, in colour if it has a `language`.
pub fn layout(ui: &Ui, text: &str, language: Option<Language>, wrap_width: f32) -> LayoutJob {
    let spans = match language {
        Some(language) if text.len() <= MAX_COLOURED => highlight::spans(language, text),
        Some(_) | None => Vec::new(),
    };
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let mut job = coloured_job(ui, text, &spans, &font);
    job.wrap.max_width = wrap_width;
    job
}

/// `text` with `spans` (in order, on character boundaries) in colour.
fn coloured_job(ui: &Ui, text: &str, spans: &[(Range<usize>, Token)], font: &FontId) -> LayoutJob {
    let p = theme::palette(ui);
    let plain = ui.visuals().text_color();
    let format = |color| TextFormat::simple(font.clone(), color);
    let mut job = LayoutJob::default();
    let mut at = 0;
    for (range, token) in spans {
        let (Some(before), Some(part)) = (text.get(at..range.start), text.get(range.clone()))
        else {
            continue;
        };
        job.append(before, 0.0, format(plain));
        job.append(part, 0.0, format(colour(&p, *token)));
        at = range.end;
    }
    job.append(text.get(at..).unwrap_or_default(), 0.0, format(plain));
    job
}
