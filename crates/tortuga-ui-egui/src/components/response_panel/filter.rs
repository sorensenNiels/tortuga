//! The filter above a JSON body (`JSONPath`, R-HTTP-13) or an XML body (`XPath`, R-HTTP-19).

use tortuga_core::model::is_json_content_type;
use tortuga_core::{Response, jsonpath, xml, xpath};

use super::{Body, ResponseView, TextBody, shown};
use crate::format;

/// What a text body is, which decides the filter it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Syntax {
    Plain,
    Json,
    Xml,
}

impl Syntax {
    /// For a body too big to pretty-print, from the content type and its start.
    pub(super) fn of(response: &Response, start: &str) -> Self {
        if response.content_type().is_some_and(is_json_content_type)
            || matches!(start.trim_start().as_bytes().first(), Some(b'{' | b'['))
        {
            Self::Json
        } else if response.looks_xml(start) {
            Self::Xml
        } else {
            Self::Plain
        }
    }

    pub(super) fn filter_hint(self) -> &'static str {
        match self {
            Self::Json => "$.items[*].id",
            Self::Xml => "//item/@id",
            Self::Plain => "",
        }
    }

    pub(super) fn filter_help(self) -> &'static str {
        match self {
            Self::Json => "JSONPath: $.name, [0], [-1], [1:3], [*], ..name, [?(@.price < 10)]",
            Self::Xml => {
                "XPath: /a/b, //name, *, @id, text(), .., [1], [last()], [@id='7'], [price < 10]. \
                 A name without a prefix matches any prefix."
            }
            Self::Plain => "",
        }
    }
}

/// The filter's result for the shown response (R-HTTP-13, R-HTTP-19).
#[derive(Debug, Clone, Copy)]
pub enum Filtered<'a> {
    /// No filter typed.
    Off,
    /// Being worked out.
    Pending,
    Done(&'a TextBody),
    Failed(&'a str),
}

impl ResponseView {
    fn syntax(&self) -> Syntax {
        match &self.body {
            Body::Text(text) => text.syntax,
            Body::Empty | Body::Binary | Body::Image(_) => Syntax::Plain,
        }
    }

    /// The whole body as JSON, parsed the first time. Slow for big bodies: call it off
    /// the UI thread.
    pub fn json(&self) -> Option<&serde_json::Value> {
        self.json
            .get_or_init(|| serde_json::from_slice(&self.response.body).ok())
            .as_ref()
    }

    /// The whole body as XML, parsed the first time. Slow for big bodies: call it off
    /// the UI thread.
    fn xml(&self) -> Option<&xml::Document> {
        self.xml
            .get_or_init(|| {
                let text = std::str::from_utf8(&self.response.body).ok()?;
                xml::Document::parse(text).ok()
            })
            .as_ref()
    }
}

/// Filters a JSON body with a `JSONPath` expression (R-HTTP-13), an XML body with an
/// `XPath` expression (R-HTTP-19). Parses the whole body the first time, so call it off
/// the UI thread.
pub fn filter_body(view: &ResponseView, expr: &str) -> Result<TextBody, String> {
    match view.syntax() {
        Syntax::Json => {
            let value = view
                .json()
                .ok_or_else(|| "The body is not valid JSON.".to_owned())?;
            let found = jsonpath::query(expr, value).map_err(|e| format::sentence(&e))?;
            let text = serde_json::to_string_pretty(&found).map_err(|e| e.to_string())?;
            Ok(shown(text, Syntax::Json))
        }
        Syntax::Xml => {
            let doc = view
                .xml()
                .ok_or_else(|| "The body is not valid XML.".to_owned())?;
            let text = xpath::query(expr, doc).map_err(|e| format::sentence(&e))?;
            Ok(shown(text, Syntax::Xml))
        }
        Syntax::Plain => Err("Only JSON and XML bodies can be filtered.".to_owned()),
    }
}
