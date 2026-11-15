pub(crate) mod commands;
#[cfg(any(test, target_os = "linux"))]
mod desktop;
mod error;
mod io;
mod model;
mod monitor;
pub(crate) mod paste;
mod rich;
mod service;
mod store;
mod util;

pub use error::ClipboardError;
pub use service::{ClipboardState, initialize};

pub(crate) fn copy_plain_text(text: &str) -> Result<(), ClipboardError> {
    io::write_text(text)
}
