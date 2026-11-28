use std::ffi::CStr;
use std::sync::Mutex;

#[cfg(target_os = "macos")]
use dispatch2::DispatchQueue;
#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::{AnyClass, AnyObject};
#[cfg(target_os = "macos")]
use objc2::{MainThreadMarker, Message, msg_send};
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
        .tooltip(tooltip_for(&labels, enabled, paused, false, 0, 0))
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
    let handle = app.clone();
    let _ = app.listen("ssh://sessions", move |_| refresh(&handle));
    let handle = app.clone();
    let _ = app.listen("docker://links", move |_| refresh(&handle));
    let handle = app.clone();
    let _ = app.listen("screenshot://record", move |_| refresh(&handle));
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
    #[serde(default)]
    tooltip_capture: String,
    #[serde(default)]
    tooltip_ssh: String,
    #[serde(default)]
    tooltip_docker: String,
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
            tooltip_capture: "正在录屏".into(),
            tooltip_ssh: "SSH {count}".into(),
            tooltip_docker: "Docker {count}".into(),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
        open_main(app, "/settings", false);
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
        MenuAction::History => {
            let clipboard_on = app
                .try_state::<ClipboardState>()
                .is_some_and(|state| state.status().enabled);
            if clipboard_on {
                open_main(app, "/clipboard", false);
            } else {
                open_main(app, "/settings", false);
            }
        }
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
        open_main(app, "/settings", false);
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
    let _ = host.icon.set_tooltip(Some(tooltip_for(
        &labels,
        status.enabled,
        status.paused,
        crate::screenshot::capture_active(),
        crate::ssh::live_sessions(app),
        crate::docker::live_links(app),
    )));
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrayEntry {
    Capture,
    RecordScreen,
    Shots,
    Hosts,
    Endpoints,
    ToolGap,
    QuitGap,
    Pause,
    Ignore,
    History,
    Settings,
    Quit,
}

