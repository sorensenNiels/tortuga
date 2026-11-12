//! OGC exception reports (R-HTTP-29, D80): map servers such as GeoServer answer many
//! errors with HTTP 200 and an XML body saying what went wrong. Recognised for every
//! response, not only with the GeoServer add-on, since any OGC service does this.

use crate::script::TestResult;
use crate::xml::{Document, Kind, local};

/// The name of the failed test a response with an exception report gets.
pub const TEST_NAME: &str = "No OGC exception report";

/// How far into the body the report's root element must start: reports are small, and
/// this keeps the check cheap for every other response.
const LOOK_AHEAD: usize = 1024;

/// One exception of a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exception {
    /// `exceptionCode` (OWS) or `code` (WMS/WFS 1.0), e.g. `InvalidParameterValue`.
    pub code: Option<String>,
    /// Which parameter it is about, e.g. `typeName`.
    pub locator: Option<String>,
    /// What the server says, whitespace collapsed.
    pub text: String,
}

impl Exception {
    /// One line: `code (locator): text`.
    pub fn summary(&self) -> String {
        let mut out = self.code.clone().unwrap_or_default();
        if let Some(locator) = &self.locator {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push('(');
            out.push_str(locator);
            out.push(')');
        }
        match (out.is_empty(), self.text.is_empty()) {
            (true, _) => self.text.clone(),
            (false, true) => out,
            (false, false) => format!("{out}: {}", self.text),
        }
    }
}

/// The exceptions of `body` if it is an OGC exception report (`ows:ExceptionReport`, or
/// `ServiceExceptionReport` of WMS and WFS 1.0); `None` for anything else.
pub fn exceptions(body: &[u8]) -> Option<Vec<Exception>> {
    let start = &body[..body.len().min(LOOK_AHEAD)];
    // Cheap first: most bodies are not reports.
    if !start.windows(15).any(|w| w == b"ExceptionReport") {
        return None;
    }
    let text = std::str::from_utf8(body).ok()?;
    let doc = Document::parse(text).ok()?;
    let root = doc.root()?;
    let Kind::Element(e) = &doc.nodes[root].kind else {
        return None;
    };
    if !matches!(local(&e.name), "ExceptionReport" | "ServiceExceptionReport") {
        return None;
    }
    let mut found = Vec::new();
    collect(&doc, root, &mut found);
    if found.is_empty() {
        // A report with no exception in it still says something went wrong.
        found.push(Exception {
            code: None,
            locator: None,
            text: squash(&doc.string_value(root)),
        });
    }
    Some(found)
}

/// The failed test a response with an exception report adds to its results (R-HTTP-29).
pub fn failed_test(body: &[u8]) -> Option<TestResult> {
    let found = exceptions(body)?;
    Some(TestResult {
        name: TEST_NAME.to_owned(),
        passed: false,
        message: found.first().map(Exception::summary),
    })
}

fn collect(doc: &Document, node: usize, out: &mut Vec<Exception>) {
    for &child in &doc.nodes[node].children {
        let Kind::Element(e) = &doc.nodes[child].kind else {
            continue;
        };
        if matches!(local(&e.name), "Exception" | "ServiceException") {
            let attr = |names: &[&str]| {
                e.attributes
                    .iter()
                    .find(|(k, _)| names.contains(&local(k)))
                    .map(|(_, v)| v.clone())
                    .filter(|v| !v.trim().is_empty())
            };
            out.push(Exception {
                code: attr(&["exceptionCode", "code"]),
                locator: attr(&["locator"]),
                text: squash(&doc.string_value(child)),
            });
        } else {
            collect(doc, child, out);
        }
    }
}

/// `text` with runs of whitespace as one space, trimmed.
fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ows:ExceptionReport xmlns:ows="http://www.opengis.net/ows/1.1" version="2.0.0">
  <ows:Exception exceptionCode="InvalidParameterValue" locator="typeName">
    <ows:ExceptionText>Feature type topp:nope
      unknown</ows:ExceptionText>
  </ows:Exception>
</ows:ExceptionReport>"#;

    /// R-HTTP-29: an OWS report gives its code, locator and text.
    #[test]
    fn ows_exception_reports_are_recognised() {
        let found = exceptions(OWS.as_bytes()).expect("a report");
        assert_eq!(
            found,
            vec![Exception {
                code: Some("InvalidParameterValue".into()),
                locator: Some("typeName".into()),
                text: "Feature type topp:nope unknown".into(),
            }]
        );
        assert_eq!(
            found[0].summary(),
            "InvalidParameterValue (typeName): Feature type topp:nope unknown"
        );
    }

    /// R-HTTP-29: WMS and WFS 1.0 use `ServiceExceptionReport` with a `code` attribute.
    #[test]
    fn service_exception_reports_are_recognised() {
        let body = r#"<ServiceExceptionReport version="1.2.0">
  <ServiceException code="LayerNotDefined">Could not find layer ü😀</ServiceException>
  <ServiceException>Second</ServiceException>
</ServiceExceptionReport>"#;
        let found = exceptions(body.as_bytes()).expect("a report");
        assert_eq!(found.len(), 2);
        assert_eq!(
            found[0].summary(),
            "LayerNotDefined: Could not find layer ü😀"
        );
        assert_eq!(found[1].summary(), "Second");
    }

    /// R-HTTP-29: other XML, JSON, empty and broken bodies are not reports.
    #[test]
    fn other_bodies_are_not_reports() {
        for body in [
            "",
            "{\"ExceptionReport\": 1}",
            "<wfs:FeatureCollection/>",
            "<ExceptionReport",
            "<Doc><ExceptionReport/></Doc>",
        ] {
            assert_eq!(exceptions(body.as_bytes()), None, "{body}");
        }
        assert_eq!(exceptions(&[0xff, 0xfe]), None);
    }

    /// R-HTTP-29: a report adds a failed test with the first exception.
    #[test]
    fn a_report_fails_a_test() {
        let test = failed_test(OWS.as_bytes()).expect("failed test");
        assert!(!test.passed);
        assert_eq!(test.name, TEST_NAME);
        assert!(
            test.message
                .expect("message")
                .starts_with("InvalidParameterValue")
        );
        assert!(failed_test(b"<ok/>").is_none());
    }

    #[test]
    fn an_empty_report_still_says_something() {
        let found =
            exceptions(b"<ows:ExceptionReport>Broken</ows:ExceptionReport>").expect("report");
        assert_eq!(found[0].summary(), "Broken");
    }
}
