use std::ffi::CStr;
use std::sync::Mutex;

#[cfg(target_os = "macos")]
use dispatch2::DispatchQueue;
#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::{AnyClass, AnyObject};
#[cfg(target_os = "macos")]
use objc2::{MainThreadMarker, msg_send};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSApplication, NSMenu, NSPopUpMenuWindowLevel, NSStatusBarButton, NSStatusItem,
    NSStatusWindowLevel, NSView, NSWindowCollectionBehavior,
};
use tauri::image::Image;
use tauri::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Listener, Manager, Wry};

use crate::clipboard::ClipboardState;
use crate::shortcut;

/// Menu-bar status item. Left click toggles the clipboard panel when history is on,
/// and opens the enable explanation when it is off. Right click is the menu.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let labels = TrayLabels::default();
    let status = app
        .try_state::<ClipboardState>()
        .map(|state| state.status());
    let enabled = status.as_ref().is_some_and(|status| status.enabled);
    let paused = status.as_ref().is_some_and(|status| status.paused);
    let menu = build_menu(app, &labels)?;
    let icon = TrayIconBuilder::with_id("clipboard-status")
        .icon(glyph_image(glyph_for(enabled, paused)))
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip(tooltip_for(&labels, enabled, paused))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|icon, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } => on_primary(icon.app_handle(), rect),
            #[cfg(target_os = "macos")]
            TrayIconEvent::Click {
                button: MouseButton::Right,
                button_state: MouseButtonState::Down,
                ..
            } => show_status_menu(icon),
            _ => {}
        })
        .on_menu_event(|app, event| on_menu(app, &event))
        .build(app)?;
    #[cfg(target_os = "macos")]
    prepare_status_item(&icon);
    app.manage(TrayHost {
        icon,
        labels: Mutex::new(labels),
    });
    let handle = app.clone();
    let _ = app.listen("clipboard://status", move |_| refresh(&handle));
    let handle = app.clone();
    let _ = app.listen("screenshot://enabled", move |_| refresh(&handle));
    let handle = app.clone();
    let _ = app.listen("screenshot://library", move |_| refresh(&handle));
    let handle = app.clone();
    let _ = app.listen("ssh://enabled", move |_| refresh(&handle));
    let handle = app.clone();
    let _ = app.listen("ssh://hosts", move |_| refresh(&handle));
    let handle = app.clone();
    let _ = app.listen("docker://enabled", move |_| refresh(&handle));
    let handle = app.clone();
    let _ = app.listen("docker://endpoints", move |_| refresh(&handle));
    Ok(())
}

