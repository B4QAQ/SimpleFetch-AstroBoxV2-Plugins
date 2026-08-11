use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::ui::persist;

/// 主Tab枚举
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MainTab {
    Connect,
    About,
}

/// 已监听的应用条目
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppEntry {
    pub pkg_name: String,
    pub enabled: bool,
    #[serde(default)]
    pub request_count: u64,
    #[serde(default)]
    pub success_count: u64,
    #[serde(default)]
    pub error_count: u64,
    #[serde(default)]
    pub last_seen_unix_ms: Option<u128>,
    #[serde(default)]
    pub last_addr: Option<String>,
    #[serde(default)]
    pub last_status: Option<String>,
    #[serde(default)]
    pub last_url: Option<String>,
}

impl AppEntry {
    pub fn new(pkg_name: &str) -> Self {
        Self {
            pkg_name: pkg_name.to_string(),
            enabled: true,
            request_count: 0,
            success_count: 0,
            error_count: 0,
            last_seen_unix_ms: None,
            last_addr: None,
            last_status: None,
            last_url: None,
        }
    }
}

/// 设备上安装的第三方快应用
#[derive(Clone, Debug)]
pub struct InstalledApp {
    pub addr: String,
    pub device_name: String,
    pub package_name: String,
    pub app_name: String,
    pub version_code: u32,
}

/// 插件全局状态
pub struct PluginState {
    /// 当前Tab
    pub current_tab: MainTab,
    /// 已监听的应用（按包名索引）
    pub apps: HashMap<String, AppEntry>,
    /// 根元素ID
    pub root_element_id: Option<String>,
    /// 通知消息
    pub last_notice: Option<String>,
    /// 手动添加包名的输入框内容
    pub pending_add_pkg: String,
    /// 已连接设备列表 (addr, name)
    pub connected_devices: Vec<(String, String)>,
    /// 所有已安装的第三方快应用
    pub installed_apps: Vec<InstalledApp>,
    /// 上次自动刷新时间戳（节流用）
    pub last_auto_refresh_ms: u128,
    /// 渲染序列号（确保UI更新反映最新状态）
    pub render_tick: u64,
    /// 设备列表是否正在加载
    pub devices_loading: bool,
    /// 应用列表是否正在加载
    pub apps_loading: bool,
}

static STATE: OnceLock<Mutex<PluginState>> = OnceLock::new();

pub fn with_state<R>(f: impl FnOnce(&mut PluginState) -> R) -> R {
    let mutex = STATE.get_or_init(|| {
        Mutex::new(PluginState {
            current_tab: MainTab::Connect,
            apps: HashMap::new(),
            root_element_id: None,
            last_notice: None,
            pending_add_pkg: String::new(),
            connected_devices: Vec::new(),
            installed_apps: Vec::new(),
            last_auto_refresh_ms: 0,
            render_tick: 0,
            devices_loading: false,
            apps_loading: false,
        })
    });
    let mut guard = mutex.lock().unwrap_or_else(|p| p.into_inner());
    f(&mut guard)
}

pub fn ensure_app(pkg_name: &str) {
    let inserted = with_state(|state| {
        let before = state.apps.contains_key(pkg_name);
        state
            .apps
            .entry(pkg_name.to_string())
            .or_insert_with(|| AppEntry::new(pkg_name));
        !before
    });
    if inserted {
        persist_now();
    }
}

/// 启动时批量加载应用条目
pub fn install_loaded_apps(entries: Vec<AppEntry>) {
    with_state(|state| {
        for entry in entries {
            state.apps.insert(entry.pkg_name.clone(), entry);
        }
        state.render_tick = state.render_tick.wrapping_add(1);
    });
}

pub fn is_enabled(pkg_name: &str) -> bool {
    with_state(|state| {
        state
            .apps
            .get(pkg_name)
            .map(|e| e.enabled)
            .unwrap_or(true)
    })
}

pub fn record_request(pkg_name: &str, addr: &str, url: Option<&str>) {
    let now_ms = now_unix_ms();
    with_state(|state| {
        let entry = state
            .apps
            .entry(pkg_name.to_string())
            .or_insert_with(|| AppEntry::new(pkg_name));
        entry.request_count = entry.request_count.saturating_add(1);
        entry.last_seen_unix_ms = Some(now_ms);
        entry.last_addr = Some(addr.to_string());
        if let Some(url) = url {
            entry.last_url = Some(url.to_string());
        }
        state.render_tick = state.render_tick.wrapping_add(1);
    });
    persist_now();
}

