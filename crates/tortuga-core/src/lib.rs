//! Tortuga core: everything that is not UI.
//!
//! This crate must stay free of any UI framework so it can be reused by the
//! desktop app, a future web build and a future CLI runner.

pub mod collection;
pub mod cookies;
pub mod curl;
pub mod data;
pub mod error;
pub mod format;
pub mod git;
pub mod history;
pub mod http;
pub mod json;
pub mod jsonpath;
pub mod model;
pub mod query;
pub mod report;
pub mod runner;
pub mod script;
pub mod secrets;
pub mod slug;
pub mod storage;
pub mod vars;
pub mod workflow;
pub mod workspace;

pub use collection::{Collection, Direction, Item};
pub use cookies::CookieJar;
pub use error::{Error, Result};
pub use format::{Environment, Manifest, Server};
pub use http::{Client, ClientSettings, FileReader};
pub use model::{
    Auth, BodyView, FormPart, HttpMethod, KeyValue, Request, RequestBody, Response, Scripts,
};
