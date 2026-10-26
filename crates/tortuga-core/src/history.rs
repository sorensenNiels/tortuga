//! Local request history (R-HIST-1..3): what was sent, and what came back.
//!
//! One folder per project (and one for the scratch request), kept by the app in its
//! data folder, never in the project (§6.7). It holds `index.json`, a list of
//! [`Summary`]s newest first, and one `<id>.json` per [`Entry`], read only when an
//! entry is opened. The newest entries are kept, as many as the settings say
//! ([`DEFAULT_SIZE`] unless changed, R-SET-6).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::{HttpMethod, KeyValue, Request, Response};
use crate::storage::Storage;
use crate::{Error, Result};

/// Entries kept per project unless the settings say otherwise (R-SET-6).
pub const DEFAULT_SIZE: usize = 200;
/// Response bodies are cut to this many bytes.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

const INDEX: &str = "index.json";

/// What the history list shows of an entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    /// Unique and sortable: the send time, then a counter.
    pub id: String,
    /// Milliseconds since 1970 (UTC).
    pub sent_at_ms: u64,
    /// The request's path in its project; `None` for the scratch request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub name: String,
    pub method: HttpMethod,
    /// The URL as typed (with `{{variables}}`).
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    /// `None` if no response came back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
    /// Why there is no response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One sent request with its response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub summary: Summary,
    /// The request as it was in the editor (variables not filled in).
    pub request: Request,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<StoredResponse>,
}

impl Entry {
    /// An entry for `request`, sent at `sent_at_ms`, and what came back.
    pub fn new(
        id: String,
        sent_at_ms: u64,
        path: Option<PathBuf>,
        request: &Request,
        environment: Option<String>,
        result: std::result::Result<&Response, String>,
    ) -> Self {
        let (status, elapsed_ms, error, response) = match result {
            Ok(r) => (
                Some(r.status),
                Some(u64::try_from(r.elapsed.as_millis()).unwrap_or(u64::MAX)),
                None,
                Some(StoredResponse::new(r)),
            ),
            Err(e) => (None, None, Some(e), None),
        };
        Self {
            summary: Summary {
                id,
                sent_at_ms,
                path,
                name: request.name.clone(),
                method: request.method,
                url: request.display_url(),
                environment,
                status,
                elapsed_ms,
                error,
            },
            request: request.clone(),
            response,
        }
    }
}

/// A response as kept in history: the body cut to [`MAX_BODY_BYTES`], as text when it
/// is UTF-8 and as base64 otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<KeyValue>,
    pub elapsed_ms: u64,
    /// Size of the whole body.
    pub size: usize,
    pub body: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub base64: bool,
}

impl StoredResponse {
    pub fn new(response: &Response) -> Self {
        use base64::Engine as _;

        let kept = &response.body[..response.body.len().min(MAX_BODY_BYTES)];
        let (body, base64) = match std::str::from_utf8(kept) {
            Ok(text) => (text.to_owned(), false),
            // Cut in the middle of a character: keep the whole characters.
            Err(e) if e.error_len().is_none() => (
                String::from_utf8_lossy(&kept[..e.valid_up_to()]).into_owned(),
                false,
            ),
            Err(_) => (base64::engine::general_purpose::STANDARD.encode(kept), true),
        };
        Self {
            status: response.status,
            status_text: response.status_text.clone(),
            headers: response.headers.clone(),
            elapsed_ms: u64::try_from(response.elapsed.as_millis()).unwrap_or(u64::MAX),
            size: response.body.len(),
            body,
            base64,
        }
    }

    /// Whether only the start of the body was kept.
    pub fn truncated(&self) -> bool {
        self.size > MAX_BODY_BYTES
    }

    /// The response again, with the body as kept.
    pub fn to_response(&self) -> Response {
        use base64::Engine as _;

        let body = if self.base64 {
            base64::engine::general_purpose::STANDARD
                .decode(&self.body)
                .unwrap_or_default()
        } else {
            self.body.clone().into_bytes()
        };
        Response {
            status: self.status,
            status_text: self.status_text.clone(),
            headers: self.headers.clone(),
            body,
            elapsed: std::time::Duration::from_millis(self.elapsed_ms),
            timing: None,
        }
    }
}