#[tauri::command]
pub fn clipboard_set_tray_icon(app: AppHandle, icon: String) -> Result<(), String> {
    let mark = TrayMark::parse(icon.trim()).ok_or("invalid_input")?;
    if let Ok(mut current) = TRAY_MARK.lock() {
        *current = mark;
    }
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn clipboard_set_tray_labels(app: AppHandle, labels: TrayLabels) {
    if let Some(host) = app.try_state::<TrayHost>() {
        if let Ok(mut current) = host.labels.lock() {
            *current = labels;
        }
    }
    refresh(&app);
}

#[tauri::command]
pub fn clipboard_take_main_request() -> Option<MainRequest> {
    PENDING_MAIN
        .lock()
        .ok()
        .and_then(|mut pending| pending.take())
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct MainRequest {
    pub path: String,
    pub enable: bool,
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayLabels {
    pause: String,
    resume: String,
    enable: String,
    #[serde(default)]
    ignore_next: String,
    #[serde(default)]
    ignore_next_armed: String,
    #[serde(default = "default_screenshot_label")]
    screenshot: String,
    #[serde(default = "default_record_label")]
    record_screen: String,
    #[serde(default = "default_recent_shots_label")]
    recent_shots: String,
    #[serde(default = "default_hosts_label")]
    hosts: String,
    #[serde(default = "default_open_shots_label")]
    open_shots: String,
    #[serde(default = "default_open_hosts_label")]
    open_hosts: String,
    #[serde(default = "default_no_shots_label")]
    no_shots: String,
    #[serde(default = "default_no_hosts_label")]
    no_hosts: String,
    #[serde(default = "default_docker_label")]
    docker: String,
    #[serde(default = "default_open_docker_label")]
    open_docker: String,
    #[serde(default = "default_no_endpoints_label")]
    no_endpoints: String,
    history: String,
    settings: String,
    quit: String,
    /// Used by the Linux menu. Other platforms open the panel from the icon click.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    show: String,
    tooltip_recording: String,
    tooltip_paused: String,
    tooltip_disabled: String,
}

impl Default for TrayLabels {
    fn default() -> Self {
        Self {
            pause: "暂停".into(),
            resume: "继续".into(),
            enable: "开启记录".into(),
            ignore_next: "忽略下一次复制".into(),
            ignore_next_armed: "将忽略下一次复制".into(),
            screenshot: default_screenshot_label(),
            record_screen: default_record_label(),
            recent_shots: default_recent_shots_label(),
            hosts: default_hosts_label(),
            open_shots: default_open_shots_label(),
            open_hosts: default_open_hosts_label(),
            no_shots: default_no_shots_label(),
            no_hosts: default_no_hosts_label(),
            docker: default_docker_label(),
            open_docker: default_open_docker_label(),
            no_endpoints: default_no_endpoints_label(),
            history: "打开全部历史".into(),
            settings: "设置".into(),
            quit: "退出 Pixo".into(),
            show: "打开剪贴板".into(),
            tooltip_recording: "正在记录剪贴板".into(),
            tooltip_paused: "剪贴板记录已暂停".into(),
            tooltip_disabled: "剪贴板记录未开启".into(),
        }
    }
}

fn default_screenshot_label() -> String {
    "截图".into()
}

fn default_record_label() -> String {
    "录屏".into()
}

fn default_recent_shots_label() -> String {
    "最近截图".into()
}

fn default_hosts_label() -> String {
    "主机".into()
}

fn default_open_shots_label() -> String {
    "全部截图".into()
}

fn default_open_hosts_label() -> String {
    "全部主机".into()
}

fn default_no_shots_label() -> String {
    "还没有截图".into()
}

fn default_no_hosts_label() -> String {
    "还没有主机".into()
}

fn default_docker_label() -> String {
    "Docker".into()
}

fn default_open_docker_label() -> String {
    "全部连接".into()
}

fn default_no_endpoints_label() -> String {
    "还没有连接".into()
}

struct TrayHost {
    icon: TrayIcon<Wry>,
    labels: Mutex<TrayLabels>,
}

static PENDING_MAIN: Mutex<Option<MainRequest>> = Mutex::new(None);
static TRAY_MARK: Mutex<TrayMark> = Mutex::new(TrayMark::Board);

#[derive(Clone, Copy, PartialEq, Eq)]
enum TrayMark {
    Board,
    Clip,
    Mark,
    Pages,
    Pin,
}

impl TrayMark {
    fn parse(icon: &str) -> Option<Self> {
        Some(match icon {
            "board" => Self::Board,
            "clip" => Self::Clip,
            "mark" => Self::Mark,
            "pages" => Self::Pages,
            "pin" => Self::Pin,
            _ => return None,
        })
    }

    fn current() -> Self {
        TRAY_MARK
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .to_owned()
    }
}

fn on_primary(app: &AppHandle, rect: tauri::Rect) {
    let enabled = app
        .try_state::<ClipboardState>()
        .is_some_and(|state| state.status().enabled);
    #[cfg(target_os = "macos")]
    if enabled {
        let (option, shift) = shortcut::click_modifiers();
        if option && shift {
            if let Some(clipboard) = app.try_state::<ClipboardState>() {
                clipboard.arm_ignore_next();
            }
            return;
        }
        if option {
            toggle_record(app);
            return;
        }
    }
    if enabled {
        shortcut::toggle_from_status(app, rect);
    } else {
        open_main(app, "/clipboard", true);
    }
}

fn on_menu(app: &AppHandle, event: &MenuEvent) {
    match menu_action(event.id.as_ref()) {
        MenuAction::Show => {
            let rect = app
                .try_state::<TrayHost>()
                .and_then(|host| host.icon.rect().ok())
                .flatten()
                .unwrap_or_default();
            on_primary(app, rect);
        }
        MenuAction::Pause => toggle_record(app),
        MenuAction::Ignore => {
            if let Some(clipboard) = app.try_state::<ClipboardState>() {
                clipboard.arm_ignore_next();
            }
        }
        MenuAction::Capture => {
            if crate::screenshot::commands::screenshot_is_enabled(app) {
                crate::screenshot::commands::screenshot_open_capture(app.clone());
            }
        }
        MenuAction::RecordScreen => {
            if crate::screenshot::commands::screenshot_is_enabled(app) {
                crate::screenshot::commands::screenshot_open_record(app.clone());
            }
        }
        MenuAction::OpenShots => {
            if crate::screenshot::commands::screenshot_is_enabled(app) {
                shortcut::present_tool(app, "/screenshot", None);
            }
        }
        MenuAction::OpenHosts => {
            if crate::ssh::menu_hosts(app).0 {
                shortcut::present_tool(app, "/ssh", None);
            }
        }
        MenuAction::CopyShot(id) => {
            if crate::screenshot::commands::screenshot_is_enabled(app) {
                crate::screenshot::commands::screenshot_menu_copy(app, id);
            }
        }
        MenuAction::OpenHost(id) => {
            if crate::ssh::menu_hosts(app).0 {
                shortcut::present_tool(app, "/ssh", Some(id));
            }
        }
        MenuAction::OpenDocker => {
            if crate::docker::menu_endpoints(app).0 {
                shortcut::present_tool(app, "/docker", None);
            }
        }
        MenuAction::OpenEndpoint(id) => {
            if crate::docker::menu_endpoints(app).0 {
                shortcut::present_tool(app, "/docker", Some(id));
            }
        }
        MenuAction::History => open_main(app, "/clipboard", false),
        MenuAction::Settings => open_main(app, "/settings", false),
        MenuAction::Quit => app.exit(0),
        MenuAction::Other => {}
    }
}

fn toggle_record(app: &AppHandle) {
    let Some(clipboard) = app.try_state::<ClipboardState>() else {
        return;
    };
    let status = clipboard.status();
    if !status.enabled {
        drop(clipboard);
        open_main(app, "/clipboard", true);
        return;
    }
    let _ = clipboard.set_paused(!status.paused);
}

fn main_request_path(path: &str) -> &'static str {
    match path {
        "/settings" => "/settings",
        "/settings/clipboard" | "/clipboard/settings" => "/settings/clipboard",
        "/settings/screenshot" => "/settings/screenshot",
        _ => "/clipboard",
    }
}

fn open_main(app: &AppHandle, path: &str, enable: bool) {
    let path = main_request_path(path);
    if let Ok(mut pending) = PENDING_MAIN.lock() {
        *pending = Some(MainRequest {
            path: path.to_string(),
            enable,
        });
    }
    shortcut::present_main(app, path, enable);
}

fn refresh(app: &AppHandle) {
    let Some(host) = app.try_state::<TrayHost>() else {
        return;
    };
    let Some(clipboard) = app.try_state::<ClipboardState>() else {
        return;
    };
    let status = clipboard.status();
    let labels = host
        .labels
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    let _ = host.icon.set_icon_with_as_template(
        Some(glyph_image(glyph_for(status.enabled, status.paused))),
        cfg!(target_os = "macos"),
    );
    let _ = host
        .icon
        .set_tooltip(Some(tooltip_for(&labels, status.enabled, status.paused)));
    if let Ok(menu) = build_menu(app, &labels) {
        let _ = host.icon.set_menu(Some(menu));
    }
}

enum MenuAction {
    Show,
    Pause,
    Ignore,
    Capture,
    RecordScreen,
    OpenShots,
    OpenHosts,
    OpenDocker,
    CopyShot(i64),
    OpenHost(i64),
    OpenEndpoint(i64),
    History,
    Settings,
    Quit,
    Other,
}

fn menu_action(id: &str) -> MenuAction {
    if let Some(rest) = id.strip_prefix("tray-shot-") {
        return rest
            .parse()
            .map(MenuAction::CopyShot)
            .unwrap_or(MenuAction::Other);
    }
    if let Some(rest) = id.strip_prefix("tray-host-") {
        return rest
            .parse()
            .map(MenuAction::OpenHost)
            .unwrap_or(MenuAction::Other);
    }
    if let Some(rest) = id.strip_prefix("tray-endpoint-") {
        return rest
            .parse()
            .map(MenuAction::OpenEndpoint)
            .unwrap_or(MenuAction::Other);
    }
    match id {
        "tray-show" => MenuAction::Show,
        "tray-record" => MenuAction::Pause,
        "tray-ignore" => MenuAction::Ignore,
        "tray-screenshot" => MenuAction::Capture,
        "tray-record-screen" => MenuAction::RecordScreen,
        "tray-shots-open" => MenuAction::OpenShots,
        "tray-hosts-open" => MenuAction::OpenHosts,
        "tray-docker-open" => MenuAction::OpenDocker,
        "tray-history" => MenuAction::History,
        "tray-settings" => MenuAction::Settings,
        "tray-quit" => MenuAction::Quit,
        _ => MenuAction::Other,
    }
}

fn build_menu(app: &AppHandle, labels: &TrayLabels) -> tauri::Result<Menu<Wry>> {
    let clipboard = app.try_state::<ClipboardState>();
    let enabled = clipboard
        .as_ref()
        .is_some_and(|state| state.status().enabled);
    let paused = clipboard
        .as_ref()
        .is_some_and(|state| state.status().paused);
    let armed = clipboard
        .as_ref()
        .is_some_and(|state| state.ignore_next_armed());
    let shots_on = crate::screenshot::commands::screenshot_is_enabled(app);
    let (hosts_on, hosts) = crate::ssh::menu_hosts(app);
    let (docker_on, endpoints) = crate::docker::menu_endpoints(app);
    let shots = if shots_on {
        crate::screenshot::commands::screenshot_menu_shots(app)
    } else {
        Vec::new()
    };

    let capture = MenuItem::with_id(
        app,
        "tray-screenshot",
        text_or(&labels.screenshot, &default_screenshot_label()),
        true,
        None::<&str>,
    )?;
    let record_screen = MenuItem::with_id(
        app,
        "tray-record-screen",
        text_or(&labels.record_screen, &default_record_label()),
        true,
        None::<&str>,
    )?;
    let shot_menu = shot_submenu(app, labels, true, &shots)?;
    let host_menu = host_submenu(app, labels, true, &hosts)?;
    let endpoint_menu = endpoint_submenu(app, labels, true, &endpoints)?;
    let pause = MenuItem::with_id(
        app,
        "tray-record",
        record_label(labels, enabled, paused),
        true,
        None::<&str>,
    )?;
    let ignore_label = if armed && !labels.ignore_next_armed.is_empty() {
        labels.ignore_next_armed.clone()
    } else if !labels.ignore_next.is_empty() {
        labels.ignore_next.clone()
    } else if armed {
        "将忽略下一次复制".to_string()
    } else {
        "忽略下一次复制".to_string()
    };
    let ignore = MenuItem::with_id(app, "tray-ignore", ignore_label, enabled, None::<&str>)?;
    let history = MenuItem::with_id(app, "tray-history", &labels.history, true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "tray-settings", &labels.settings, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray-quit", &labels.quit, true, None::<&str>)?;
    let gap_tools = PredefinedMenuItem::separator(app)?;
    let gap_quit = PredefinedMenuItem::separator(app)?;
    #[cfg(target_os = "linux")]
    let show = MenuItem::with_id(app, "tray-show", &labels.show, true, None::<&str>)?;
    #[cfg(target_os = "linux")]
    let gap_show = PredefinedMenuItem::separator(app)?;
    // A tool stays out of the menu until its settings switch is on.
    let mut entries: Vec<&dyn IsMenuItem<Wry>> = Vec::new();
    #[cfg(target_os = "linux")]
    {
        entries.push(&show);
        entries.push(&gap_show);
    }
    if shots_on {
        entries.push(&capture);
        entries.push(&record_screen);
        entries.push(&shot_menu);
    }
    if hosts_on {
        entries.push(&host_menu);
    }
    if docker_on {
        entries.push(&endpoint_menu);
    }
    if shots_on || hosts_on || docker_on {
        entries.push(&gap_tools);
    }
    entries.push(&pause);
    entries.push(&ignore);
    entries.push(&history);
    entries.push(&gap_quit);
    entries.push(&settings);
    entries.push(&quit);
    Menu::with_items(app, &entries)
}

fn shot_submenu(
    app: &AppHandle,
    labels: &TrayLabels,
    enabled: bool,
    shots: &[(i64, String)],
) -> tauri::Result<Submenu<Wry>> {
    let mut owned = Vec::new();
    let shown = shots.iter().take(5);
    for (id, label) in shown {
        owned.push(MenuItem::with_id(
            app,
            format!("tray-shot-{id}"),
            menu_text(label),
            true,
            None::<&str>,
        )?);
    }
    if owned.is_empty() {
        owned.push(MenuItem::with_id(
            app,
            "tray-shots-empty",
            text_or(&labels.no_shots, &default_no_shots_label()),
            false,
            None::<&str>,
        )?);
    }
    owned.push(MenuItem::with_id(
        app,
        "tray-shots-open",
        text_or(&labels.open_shots, &default_open_shots_label()),
        true,
        None::<&str>,
    )?);
    let refs: Vec<&dyn IsMenuItem<Wry>> = owned.iter().map(|item| item as _).collect();
    Submenu::with_items(
        app,
        text_or(&labels.recent_shots, &default_recent_shots_label()),
        enabled,
        &refs,
    )
}

fn host_submenu(
    app: &AppHandle,
    labels: &TrayLabels,
    enabled: bool,
    hosts: &[(i64, String)],
) -> tauri::Result<Submenu<Wry>> {
    let mut owned = Vec::new();
    for (id, label) in hosts.iter().take(8) {
        owned.push(MenuItem::with_id(
            app,
            format!("tray-host-{id}"),
            menu_text(label),
            true,
            None::<&str>,
        )?);
    }
    if owned.is_empty() {
        owned.push(MenuItem::with_id(
            app,
            "tray-hosts-empty",
            text_or(&labels.no_hosts, &default_no_hosts_label()),
            false,
            None::<&str>,
        )?);
    }
    owned.push(MenuItem::with_id(
        app,
        "tray-hosts-open",
        text_or(&labels.open_hosts, &default_open_hosts_label()),
        true,
        None::<&str>,
    )?);
    let refs: Vec<&dyn IsMenuItem<Wry>> = owned.iter().map(|item| item as _).collect();
    Submenu::with_items(
        app,
        text_or(&labels.hosts, &default_hosts_label()),
        enabled,
        &refs,
    )
}

fn endpoint_submenu(
    app: &AppHandle,
    labels: &TrayLabels,
    enabled: bool,
    endpoints: &[(i64, String)],
) -> tauri::Result<Submenu<Wry>> {
    let mut owned = Vec::new();
    for (id, label) in endpoints.iter().take(8) {
        owned.push(MenuItem::with_id(
            app,
            format!("tray-endpoint-{id}"),
            menu_text(label),
            true,
            None::<&str>,
        )?);
    }
    if owned.is_empty() {
        owned.push(MenuItem::with_id(
            app,
            "tray-endpoints-empty",
            text_or(&labels.no_endpoints, &default_no_endpoints_label()),
            false,
            None::<&str>,
        )?);
    }
    owned.push(MenuItem::with_id(
        app,
        "tray-docker-open",
        text_or(&labels.open_docker, &default_open_docker_label()),
        true,
        None::<&str>,
    )?);
    let refs: Vec<&dyn IsMenuItem<Wry>> = owned.iter().map(|item| item as _).collect();
    Submenu::with_items(
        app,
        text_or(&labels.docker, &default_docker_label()),
        enabled,
        &refs,
    )
}

fn text_or(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

fn menu_text(value: &str) -> String {
    let trimmed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = if trimmed.is_empty() {
        value.trim()
    } else {
        &trimmed
    };
    let mut text: String = trimmed.chars().take(48).collect();
    if trimmed.chars().count() > 48 {
        text.push('…');
    }
    #[cfg(windows)]
    {
        text = text.replace('&', "&&");
    }
    text
}

fn record_label(labels: &TrayLabels, enabled: bool, paused: bool) -> String {
    if !enabled {
        labels.enable.clone()
    } else if paused {
        labels.resume.clone()
    } else {
        labels.pause.clone()
    }
}

fn tooltip_for(labels: &TrayLabels, enabled: bool, paused: bool) -> String {
    if !enabled {
        labels.tooltip_disabled.clone()
    } else if paused {
        labels.tooltip_paused.clone()
    } else {
        labels.tooltip_recording.clone()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Glyph {
    Recording,
    Paused,
    Disabled,
}

fn glyph_for(enabled: bool, paused: bool) -> Glyph {
    if !enabled {
        Glyph::Disabled
    } else if paused {
        Glyph::Paused
    } else {
        Glyph::Recording
    }
}

fn glyph_image(kind: Glyph) -> Image<'static> {
    const SIZE: u32 = 44;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let (red, green, blue) = if cfg!(target_os = "macos") {
        (0, 0, 0)
    } else {
        (22, 119, 255)
    };
    for y in 0..SIZE {
        for x in 0..SIZE {
            let alpha = glyph_alpha(kind, TrayMark::current(), x, y);
            if alpha == 0 {
                continue;
            }
            let index = ((y * SIZE + x) * 4) as usize;
            rgba[index] = red;
            rgba[index + 1] = green;
            rgba[index + 2] = blue;
            rgba[index + 3] = alpha;
        }
    }
    Image::new_owned(rgba, SIZE, SIZE)
}

fn glyph_alpha(kind: Glyph, mark: TrayMark, x: u32, y: u32) -> u8 {
    let px = f64::from(x) + 0.5;
    let py = f64::from(y) + 0.5;
    let mut on = mark_shape(mark, px, py);
    if kind == Glyph::Paused {
        on = on || dist_to_segment(px, py, 9.0, 7.0, 35.0, 37.0) <= 2.4;
    }
    if !on {
        0
    } else if kind == Glyph::Disabled {
        96
    } else {
        255
    }
}

fn mark_shape(mark: TrayMark, px: f64, py: f64) -> bool {
    match mark {
        TrayMark::Board => {
            stroke_round(px, py, 11.0, 15.0, 33.0, 39.0, 4.0, 2.2)
                || inside_round(px, py, 17.0, 8.0, 27.0, 17.0, 2.5)
                || inside_rect(px, py, 16.0, 23.0, 28.0, 25.0)
                || inside_rect(px, py, 16.0, 29.0, 24.0, 31.0)
        }
        TrayMark::Clip => on_stroke(
            px,
            py,
            &[
                (20.0, 30.0),
                (20.0, 16.0),
                (24.0, 11.0),
                (29.0, 14.0),
                (29.0, 32.0),
                (22.0, 36.0),
                (15.0, 32.0),
                (15.0, 18.0),
            ],
            1.7,
        ),
        TrayMark::Mark => {
            inside_rect(px, py, 14.0, 11.0, 18.6, 33.0)
                || (inside_round(px, py, 14.0, 11.0, 31.0, 23.8, 6.2)
                    && !inside_round(px, py, 18.6, 14.4, 27.4, 20.6, 3.0)
                    && px >= 16.5)
        }
        TrayMark::Pages => {
            inside_round(px, py, 8.0, 15.0, 26.0, 36.0, 3.2)
                || inside_round(px, py, 16.0, 8.0, 36.0, 29.0, 3.2)
        }
        TrayMark::Pin => {
            let dx = px - 22.0;
            let dy = py - 12.0;
            dx * dx + dy * dy <= 20.0
                || inside_rect(px, py, 20.6, 15.0, 23.4, 34.5)
                || inside_rect(px, py, 15.0, 19.0, 29.0, 21.6)
        }
    }
}

fn on_stroke(px: f64, py: f64, points: &[(f64, f64)], width: f64) -> bool {
    points.windows(2).any(|segment| {
        dist_to_segment(
            px,
            py,
            segment[0].0,
            segment[0].1,
            segment[1].0,
            segment[1].1,
        ) <= width
    })
}

fn stroke_round(
    px: f64,
    py: f64,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    radius: f64,
    stroke: f64,
) -> bool {
    inside_round(px, py, left, top, right, bottom, radius)
        && !inside_round(
            px,
            py,
            left + stroke,
            top + stroke,
            right - stroke,
            bottom - stroke,
            (radius - stroke).max(0.0),
        )
}

fn inside_round(
    px: f64,
    py: f64,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    radius: f64,
) -> bool {
    if px < left || py < top || px > right || py > bottom {
        return false;
    }
    let cx = if px < left + radius {
        left + radius
    } else if px > right - radius {
        right - radius
    } else {
        px
    };
    let cy = if py < top + radius {
        top + radius
    } else if py > bottom - radius {
        bottom - radius
    } else {
        py
    };
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy <= radius * radius
}

fn inside_rect(px: f64, py: f64, left: f64, top: f64, right: f64, bottom: f64) -> bool {
    px >= left && py >= top && px <= right && py <= bottom
}

fn is_popup_menu_window(class_name: &str) -> bool {
    let name = class_name.rsplit('.').next().unwrap_or(class_name);
    name.ends_with("MenuWindow") && !name.contains("Status") && !name.contains("MenuBar")
}

fn plausible_object_pointer(value: usize) -> bool {
    value >= 0x1_0000_0000 && value.is_multiple_of(8) && value <= 0x0000_FFFF_FFFF_FFFF
}

fn ivar_byte_len(encoding: &CStr) -> usize {
    const FALLBACK: usize = 40;
    let Ok(text) = encoding.to_str() else {
        return FALLBACK;
    };
    let Some(rest) = text.strip_prefix('[') else {
        return FALLBACK;
    };
    let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) else {
        return FALLBACK;
    };
    let Ok(count) = rest[..end].parse::<usize>() else {
        return FALLBACK;
    };
    let unit = match rest.as_bytes().get(end).copied() {
        Some(b'C') => 1,
        Some(b'S') => 2,
        Some(b'I') => 4,
        Some(b'Q') => 8,
        _ => return FALLBACK,
    };
    let len = count.saturating_mul(unit);
    if len == 0 || len > 64 { FALLBACK } else { len }
}

#[cfg(target_os = "macos")]
fn fullscreen_menu_behavior() -> NSWindowCollectionBehavior {
    NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::FullScreenAuxiliary
        | NSWindowCollectionBehavior::Transient
        | NSWindowCollectionBehavior::IgnoresCycle
}

#[cfg(target_os = "macos")]
fn prepare_status_item(icon: &TrayIcon<Wry>) {
    let _ = icon.with_inner_tray_icon(|inner| {
        inner.set_show_menu_on_right_click(false);
        if let Some(item) = inner.ns_status_item() {
            allow_status_item_in_fullscreen(&item);
        }
    });
}

#[cfg(target_os = "macos")]
fn show_status_menu(icon: &TrayIcon<Wry>) {
    let icon = icon.clone();
    let _ = icon.with_inner_tray_icon(|inner| {
        let Some(item) = inner.ns_status_item() else {
            return;
        };
        allow_status_item_in_fullscreen(&item);
        // The menu window is created inside the tracking loop, so the level change
        // has to run there. A fullscreen app covers a menu left on the desktop space.
        let _ = DispatchQueue::main().exec_async(raise_popup_menus);
        if let Some(menu) = status_item_menu(&item) {
            item.setMenu(Some(&menu));
            #[allow(deprecated)]
            item.popUpStatusItemMenu(&menu);
            item.setMenu(None);
        } else {
            inner.show_menu();
        }
    });
    clear_status_highlight(icon);
}

#[cfg(target_os = "macos")]
fn clear_status_highlight(icon: TrayIcon<Wry>) {
    // The built-in click handler highlights the icon after this function returns.
    let _ = DispatchQueue::main().exec_async(move || {
        let _ = icon.with_inner_tray_icon(|inner| {
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            let Some(item) = inner.ns_status_item() else {
                return;
            };
            let Some(button) = item.button(mtm) else {
                return;
            };
            let _: () = unsafe { msg_send![&*button, highlight: false] };
        });
    });
}

#[cfg(target_os = "macos")]
fn allow_status_item_in_fullscreen(item: &NSStatusItem) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(button) = item.button(mtm) else {
        return;
    };
    let Some(window) = status_button_view(&button).window() else {
        return;
    };
    window.setCollectionBehavior(fullscreen_menu_behavior());
    window.setLevel(NSStatusWindowLevel);
    window.setHidesOnDeactivate(false);
}

#[cfg(target_os = "macos")]
fn status_button_view(button: &NSStatusBarButton) -> &NSView {
    unsafe { &*std::ptr::from_ref(button).cast::<NSView>() }
}

#[cfg(target_os = "macos")]
fn raise_popup_menus() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let windows = NSApplication::sharedApplication(mtm).windows();
    for index in 0..windows.count() {
        let window = windows.objectAtIndex(index);
        let Ok(name) = window.class().name().to_str() else {
            continue;
        };
        if !is_popup_menu_window(name) {
            continue;
        }
        window.setCollectionBehavior(fullscreen_menu_behavior());
        window.setLevel(NSPopUpMenuWindowLevel);
        window.setHidesOnDeactivate(false);
        if window.isVisible() {
            window.orderFrontRegardless();
        }
    }
}

#[cfg(target_os = "macos")]
fn status_item_menu(item: &NSStatusItem) -> Option<Retained<NSMenu>> {
    let mtm = MainThreadMarker::new()?;
    let button = item.button(mtm)?;
    let subviews = status_button_view(&button).subviews();
    for index in 0..subviews.count() {
        if let Some(menu) = menu_from_tray_target(&subviews.objectAtIndex(index)) {
            return Some(menu);
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn menu_from_tray_target(view: &NSView) -> Option<Retained<NSMenu>> {
    if view.class().name().to_bytes() != b"TaoTrayTarget" {
        return None;
    }
    let ivar = view.class().instance_variable(c"TaoTrayTarget_ivars")?;
    let menu_class = AnyClass::get(c"NSMenu")?;
    let len = ivar_byte_len(ivar.type_encoding());
    let start = unsafe { std::ptr::from_ref(view).cast::<u8>().offset(ivar.offset()) };
    let mut offset = 0;
    while offset + 8 <= len {
        let pointer = unsafe { start.add(offset).cast::<*mut AnyObject>().read_unaligned() };
        offset += 8;
        if !plausible_object_pointer(pointer as usize) {
            continue;
        }
        let is_menu: objc2::runtime::Bool =
            unsafe { msg_send![pointer, isKindOfClass: menu_class] };
        if !is_menu.as_bool() {
            continue;
        }
        return unsafe { Retained::retain(pointer.cast::<NSMenu>()) };
    }
    None
}

fn dist_to_segment(px: f64, py: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len2 = dx * dx + dy * dy;
    let t = (((px - x1) * dx + (py - y1) * dy) / len2).clamp(0.0, 1.0);
    let ox = px - (x1 + t * dx);
    let oy = py - (y1 + t * dy);
    (ox * ox + oy * oy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::{Glyph, TrayMark, glyph_alpha, main_request_path};

    #[test]
    fn paused_mark_is_only_on_the_paused_icon() {
        assert_eq!(glyph_alpha(Glyph::Recording, TrayMark::Board, 21, 21), 0);
        assert_eq!(glyph_alpha(Glyph::Disabled, TrayMark::Board, 21, 21), 0);
        assert!(glyph_alpha(Glyph::Paused, TrayMark::Board, 21, 21) > 200);
        let mut recording = 0u8;
        let mut disabled = 0u8;
        for y in 0..44 {
            for x in 0..44 {
                recording = recording.max(glyph_alpha(Glyph::Recording, TrayMark::Board, x, y));
                disabled = disabled.max(glyph_alpha(Glyph::Disabled, TrayMark::Board, x, y));
            }
        }
        assert_eq!(recording, 255);
        assert_eq!(disabled, 96);
    }

    #[test]
    fn menu_ids_distinguish_recording_from_clipboard_pause() {
        assert!(matches!(
            super::menu_action("tray-record"),
            super::MenuAction::Pause
        ));
        assert!(matches!(
            super::menu_action("tray-record-screen"),
            super::MenuAction::RecordScreen
        ));
        assert!(matches!(
            super::menu_action("tray-shot-12"),
            super::MenuAction::CopyShot(12)
        ));
        assert!(matches!(
            super::menu_action("tray-host-4"),
            super::MenuAction::OpenHost(4)
        ));
        assert!(matches!(
            super::menu_action("tray-docker-open"),
            super::MenuAction::OpenDocker
        ));
        assert!(matches!(
            super::menu_action("tray-endpoint-3"),
            super::MenuAction::OpenEndpoint(3)
        ));
        assert!(matches!(
            super::menu_action("tray-shot-nope"),
            super::MenuAction::Other
        ));
    }

    #[test]
    fn settings_menu_opens_the_general_tab() {
        assert_eq!(main_request_path("/settings"), "/settings");
        assert_eq!(
            main_request_path("/settings/clipboard"),
            "/settings/clipboard"
        );
        assert_eq!(
            main_request_path("/clipboard/settings"),
            "/settings/clipboard"
        );
        assert_eq!(
            main_request_path("/settings/screenshot"),
            "/settings/screenshot"
        );
        assert_eq!(main_request_path("/clipboard"), "/clipboard");
        assert_eq!(main_request_path("/other"), "/clipboard");
    }

    #[test]
    fn every_taskbar_mark_draws_and_unknown_names_are_rejected() {
        for mark in [
            TrayMark::Board,
            TrayMark::Clip,
            TrayMark::Mark,
            TrayMark::Pages,
            TrayMark::Pin,
        ] {
            let mut peak = 0u8;
            for y in 0..44 {
                for x in 0..44 {
                    peak = peak.max(glyph_alpha(Glyph::Recording, mark, x, y));
                }
            }
            assert_eq!(peak, 255);
        }
        assert!(TrayMark::parse("board").is_some());
        assert!(TrayMark::parse("dock").is_none());
    }

    #[test]
    fn popup_menu_windows_are_not_the_menu_bar() {
        assert!(super::is_popup_menu_window("NSMenuWindow"));
        assert!(super::is_popup_menu_window("_NSContextMenuWindow"));
        assert!(!super::is_popup_menu_window("NSMenuBarWindow"));
        assert!(!super::is_popup_menu_window("NSStatusBarWindow"));
        assert!(!super::is_popup_menu_window("NSPanel"));
    }

    #[test]
    fn menu_scan_skips_nulls_and_small_integers() {
        assert!(!super::plausible_object_pointer(0));
        assert!(!super::plausible_object_pointer(1));
        assert!(!super::plausible_object_pointer(0x1000));
        assert!(super::plausible_object_pointer(0x1_0000_0000));
        assert!(!super::plausible_object_pointer(0x1_0000_0001));
    }

    #[test]
    fn tray_target_ivar_length_comes_from_its_encoding() {
        use std::ffi::CString;
        let encoding = CString::new("[5Q]").unwrap();
        assert_eq!(super::ivar_byte_len(&encoding), 40);
        let encoding = CString::new("[40C]").unwrap();
        assert_eq!(super::ivar_byte_len(&encoding), 40);
        let encoding = CString::new("[100Q]").unwrap();
        assert_eq!(super::ivar_byte_len(&encoding), 40);
        let encoding = CString::new("not-an-array").unwrap();
        assert_eq!(super::ivar_byte_len(&encoding), 40);
    }
}
