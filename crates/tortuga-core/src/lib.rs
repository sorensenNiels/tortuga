//! Tortuga core: everything that is not UI.
//!
//! This crate must stay free of any UI framework so the desktop app, the web build
//! and `tortuga-cli` all behave the same (P7).

pub mod collection;
pub mod cookies;
pub mod curl;
pub mod data;
pub mod error;
pub mod format;
pub mod git;
pub mod history;
pub mod http;
pub mod http_file;
pub mod import;
pub mod json;
pub mod jsonpath;
pub mod model;
pub mod openapi;
pub mod postman;
pub mod query;
pub mod report;
pub mod runner;
pub mod script;
pub mod secrets;
pub mod settings;
pub mod slug;
pub mod storage;
pub mod vars;
pub mod workflow;
pub mod workspace;

pub use collection::{Collection, Direction, Item, NewItem};
pub use cookies::{CookieInfo, CookieJar};
pub use error::{Error, Result};
pub use format::{Environment, Manifest, Server};
pub use http::{Client, ClientSettings, FileReader};
pub use model::{
    Auth, BodyView, FormPart, HttpMethod, KeyValue, Request, RequestBody, Response, Scripts,
};
pub use settings::AppSettings;
