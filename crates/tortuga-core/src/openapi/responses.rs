//! An operation's responses as the request's examples for the mock (R-MOCK-6).

use serde_json::Value;

use super::Importer;
use crate::import::unique_name;
use crate::mock::Example;
use crate::model::{KeyValue, is_json_content_type};

impl<'a> Importer<'a> {
    /// An example per response code: each named example of its preferred media type
    /// (named by its key), else one from `example` or the schema (named by the code). A
    /// response without content gives an example without a body. Successful responses
    /// come first, so they answer when nothing else is asked for.
    pub(super) fn examples(&mut self, op: &'a Value) -> Vec<Example> {
        let Some(responses) = op.get("responses").and_then(Value::as_object) else {
            return Vec::new();
        };
        let mut examples: Vec<Example> = Vec::new();
        for (code, response) in responses {
            let Some(status) = status(code) else { continue };
            let response = self.resolve(response);
            for (name, example) in self.response_examples(code, response) {
                let name = unique_name(&name, |n| examples.iter().any(|e| e.name == n));
                examples.push(Example {
                    name,
                    status,
                    ..example
                });
            }
        }
        examples.sort_by_key(|e| !(200..300).contains(&e.status));
        examples
    }

    fn response_examples(&mut self, code: &str, response: &'a Value) -> Vec<(String, Example)> {
        let content = response.get("content").and_then(Value::as_object);
        let preferred = content.and_then(|content| {
            content
                .iter()
                .find(|(ct, _)| is_json_content_type(ct))
                .or_else(|| content.iter().next())
        });
        let Some((content_type, media)) = preferred else {
            return vec![(code.to_owned(), Example::new(code))];
        };
        let with_body = |value: &Value| Example {
            body: body_text(value),
            headers: vec![KeyValue::new("Content-Type", content_type.clone())],
            ..Example::new(code)
        };
        let named: Vec<(&String, &'a Value)> = media
            .get("examples")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .collect();
        if named.is_empty() {
            let value = self.media_example(media);
            return vec![(code.to_owned(), with_body(&value))];
        }
        named
            .into_iter()
            .map(|(name, example)| {
                let value = self.resolve(example).get("value").unwrap_or(&Value::Null);
                (name.clone(), with_body(value))
            })
            .collect()
    }
}

/// The status of a response code: `404`, or `4XX` as `400`; `default` has none.
fn status(code: &str) -> Option<u16> {
    let code = code.trim().to_ascii_uppercase();
    let digits = code
        .strip_suffix("XX")
        .map_or(code.clone(), |d| format!("{d}00"));
    digits.parse().ok().filter(|s| (100..=599).contains(s))
}

fn body_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Request;

    const API: &str = r#"
openapi: 3.1.0
info: { title: Users, version: "1" }
paths:
  /users/{id}:
    get:
      summary: Get user
      responses:
        "404":
          description: Not found
          content:
            application/problem+json:
              example: { title: "No such user" }
        "200":
          description: Found
          content:
            text/plain:
              example: ignored, JSON is preferred
            application/json:
              examples:
                ann: { value: { id: 1, name: "Åse ü😀" } }
                bob: { $ref: '#/components/examples/Bob' }
        default:
          description: Anything else
    delete:
      summary: Delete user
      responses:
        "204": { description: Deleted }
        5XX:
          description: Broken
          content:
            text/plain:
              schema: { type: string }
  /users:
    post:
      summary: Create user
      responses:
        "201":
          $ref: '#/components/responses/Created'
components:
  examples:
    Bob: { value: { id: 2, name: Bob } }
  responses:
    Created:
      description: Created
      content:
        application/json:
          schema:
            type: object
            properties:
              id: { type: integer }
              tags: { type: array, items: { type: string } }
"#;

    fn request<'a>(import: &'a crate::import::Import, name: &str) -> &'a Request {
        crate::import::requests(&import.items)
            .into_iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("no request {name}"))
    }

    fn summary(request: &Request) -> Vec<(&str, u16, &str)> {
        request
            .examples
            .iter()
            .map(|e| (e.name.as_str(), e.status, e.body.as_str()))
            .collect()
    }

    /// R-MOCK-6: named examples, examples from `example` and schemas, codes without
    /// content, `4XX`-style codes and `$ref`s; successes first.
    #[test]
    fn responses_become_examples() {
        let import = super::super::parse(API).expect("import");
        let get = request(&import, "Get user");
        assert_eq!(
            summary(get),
            [
                ("ann", 200, "{\n  \"id\": 1,\n  \"name\": \"Åse ü😀\"\n}"),
                ("bob", 200, "{\n  \"id\": 2,\n  \"name\": \"Bob\"\n}"),
                ("404", 404, "{\n  \"title\": \"No such user\"\n}"),
            ]
        );
        assert_eq!(
            get.examples[0].headers,
            [KeyValue::new("Content-Type", "application/json")]
        );
        assert_eq!(
            get.examples[2].headers,
            [KeyValue::new("Content-Type", "application/problem+json")]
        );

        let delete = request(&import, "Delete user");
        assert_eq!(summary(delete), [("204", 204, ""), ("5XX", 500, "string")]);
        assert_eq!(delete.examples[0].headers, []);

        let create = request(&import, "Create user");
        assert_eq!(
            summary(create),
            [(
                "201",
                201,
                "{\n  \"id\": 0,\n  \"tags\": [\n    \"string\"\n  ]\n}"
            )]
        );
    }

    #[test]
    fn status_codes() {
        assert_eq!(status("200"), Some(200));
        assert_eq!(status("4xx"), Some(400));
        assert_eq!(status("default"), None);
        assert_eq!(status("99"), None);
        assert_eq!(status("ü😀"), None);
    }
}