/// Milliseconds since 1970 (UTC), now.
pub fn now_ms() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// A history id: sortable by time, unique thanks to `seq`.
pub fn new_id(sent_at_ms: u64, seq: u64) -> String {
    format!("{sent_at_ms:015}-{seq:04}")
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    entries: Vec<Summary>,
}

/// The history in one folder.
pub struct HistoryStore<'a> {
    storage: &'a dyn Storage,
    dir: PathBuf,
}

impl std::fmt::Debug for HistoryStore<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HistoryStore")
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

impl<'a> HistoryStore<'a> {
    pub fn new(storage: &'a dyn Storage, dir: impl Into<PathBuf>) -> Self {
        Self {
            storage,
            dir: dir.into(),
        }
    }

    /// The summaries, newest first. A missing folder is an empty history.
    pub fn list(&self) -> Result<Vec<Summary>> {
        Ok(self.index()?.entries)
    }

    /// One entry, in full.
    pub fn load(&self, id: &str) -> Result<Entry> {
        let path = self.entry_path(id)?;
        let text = self
            .storage
            .read_to_string(&path)
            .map_err(|e| io(&path, e))?;
        serde_json::from_str(&text).map_err(|e| Error::InvalidFile {
            path,
            message: e.to_string(),
        })
    }

    /// Adds `entry`, then removes the oldest beyond `keep` (at least the new one stays).
    pub fn add(&self, entry: &Entry, keep: usize) -> Result<()> {
        self.storage
            .create_dir_all(&self.dir)
            .map_err(|e| io(&self.dir, e))?;
        let path = self.entry_path(&entry.summary.id)?;
        let text =
            serde_json::to_string(entry).map_err(|e| Error::InvalidOperation(e.to_string()))?;
        self.storage.write(&path, &text).map_err(|e| io(&path, e))?;

        let mut index = self.index()?;
        index.entries.retain(|s| s.id != entry.summary.id);
        index.entries.insert(0, entry.summary.clone());
        index.entries.sort_by(|a, b| b.id.cmp(&a.id));
        self.keep_newest(index, keep.max(1))
    }

    /// Removes the oldest entries beyond `keep` (R-SET-6).
    pub fn trim(&self, keep: usize) -> Result<()> {
        if !self.storage.exists(&self.dir.join(INDEX)) {
            return Ok(());
        }
        let index = self.index()?;
        if index.entries.len() <= keep {
            return Ok(());
        }
        self.keep_newest(index, keep)
    }

    fn keep_newest(&self, mut index: Index, keep: usize) -> Result<()> {
        let dropped = index.entries.split_off(index.entries.len().min(keep));
        self.write_index(&index)?;
        for old in dropped {
            if let Ok(path) = self.entry_path(&old.id) {
                // Already gone is fine.
                let _ = self.storage.remove_file(&path);
            }
        }
        Ok(())
    }

    /// Removes the whole history of this folder (R-HIST-3).
    pub fn clear(&self) -> Result<()> {
        if !self.storage.exists(&self.dir) {
            return Ok(());
        }
        self.storage
            .remove_dir_all(&self.dir)
            .map_err(|e| io(&self.dir, e))
    }

    fn index(&self) -> Result<Index> {
        let path = self.dir.join(INDEX);
        if !self.storage.exists(&path) {
            return Ok(Index::default());
        }
        let text = self
            .storage
            .read_to_string(&path)
            .map_err(|e| io(&path, e))?;
        serde_json::from_str(&text).map_err(|e| Error::InvalidFile {
            path,
            message: e.to_string(),
        })
    }

    fn write_index(&self, index: &Index) -> Result<()> {
        let path = self.dir.join(INDEX);
        let text =
            serde_json::to_string(index).map_err(|e| Error::InvalidOperation(e.to_string()))?;
        self.storage.write(&path, &text).map_err(|e| io(&path, e))
    }

    /// `<dir>/<id>.json`; ids come from files, so anything that could leave the folder is refused.
    fn entry_path(&self, id: &str) -> Result<PathBuf> {
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(Error::InvalidOperation(format!(
                "not a history entry: {id}"
            )));
        }
        Ok(self.dir.join(format!("{id}.json")))
    }
}

