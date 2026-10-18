//! Data files for data-driven workflow runs (R-FLOW-4): CSV with a header row, or a
//! JSON array of objects. Each row becomes the variables of one iteration.

use std::collections::BTreeMap;

/// One row: variable name → value.
pub type Row = BTreeMap<String, String>;

/// Reads `text` as CSV, or as JSON when `name` ends in `.json` (or the text starts
/// with `[`). The error says what is wrong.
pub fn parse(name: &str, text: &str) -> Result<Vec<Row>, String> {
    let json = std::path::Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        || text.trim_start().starts_with('[');
    if json {
        parse_json(text)
    } else {
        parse_csv(text)
    }
}

fn parse_json(text: &str) -> Result<Vec<Row>, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("not valid JSON: {e}"))?;
    let serde_json::Value::Array(items) = value else {
        return Err("a JSON data file must be an array of objects".into());
    };
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| match item {
            serde_json::Value::Object(map) => Ok(map
                .into_iter()
                .map(|(k, v)| {
                    let v = match v {
                        serde_json::Value::String(s) => s,
                        serde_json::Value::Null => String::new(),
                        other => other.to_string(),
                    };
                    (k, v)
                })
                .collect()),
            _ => Err(format!("item {} is not an object", i + 1)),
        })
        .collect()
}

/// RFC 4180 CSV: commas, double quotes around fields with commas, quotes or line
/// breaks, `""` for a quote inside them. The first row names the variables.
fn parse_csv(text: &str) -> Result<Vec<Row>, String> {
    let mut records = csv_records(text)?;
    records.retain(|r| !(r.len() == 1 && r[0].is_empty()));
    let mut records = records.into_iter();
    let Some(header) = records.next() else {
        return Ok(Vec::new());
    };
    let header: Vec<String> = header.into_iter().map(|h| h.trim().to_owned()).collect();
    if header.iter().any(String::is_empty) {
        return Err("every column in the first row needs a name".into());
    }
    records
        .enumerate()
        .map(|(i, record)| {
            if record.len() > header.len() {
                return Err(format!(
                    "row {} has more fields than there are names",
                    i + 2
                ));
            }
            Ok(header
                .iter()
                .cloned()
                .zip(record.into_iter().chain(std::iter::repeat(String::new())))
                .collect())
        })
        .collect()
}

fn csv_records(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            (true, '"') => quoted = false,
            (false, '"') if field.is_empty() => quoted = true,
            (false, ',') => record.push(std::mem::take(&mut field)),
            (false, '\r') if chars.peek() == Some(&'\n') => {}
            (false, '\n' | '\r') => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            (_, c) => field.push(c),
        }
    }
    if quoted {
        return Err("a quoted field is not closed".into());
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pairs: &[(&str, &str)]) -> Row {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn csv_rows() {
        let text = "name,email,note\r\nAnn,ann@x.test,\"likes \"\"quotes\"\", commas\"\nBob,bob@x.test\n\n\"Multi\nline\",m@x.test,ok\n";
        let rows = parse("users.csv", text).expect("csv");
        assert_eq!(
            rows,
            [
                row(&[
                    ("name", "Ann"),
                    ("email", "ann@x.test"),
                    ("note", "likes \"quotes\", commas")
                ]),
                row(&[("name", "Bob"), ("email", "bob@x.test"), ("note", "")]),
                row(&[
                    ("name", "Multi\nline"),
                    ("email", "m@x.test"),
                    ("note", "ok")
                ]),
            ]
        );
        assert!(parse("x.csv", "a,b\n1,2,3\n").is_err());
        assert!(parse("x.csv", "a,\n1,2\n").is_err());
        assert!(parse("x.csv", "a\n\"open\n").is_err());
        assert_eq!(parse("x.csv", "").expect("empty"), Vec::<Row>::new());
    }

    #[test]
    fn json_rows() {
        let rows = parse(
            "users.json",
            r#"[{"name": "Ann", "age": 31, "admin": true, "x": null}, {"name": "Bob", "tags": ["a"]}]"#,
        )
        .expect("json");
        assert_eq!(
            rows,
            [
                row(&[("name", "Ann"), ("age", "31"), ("admin", "true"), ("x", "")]),
                row(&[("name", "Bob"), ("tags", r#"["a"]"#)]),
            ]
        );
        assert!(parse("x.json", r#"{"a": 1}"#).is_err());
        assert!(parse("x.json", "[1]").is_err());
    }
}