pub fn record_result(pkg_name: &str, ok: bool, status: Option<String>) {
    with_state(|state| {
        let entry = state
            .apps
            .entry(pkg_name.to_string())
            .or_insert_with(|| AppEntry::new(pkg_name));
        if ok {
            entry.success_count = entry.success_count.saturating_add(1);
        } else {
            entry.error_count = entry.error_count.saturating_add(1);
        }
        if let Some(status) = status {
            entry.last_status = Some(status);
        }
        state.render_tick = state.render_tick.wrapping_add(1);
    });
    persist_now();
}

pub fn persist_now() {
    let entries = snapshot_apps();
    persist::save_apps(&entries);
}

pub fn set_enabled(pkg_name: &str, enabled: bool) {
    with_state(|state| {
        let entry = state
            .apps
            .entry(pkg_name.to_string())
            .or_insert_with(|| AppEntry::new(pkg_name));
        entry.enabled = enabled;
        state.render_tick = state.render_tick.wrapping_add(1);
    });
    persist_now();
}

pub fn remove_app(pkg_name: &str) {
    let removed = with_state(|state| {
        let removed = state.apps.remove(pkg_name).is_some();
        state.render_tick = state.render_tick.wrapping_add(1);
        removed
    });
    if removed {
        persist_now();
    }
}

pub fn snapshot_apps() -> Vec<AppEntry> {
    with_state(|state| {
        let mut list: Vec<AppEntry> = state.apps.values().cloned().collect();
        list.sort_by(|a, b| a.pkg_name.cmp(&b.pkg_name));
        list
    })
}

pub fn pkg_names() -> Vec<String> {
    with_state(|state| {
        let mut list: Vec<String> = state.apps.keys().cloned().collect();
        list.sort();
        list
    })
}

pub fn set_notice(msg: impl Into<String>) {
    with_state(|state| {
        state.last_notice = Some(msg.into());
        state.render_tick = state.render_tick.wrapping_add(1);
    });
}

pub fn clear_notice() {
    with_state(|state| {
        state.last_notice = None;
    });
}

pub fn set_pending_add(value: String) {
    with_state(|state| state.pending_add_pkg = value);
}

pub fn take_pending_add() -> String {
    with_state(|state| std::mem::take(&mut state.pending_add_pkg))
}

pub fn set_root(element_id: &str) {
    with_state(|state| state.root_element_id = Some(element_id.to_string()));
}

pub fn root() -> Option<String> {
    with_state(|state| state.root_element_id.clone())
}

pub fn set_connected_devices(devices: Vec<(String, String)>) {
    with_state(|state| {
        state.connected_devices = devices;
        state.devices_loading = false;
        state.render_tick = state.render_tick.wrapping_add(1);
    });
}

pub fn connected_devices() -> Vec<(String, String)> {
    with_state(|state| state.connected_devices.clone())
}

pub fn set_installed_apps(apps: Vec<InstalledApp>) {
    with_state(|state| {
        state.installed_apps = apps;
        state.apps_loading = false;
        state.render_tick = state.render_tick.wrapping_add(1);
    });
}

pub fn installed_apps() -> Vec<InstalledApp> {
    with_state(|state| state.installed_apps.clone())
}

pub fn is_monitored(pkg: &str) -> bool {
    with_state(|state| state.apps.contains_key(pkg))
}

/// 节流自动刷新：返回true表示允许执行
pub fn try_claim_auto_refresh(min_interval_ms: u128) -> bool {
    let now = now_unix_ms();
    with_state(|state| {
        if now.saturating_sub(state.last_auto_refresh_ms) < min_interval_ms {
            return false;
        }
        state.last_auto_refresh_ms = now;
        true
    })
}

pub fn set_devices_loading(loading: bool) {
    with_state(|state| state.devices_loading = loading);
}

pub fn set_apps_loading(loading: bool) {
    with_state(|state| state.apps_loading = loading);
}

pub fn is_devices_loading() -> bool {
    with_state(|state| state.devices_loading)
}

pub fn is_apps_loading() -> bool {
    with_state(|state| state.apps_loading)
}

pub fn first_device_addr_for(pkg_name: &str) -> Option<String> {
    with_state(|state| {
        if let Some(entry) = state.apps.get(pkg_name) {
            if let Some(addr) = entry.last_addr.clone() {
                return Some(addr);
            }
        }
        state.connected_devices.first().map(|(a, _)| a.clone())
    })
}

pub fn now_unix_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}