fn io(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.to_owned(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::storage::FsStorage;

    fn response(body: Vec<u8>) -> Response {
        Response {
            status: 200,
            status_text: "OK".into(),
            headers: vec![KeyValue::new("content-type", "application/json")],
            body,
            elapsed: Duration::from_millis(42),
            timing: None,
        }
    }

    fn entry(n: u64) -> Entry {
        let mut request = Request::new(HttpMethod::Post, "https://{{host}}/users?page=2");
        request.name = format!("Request {n}");
        Entry::new(
            new_id(1_760_000_000_000 + n, n),
            1_760_000_000_000 + n,
            Some("users/create.toml".into()),
            &request,
            Some("Staging".into()),
            Ok(&response(br#"{"id":1}"#.to_vec())),
        )
    }

    /// R-HIST-1: entries round-trip; the list is newest first and the oldest are dropped.
    #[test]
    fn add_list_load_and_prune() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = HistoryStore::new(&FsStorage, dir.path().join("p"));
        assert_eq!(store.list().expect("empty"), []);

        for n in 0..(DEFAULT_SIZE as u64 + 5) {
            store.add(&entry(n), DEFAULT_SIZE).expect("add");
        }
        let list = store.list().expect("list");
        assert_eq!(list.len(), DEFAULT_SIZE);
        assert_eq!(
            list[0].name,
            format!("Request {}", DEFAULT_SIZE + 4),
            "newest first"
        );
        assert_eq!(list[0].url, "https://{{host}}/users?page=2");
        assert_eq!(list[0].status, Some(200));
        assert_eq!(list[0].environment.as_deref(), Some("Staging"));

        let newest = store.load(&list[0].id).expect("load");
        assert_eq!(newest, entry(DEFAULT_SIZE as u64 + 4));
        assert_eq!(
            newest.response.expect("response").to_response().body,
            br#"{"id":1}"#
        );
        assert!(store.load(&entry(0).summary.id).is_err(), "oldest removed");
        let files = std::fs::read_dir(dir.path().join("p"))
            .expect("dir")
            .count();
        assert_eq!(files, DEFAULT_SIZE + 1, "entries and the index");

        // R-SET-6: a smaller size trims what is there.
        store.trim(3).expect("trim");
        let list = store.list().expect("list");
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].name, format!("Request {}", DEFAULT_SIZE + 4));
        let files = std::fs::read_dir(dir.path().join("p"))
            .expect("dir")
            .count();
        assert_eq!(files, 4, "trimmed entries' files are removed");
        store.add(&entry(999), 0).expect("add");
        assert_eq!(store.list().expect("list").len(), 1, "the new one stays");

        store.clear().expect("clear");
        assert_eq!(store.list().expect("cleared"), []);
        store.clear().expect("clearing twice is fine");
        store.trim(1).expect("trimming nothing is fine");
    }

    #[test]
    fn big_and_binary_bodies() {
        let big = StoredResponse::new(&response("é".repeat(MAX_BODY_BYTES).into_bytes()));
        assert!(big.truncated());
        assert!(!big.base64);
        assert!(big.body.len() <= MAX_BODY_BYTES);
        assert_eq!(big.size, MAX_BODY_BYTES * 2);

        let binary = StoredResponse::new(&response(vec![0xff, 0x00, 0x10]));
        assert!(binary.base64);
        assert_eq!(binary.to_response().body, [0xff, 0x00, 0x10]);
    }

    #[test]
    fn errors_are_kept_without_a_response() {
        let request = Request::new(HttpMethod::Get, "https://down.test/");
        let e = Entry::new(
            new_id(5, 1),
            5,
            None,
            &request,
            None,
            Err("Connection refused".into()),
        );
        assert_eq!(e.summary.status, None);
        assert_eq!(e.summary.error.as_deref(), Some("Connection refused"));
        assert_eq!(e.response, None);
        let json = serde_json::to_string(&e).expect("json");
        assert_eq!(serde_json::from_str::<Entry>(&json).expect("parse"), e);
    }

    #[test]
    fn ids_sort_by_time_and_cannot_leave_the_folder() {
        assert!(new_id(9, 2) < new_id(10, 1));
        let store = HistoryStore::new(&FsStorage, "/tmp/h");
        assert!(store.load("../secrets").is_err());
        assert!(store.load("").is_err());
    }
}
