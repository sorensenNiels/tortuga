//! Tortuga core: everything that is not UI.
//!
//! This crate must stay free of any UI framework so the desktop app, the web build
//! and `tortuga-cli` all behave the same (P7).

pub mod bruno;
pub mod changes;
pub mod collection;
pub mod cookies;
pub mod curl;
pub mod data;
pub mod error;
pub mod format;
pub mod git;
pub mod highlight;
pub mod history;
pub mod http;
pub mod http_file;
pub mod import;
pub mod insomnia;
pub mod js_format;
pub mod json;
pub mod jsonpath;
pub mod mock;
pub mod model;
pub mod openapi;
pub mod postman;
pub mod query;
pub mod report;
pub mod request_docs;
pub mod runner;
pub mod script;
pub mod secrets;
pub mod settings;
pub mod slug;
pub mod storage;
pub mod update;
pub mod vars;
pub mod workflow;
pub mod workspace;
pub mod xml;
pub mod xpath;

pub use collection::{Collection, Direction, Item, NewItem};
pub use cookies::{CookieInfo, CookieJar, Expiry, ResponseCookies, SetCookie};
pub use error::{Error, Result};
pub use format::{Environment, EnvironmentColor, Manifest, Server};
pub use http::{Client, ClientSettings, FileReader, ProxyMode, check_proxy_url};
pub use model::{
    Auth, BodyView, Connection, FormPart, HttpMethod, KeyValue, ParamsBody, Phase, Request,
    RequestBody, Response, Scripts, Timing,
};
pub use settings::AppSettings;