fn tray_entries(clipboard: bool, screenshot: bool, ssh: bool, docker: bool) -> Vec<TrayEntry> {
    let mut entries = Vec::new();
    if screenshot {
        entries.push(TrayEntry::Capture);
        entries.push(TrayEntry::RecordScreen);
        entries.push(TrayEntry::Shots);
    }
    if ssh {
        entries.push(TrayEntry::Hosts);
    }
    if docker {
        entries.push(TrayEntry::Endpoints);
    }
    let other_tools = screenshot || ssh || docker;
    if other_tools && clipboard {
        entries.push(TrayEntry::ToolGap);
    }
    if clipboard {
        entries.push(TrayEntry::Pause);
        entries.push(TrayEntry::Ignore);
        entries.push(TrayEntry::History);
    }
    if other_tools || clipboard {
        entries.push(TrayEntry::QuitGap);
    }
    entries.push(TrayEntry::Settings);
    entries.push(TrayEntry::Quit);
    entries
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
    for entry in tray_entries(enabled, shots_on, hosts_on, docker_on) {
        match entry {
            TrayEntry::Capture => entries.push(&capture),
            TrayEntry::RecordScreen => entries.push(&record_screen),
            TrayEntry::Shots => entries.push(&shot_menu),
            TrayEntry::Hosts => entries.push(&host_menu),
            TrayEntry::Endpoints => entries.push(&endpoint_menu),
            TrayEntry::ToolGap => entries.push(&gap_tools),
            TrayEntry::QuitGap => entries.push(&gap_quit),
            TrayEntry::Pause => entries.push(&pause),
            TrayEntry::Ignore => entries.push(&ignore),
            TrayEntry::History => entries.push(&history),
            TrayEntry::Settings => entries.push(&settings),
            TrayEntry::Quit => entries.push(&quit),
        }
    }
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

fn tooltip_for(
    labels: &TrayLabels,
    enabled: bool,
    paused: bool,
    capture: bool,
    ssh: usize,
    docker: usize,
) -> String {
    let base = if !enabled {
        labels.tooltip_disabled.clone()
    } else if paused {
        labels.tooltip_paused.clone()
    } else {
        labels.tooltip_recording.clone()
    };
    let mut parts = Vec::new();
    if capture {
        parts.push(text_or(&labels.tooltip_capture, "正在录屏"));
    }
    if ssh > 0 {
        parts
            .push(text_or(&labels.tooltip_ssh, "SSH {count}").replace("{count}", &ssh.to_string()));
    }
    if docker > 0 {
        parts.push(
            text_or(&labels.tooltip_docker, "Docker {count}")
                .replace("{count}", &docker.to_string()),
        );
    }
    if parts.is_empty() {
        base
    } else {
        parts.push(base);
        parts.join(" · ")
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
    // The menu-bar image is 44px. Icons are drawn on the same 24-unit grid as the settings preview.
    let x = px * 24.0 / 44.0;
    let y = py * 24.0 / 44.0;
    match mark {
        TrayMark::Board => {
            let board = svg_stroke_round(x, y, 5.0, 6.4, 19.0, 20.0, 2.2, 2.0)
                && !(x >= 8.8 && x <= 15.2 && y <= 7.4);
            let clip = svg_stroke_round(x, y, 8.8, 3.6, 15.2, 8.2, 1.5, 2.0);
            board
                || clip
                || on_stroke(x, y, &[(8.3, 12.2), (15.7, 12.2)], 0.825)
                || on_stroke(x, y, &[(8.3, 15.4), (13.2, 15.4)], 0.825)
        }
        TrayMark::Clip => on_stroke(x, y, &paperclip(), 1.0),
        TrayMark::Mark => on_stroke(x, y, &letter_p(), 1.2),
        TrayMark::Pages => {
            let covered = inside_round(x, y, 7.8, 7.4, 20.3, 20.6, 3.0);
            let back = svg_stroke_round(x, y, 4.5, 4.0, 15.0, 15.2, 2.0, 2.0) && !covered;
            let front = svg_stroke_round(x, y, 8.8, 8.4, 19.3, 19.6, 2.0, 2.0);
            back || front
                || on_stroke(x, y, &[(11.4, 12.6), (16.4, 12.6)], 0.8)
                || on_stroke(x, y, &[(11.4, 15.5), (14.2, 15.5)], 0.8)
        }
        TrayMark::Pin => {
            let head = svg_stroke_round(x, y, 6.9, 3.6, 17.1, 9.2, 2.8, 2.0);
            let needle = on_stroke(x, y, &[(12.0, 9.2), (12.0, 20.4)], 1.0);
            head || needle
        }
    }
}

fn paperclip() -> Vec<(f64, f64)> {
    let mut points = vec![(6.8, 7.6), (6.8, 14.6)];
    arc(&mut points, 11.6, 14.6, 4.8, std::f64::consts::PI, 0.0, 12);
    points.push((16.4, 6.6));
    arc(&mut points, 13.3, 6.6, 3.1, 0.0, -std::f64::consts::PI, 10);
    points.push((10.2, 14.8));
    arc(&mut points, 11.7, 14.8, 1.5, std::f64::consts::PI, 0.0, 8);
    points.push((13.2, 9.6));
    points
}

fn letter_p() -> Vec<(f64, f64)> {
    let mut points = vec![(6.9, 19.6), (6.9, 4.4), (12.1, 4.4)];
    arc(
        &mut points,
        12.1,
        9.4,
        5.0,
        -std::f64::consts::FRAC_PI_2,
        std::f64::consts::FRAC_PI_2,
        16,
    );
    points.push((6.9, 14.4));
    points
}

fn svg_stroke_round(
    px: f64,
    py: f64,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    radius: f64,
    stroke: f64,
) -> bool {
    let half = stroke / 2.0;
    stroke_round(
        px,
        py,
        left - half,
        top - half,
        right + half,
        bottom + half,
        radius + half,
        stroke,
    )
}

fn arc(
    points: &mut Vec<(f64, f64)>,
    cx: f64,
    cy: f64,
    radius: f64,
    start: f64,
    end: f64,
    steps: usize,
) {
    for step in 0..=steps {
        let angle = start + (end - start) * (step as f64) / (steps as f64);
        points.push((cx + radius * angle.cos(), cy + radius * angle.sin()));
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
struct StatusButton(*mut NSView);

#[cfg(target_os = "macos")]
unsafe impl Send for StatusButton {}

#[cfg(target_os = "macos")]
static STATUS_BUTTON: Mutex<StatusButton> = Mutex::new(StatusButton(std::ptr::null_mut()));

#[cfg(target_os = "macos")]
fn remember_status_button(button: &NSStatusBarButton) {
    let mut slot = match STATUS_BUTTON.lock() {
        Ok(slot) => slot,
        Err(poisoned) => poisoned.into_inner(),
    };
    if !slot.0.is_null() {
        return;
    }
    slot.0 = Retained::into_raw(status_button_view(button).retain());
}

/// Screen rectangle of the menu-bar icon, in AppKit points, grown slightly so a
/// click on its highlight still counts as the icon.
#[cfg(target_os = "macos")]
pub(crate) fn status_button_frame() -> Option<(f64, f64, f64, f64)> {
    let slot = match STATUS_BUTTON.lock() {
        Ok(slot) => slot,
        Err(poisoned) => poisoned.into_inner(),
    };
    if slot.0.is_null() {
        return None;
    }
    let view = unsafe { &*slot.0 };
    let window = view.window()?;
    let screen = window.convertRectToScreen(view.convertRect_toView(view.bounds(), None));
    let slop = 4.0;
    Some((
        screen.origin.x - slop,
        screen.origin.y - slop,
        screen.size.width + slop * 2.0,
        screen.size.height + slop * 2.0,
    ))
}

#[cfg(target_os = "macos")]
fn allow_status_item_in_fullscreen(item: &NSStatusItem) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(button) = item.button(mtm) else {
        return;
    };
    remember_status_button(&button);
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
    use super::{Glyph, TrayEntry, TrayMark, glyph_alpha, main_request_path, tray_entries};

    #[test]
    fn paused_slash_appears_only_on_the_paused_icon() {
        let mut slash = 0;
        let mut recording = 0u8;
        let mut disabled = 0u8;
        for y in 0..44 {
            for x in 0..44 {
                let live = glyph_alpha(Glyph::Recording, TrayMark::Board, x, y);
                let paused = glyph_alpha(Glyph::Paused, TrayMark::Board, x, y);
                let off = glyph_alpha(Glyph::Disabled, TrayMark::Board, x, y);
                if live == 0 && paused > 200 {
                    slash += 1;
                }
                if live > 0 {
                    assert_eq!(off, 96);
                } else {
                    assert_eq!(off, 0);
                }
                recording = recording.max(live);
                disabled = disabled.max(off);
            }
        }
        assert_eq!(recording, 255);
        assert_eq!(disabled, 96);
        assert!(
            slash > 20,
            "the pause slash should stay visible beside the mark"
        );
    }

    #[test]
    fn taskbar_marks_share_one_optical_frame() {
        for mark in [
            TrayMark::Board,
            TrayMark::Clip,
            TrayMark::Mark,
            TrayMark::Pages,
            TrayMark::Pin,
        ] {
            let mut ink = 0;
            let mut min_x = 44u32;
            let mut min_y = 44u32;
            let mut max_x = 0u32;
            let mut max_y = 0u32;
            for y in 0..44 {
                for x in 0..44 {
                    if glyph_alpha(Glyph::Recording, mark, x, y) == 0 {
                        continue;
                    }
                    ink += 1;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
            assert!(
                ink > 120 && ink < 1100,
                "{mark:?} has {ink} pixels, outside the shared weight"
            );
            assert!(
                min_x >= 3 && min_y >= 3,
                "{mark:?} touches the top or left edge at {min_x},{min_y}"
            );
            assert!(
                max_x <= 40 && max_y <= 40,
                "{mark:?} touches the bottom or right edge at {max_x},{max_y}"
            );
            let width = max_x - min_x;
            let height = max_y - min_y;
            assert!(
                width >= 14 && height >= 24,
                "{mark:?} is {width}x{height}, too small for the menu bar"
            );
        }
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
    fn a_turned_off_tool_leaves_the_tray() {
        let clipboard_off = tray_entries(false, true, true, true);
        assert!(!clipboard_off.contains(&TrayEntry::Pause));
        assert!(!clipboard_off.contains(&TrayEntry::Ignore));
        assert!(!clipboard_off.contains(&TrayEntry::History));
        assert!(clipboard_off.contains(&TrayEntry::Capture));
        assert!(clipboard_off.contains(&TrayEntry::Hosts));
        assert!(clipboard_off.contains(&TrayEntry::Endpoints));
        assert_eq!(
            tray_entries(true, false, false, false),
            vec![
                TrayEntry::Pause,
                TrayEntry::Ignore,
                TrayEntry::History,
                TrayEntry::QuitGap,
                TrayEntry::Settings,
                TrayEntry::Quit,
            ]
        );
        assert_eq!(
            tray_entries(false, false, false, false),
            vec![TrayEntry::Settings, TrayEntry::Quit]
        );
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

    #[test]
    fn tooltip_puts_live_work_ahead_of_the_clipboard_state() {
        let labels = super::TrayLabels::default();
        assert_eq!(
            super::tooltip_for(&labels, true, false, false, 0, 0),
            "正在记录剪贴板"
        );
        assert_eq!(
            super::tooltip_for(&labels, false, false, true, 2, 1),
            "正在录屏 · SSH 2 · Docker 1 · 剪贴板记录未开启"
        );
    }
}
