//! Tortuga core: everything that is not UI.
//!
//! This crate must stay free of any UI framework so it can be reused by the
//! desktop app, a future web build and a future CLI runner.

pub mod collection;
pub mod error;
pub mod format;
pub mod http;
pub mod json;
pub mod model;
pub mod query;
pub mod secrets;
pub mod slug;
pub mod storage;
pub mod vars;

pub use collection::{Collection, Direction, Item};
pub use error::{Error, Result};
pub use format::{Environment, Manifest, Server};
pub use http::{Client, ClientSettings};
pub use model::{Auth, BodyView, HttpMethod, KeyValue, Request, RequestBody, Response};
