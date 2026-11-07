pub(crate) mod commands;
mod error;
mod io;
mod model;
mod monitor;
mod rich;
mod service;
mod store;
mod util;

pub use error::ClipboardError;
pub use service::{ClipboardState, initialize};
