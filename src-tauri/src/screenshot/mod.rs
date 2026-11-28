use tauri::Manager;

mod audio;
mod bytes;
mod capture;
mod clock;
pub mod commands;
mod error;
mod frame;
mod hotkey;
mod library;
mod model;
mod mp4;
mod ocr;
mod record;
mod scroll;
mod session;
mod snap;
mod windows;

pub use session::{conflicts_with, initialize};

pub(crate) fn save_directory(app: &tauri::AppHandle) -> String {
    app.try_state::<session::ScreenshotState>()
        .map(|state| state.settings().save_dir)
        .unwrap_or_default()
}

pub(crate) fn capture_active() -> bool {
    record::is_active()
}
