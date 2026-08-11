//! 事件处理：Interconnect 消息分发、SimpleFetch(SF_)协议、UI事件、设备/应用管理。

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use base64::Engine;
use serde_json::{json, Value};

use crate::astrobox::psys_host;
use crate::astrobox::psys_host::dialog;
use crate::astrobox::psys_host::interconnect;
use crate::astrobox::psys_host::register;
use crate::astrobox::psys_host::thirdpartyapp;
use crate::ui::api_client;
use crate::ui::state::{self, InstalledApp, MainTab};

// ========== 事件ID常量 ==========

pub const TAB_CONNECT_EVENT: &str = "tab_connect";
pub const TAB_ABOUT_EVENT: &str = "tab_about";
pub const EVENT_REFRESH_DEVICES: &str = "action:devices.refresh";
pub const EVENT_CLEAR_NOTICE: &str = "action:notice.clear";
pub const EVENT_ADD_PKG_INPUT: &str = "input:add-pkg";
pub const EVENT_ADD_PKG_SUBMIT: &str = "action:add-pkg.submit";
pub const PKG_TOGGLE_PREFIX: &str = "toggle:pkg:";
pub const PKG_REMOVE_PREFIX: &str = "action:pkg.remove:";
pub const PKG_REREGISTER_PREFIX: &str = "action:pkg.reregister:";
pub const PKG_PICK_PREFIX: &str = "action:pkg.pick:";
pub const OPEN_HELP_DOC_EVENT: &str = "open_help_doc";
pub const OPEN_QQ_GROUP_EVENT: &str = "open_qq_group";

/// 小响应体阈值（≤16KB 单条文本返回）
const SMALL_BODY_THRESHOLD: usize = 16 * 1024;
/// 分片时每片 base64 字符数
const CHUNK_SIZE: usize = 12 * 1024;
/// 自动刷新节流间隔
const AUTO_REFRESH_THROTTLE_MS: u128 = 1500;

/// 活跃的 SSE 流取消标志，按请求ID索引
static SSE_CANCELS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();

fn sse_cancels() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    SSE_CANCELS.get_or_init(|| Mutex::new(HashMap::new()))
}

// ========== Interconnect 消息处理 ==========

/// AstroBox 将互联消息包装为信封：
/// `{ "addr": "...", "pkgName": "...", "payloadText": "..." }`
struct ParsedMessage {
    addr: String,
    pkg_name: String,
    data: String,
}

fn parse_message(payload: &str) -> ParsedMessage {
    let envelope: Option<Value> = serde_json::from_str(payload).ok();

    // 提取实际消息内容
    let data = envelope
        .as_ref()
        .and_then(|v| v.get("payloadText").and_then(|x| x.as_str()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| payload.to_string());

    let pkg_name = envelope
        .as_ref()
        .and_then(|v| v.get("pkgName").and_then(|x| x.as_str()))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| guess_pkg_name(&data));

    let addr = envelope
        .as_ref()
        .and_then(|v| v.get("addr").and_then(|x| x.as_str()))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| state::first_device_addr_for(&pkg_name).unwrap_or_default());

    ParsedMessage {
        addr,
        pkg_name,
        data,
    }
}

/// 旧版宿主缺少包名时的回退猜测：取最近活跃的已启用应用
fn guess_pkg_name(data: &str) -> String {
    let _ = data;
    let apps = state::snapshot_apps();
    let mut enabled: Vec<_> = apps.iter().filter(|a| a.enabled).collect();
    enabled.sort_by(|a, b| b.last_seen_unix_ms.cmp(&a.last_seen_unix_ms));
    if let Some(top) = enabled.first() {
        return top.pkg_name.clone();
    }
    String::new()
}

/// 处理来自快应用的互联消息
pub fn handle_interconnect_message(payload: &str) {
    let parsed = parse_message(payload);
    if parsed.addr.is_empty() || parsed.pkg_name.is_empty() {
        tracing::warn!("丢弃无法定位来源的互联消息");
        return;
    }

    // 未启用的应用直接丢弃
    if !state::is_enabled(&parsed.pkg_name) {
        tracing::warn!(
            "消息被丢弃（应用已禁用）: pkg={} addr={}",
            parsed.pkg_name,
            parsed.addr
        );
        return;
    }

    state::ensure_app(&parsed.pkg_name);

    let msg: Value = match serde_json::from_str(&parsed.data) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("解析SF消息失败: {} raw={}", e, parsed.data);
            return;
        }
    };

    let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let status = msg.get("status").and_then(|v| v.as_str()).unwrap_or("");
    let data = msg.get("data").cloned();

    tracing::info!(
        "SF消息: type={} status={} pkg={} addr={}",
        msg_type,
        status,
        parsed.pkg_name,
        parsed.addr
    );

    dispatch_sf(&parsed.addr, &parsed.pkg_name, msg_type, status, data);

    crate::ui::build::render_without_auto_refresh();
}

/// 按 SF_ 消息类型分发
fn dispatch_sf(addr: &str, pkg: &str, msg_type: &str, status: &str, data: Option<Value>) {
    match msg_type {
        "SF_HANDSHAKE_ACK" => {
            // 握手确认，桥接建立
            if status == "OK" {
                tracing::info!("握手成功: pkg={} addr={}", pkg, addr);
                state::set_notice(format!("已连接 {}", pkg));
            }
        }
        "SF_PING" => {
            // 心跳：原样回传 ts
            let ts = data
                .as_ref()
                .and_then(|d| d.get("ts"))
                .cloned()
                .unwrap_or(json!(0));
            send_json(
                addr,
                pkg,
                json!({
                    "type": "SF_PONG",
                    "status": "OK",
                    "data": { "ts": ts }
                }),
            );
        }
        "SF_REQUEST" => {
            if let Some(d) = data {
                handle_sf_request(addr, pkg, d);
            }
        }
        "SF_CLOSE" => {
            // 快应用主动关闭 SSE/取消请求
            if let Some(id) = data.and_then(|d| d.get("id").and_then(|v| v.as_str()).map(String::from)) {
                cancel_sse(&id);
                tracing::info!("收到SF_CLOSE，已取消请求: id={}", id);
            }
        }
        _ => {
            tracing::info!("未处理的SF消息类型: {}", msg_type);
        }
    }
}

// ========== SF_REQUEST 处理 ==========

fn handle_sf_request(addr: &str, pkg: &str, data: Value) {
    // 解析请求字段
    let id = data.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let url = match data.get("url").and_then(|v| v.as_str()) {
        Some(u) => u.to_string(),
        None => {
            send_error(addr, pkg, &id, 0, "请求缺少url字段");
            return;
        }
    };
    let method = data
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("GET")
        .to_string();
    let is_sse = data.get("sse").and_then(|v| v.as_bool()).unwrap_or(false);
    let timeout_ms = data
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(15000) as u32;

    // 解析请求头
    let headers = parse_headers(data.get("headers"));
    // 解析请求体
    let body_bytes = data
        .get("body")
        .and_then(|v| v.as_str())
        .map(|s| s.as_bytes().to_vec());

    state::record_request(pkg, addr, Some(&url));

    let addr = addr.to_string();
    let pkg = pkg.to_string();

    if is_sse {
        // SSE 流式请求：spawn 避免长时间阻塞事件分发
        spawn_sse(addr, pkg, id, method, url, headers, body_bytes, timeout_ms);
    } else {
        // 普通请求：同步执行并回送响应（与参考插件一致，
        // execute_request 内部的 WASI 阻塞读取会自行等待并驱动回调）
        execute_and_respond(&addr, &pkg, &id, &method, &url, &headers, body_bytes.as_deref(), timeout_ms);
    }
}

/// 执行普通请求并回送响应（含分片）
fn execute_and_respond(
    addr: &str,
    pkg: &str,
    id: &str,
    method: &str,
    url: &str,
    headers: &HashMap<String, String>,
    body: Option<&[u8]>,
    timeout_ms: u32,
) {
    match api_client::execute_request(method, url, headers, body, timeout_ms) {
        Ok(resp) => {
            let ok = (200..300).contains(&resp.status_code);
            state::record_result(pkg, ok, Some(format!("HTTP {}", resp.status_code)));

            // 过滤掉 hop-by-hop 响应头
            let resp_headers = filter_response_headers(&resp.headers);

            send_response(addr, pkg, id, resp.status_code, &resp_headers, &resp.body);
        }
        Err(e) => {
            tracing::error!("HTTP请求失败: {}", e);
            state::record_result(pkg, false, Some(e.clone()));
            let err_msg = classify_network_error(&e);
            send_error(addr, pkg, id, 0, &err_msg);
        }
    }
}

/// 发送响应，自动处理小数据/大数据分片
fn send_response(
    addr: &str,
    pkg: &str,
    id: &str,
    status_code: u16,
    headers: &HashMap<String, String>,
    body: &[u8],
) {
    // 小数据且为有效UTF-8：单条文本返回
    if body.len() <= SMALL_BODY_THRESHOLD {
        if let Ok(text) = std::str::from_utf8(body) {
            send_json(
                addr,
                pkg,
                json!({
                    "type": "SF_RESPONSE",
                    "status": "OK",
                    "data": {
                        "id": id,
                        "statusCode": status_code,
                        "headers": headers,
                        "body": text,
                        "chunk": 0,
                        "totalChunks": 0
                    }
                }),
            );
            return;
        }
    }

    // 大数据：base64 编码后分片
    let encoded = base64::engine::general_purpose::STANDARD.encode(body);
    let total_chunks = (encoded.len() + CHUNK_SIZE - 1) / CHUNK_SIZE;

    for i in 0..total_chunks {
        let start = i * CHUNK_SIZE;
        let end = (start + CHUNK_SIZE).min(encoded.len());
        let chunk_body = &encoded[start..end];
        let chunk_num = i + 1; // 从1开始

        if chunk_num == 1 {
            // 第一片包含 statusCode 和 headers
            send_json(
                addr,
                pkg,
                json!({
                    "type": "SF_RESPONSE",
                    "status": "OK",
                    "data": {
                        "id": id,
                        "statusCode": status_code,
                        "headers": headers,
                        "body": chunk_body,
                        "chunk": chunk_num,
                        "totalChunks": total_chunks
                    }
                }),
            );
        } else {
            send_json(
                addr,
                pkg,
                json!({
                    "type": "SF_RESPONSE",
                    "status": "OK",
                    "data": {
                        "id": id,
                        "body": chunk_body,
                        "chunk": chunk_num,
                        "totalChunks": total_chunks
                    }
                }),
            );
        }
    }

    tracing::info!(
        "分片响应完成: id={} chunks={} body_len={}",
        id,
        total_chunks,
        body.len()
    );
}

/// 发送错误响应
fn send_error(addr: &str, pkg: &str, id: &str, status_code: u16, error: &str) {
    send_json(
        addr,
        pkg,
        json!({
            "type": "SF_RESPONSE",
            "status": error,
            "data": {
                "id": id,
                "statusCode": status_code,
                "error": error
            }
        }),
    );
}

// ========== SSE 流式请求 ==========

fn spawn_sse(
    addr: String,
    pkg: String,
    id: String,
    method: String,
    url: String,
    headers: HashMap<String, String>,
    body: Option<Vec<u8>>,
    timeout_ms: u32,
) {
    // 注册取消标志
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut cancels = sse_cancels().lock().unwrap_or_else(|p| p.into_inner());
        cancels.insert(id.clone(), cancel.clone());
    }

    let cancel_check = cancel.clone();
    let id_for_task = id.clone();
    let addr_task = addr.clone();
    let pkg_task = pkg.clone();

    wit_bindgen::spawn(async move {
        let result = api_client::execute_sse(
            &method,
            &url,
            &headers,
            body.as_deref(),
            timeout_ms,
            &mut |event, data| {
                // 每个事件转发为 SF_SSE_EVENT
                send_json(
                    &addr_task,
                    &pkg_task,
                    json!({
                        "type": "SF_SSE_EVENT",
                        "data": {
                            "id": id_for_task,
                            "event": if event.is_empty() { "message" } else { event },
                            "data": data
                        }
                    }),
                );
                // 检查是否被取消
                !cancel_check.load(Ordering::SeqCst)
            },
            &|| cancel_check.load(Ordering::SeqCst),
        );

        // 清理取消标志
        {
            let mut cancels = sse_cancels().lock().unwrap_or_else(|p| p.into_inner());
            cancels.remove(&id_for_task);
        }

        match result {
            Ok(()) => {
                // 流正常结束
                send_json(
                    &addr_task,
                    &pkg_task,
                    json!({
                        "type": "SF_SSE_END",
                        "data": { "id": id_for_task }
                    }),
                );
                state::record_result(&pkg_task, true, Some("SSE完成".to_string()));
            }
            Err(e) => {
                if cancel.load(Ordering::SeqCst) {
                    // 被主动取消，也发送 END
                    send_json(
                        &addr_task,
                        &pkg_task,
                        json!({
                            "type": "SF_SSE_END",
                            "data": { "id": id_for_task }
                        }),
                    );
                } else {
                    tracing::error!("SSE错误: {}", e);
                    state::record_result(&pkg_task, false, Some(e.clone()));
                    send_json(
                        &addr_task,
                        &pkg_task,
                        json!({
                            "type": "SF_SSE_ERROR",
                            "status": e,
                            "data": {
                                "id": id_for_task,
                                "error": e
                            }
                        }),
                    );
                }
            }
        }

        crate::ui::build::render_without_auto_refresh();
    });
}

/// 取消指定 SSE 流
fn cancel_sse(id: &str) {
    let cancels = sse_cancels().lock().unwrap_or_else(|p| p.into_inner());
    if let Some(flag) = cancels.get(id) {
        flag.store(true, Ordering::SeqCst);
    }
}

// ========== 辅助函数 ==========

/// 解析请求头为 HashMap
fn parse_headers(value: Option<&Value>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(obj) = value.and_then(|v| v.as_object()) {
        for (k, v) in obj {
            if let Some(s) = v.as_str() {
                map.insert(k.clone(), s.to_string());
            } else {
                map.insert(k.clone(), v.to_string());
            }
        }
    }
    map
}

/// 过滤掉逐跳响应头（不应转发给快应用）
fn filter_response_headers(headers: &HashMap<String, String>) -> HashMap<String, String> {
    const HOP_BY_HOP: &[&str] = &[
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailers",
        "transfer-encoding",
        "upgrade",
        "content-encoding",
        "content-length",
    ];
    headers
        .iter()
        .filter(|(k, _)| !HOP_BY_HOP.contains(&k.to_lowercase().as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// 将底层错误分类为中文网络错误描述
fn classify_network_error(e: &str) -> String {
    let lower = e.to_lowercase();
    if lower.contains("timeout") || lower.contains("超时") {
        "请求超时".to_string()
    } else if lower.contains("dns") || lower.contains("resolve") {
        "DNS解析失败".to_string()
    } else if lower.contains("refused") || lower.contains("拒绝") {
        "连接被拒绝".to_string()
    } else if lower.contains("unreachable") || lower.contains("network") {
        "网络不可达".to_string()
    } else {
        format!("网络错误: {}", e)
    }
}

/// 发送 JSON 消息到指定设备和包名
fn send_json(addr: &str, pkg_name: &str, message: Value) {
    let text = message.to_string();
    tracing::debug!("发送SF消息: addr={} pkg={} len={}", addr, pkg_name, text.len());

    let addr_owned = addr.to_string();
    let pkg_owned = pkg_name.to_string();
    let result = wit_bindgen::block_on(async move {
        interconnect::send_qaic_message(&addr_owned, &pkg_owned, &text).await
    });
    if let Err(e) = result {
        tracing::error!("发送SF消息失败: addr={} pkg={} err={:?}", addr, pkg_name, e);
    }
}

// ========== 设备/应用管理 ==========

/// 启动时的初始刷新：填充设备和应用缓存，不弹通知
pub fn initial_refresh() {
    refresh_connected_devices();
    refresh_installed_apps();
    for pkg in state::pkg_names() {
        register_for_all_devices(&pkg);
    }
    crate::ui::build::render_without_auto_refresh();
}

/// 自动刷新（节流）：刷新设备列表、应用列表、重注册接收器
pub fn auto_refresh() -> bool {
    if !state::try_claim_auto_refresh(AUTO_REFRESH_THROTTLE_MS) {
        return false;
    }
    refresh_connected_devices();
    refresh_installed_apps();
    for pkg in state::pkg_names() {
        register_for_all_devices(&pkg);
    }
    true
}

/// 手动刷新设备和应用列表
pub fn refresh_device_list() {
    state::set_devices_loading(true);
    state::set_apps_loading(true);
    crate::ui::build::render_without_auto_refresh();

    refresh_connected_devices();
    refresh_installed_apps();
    for pkg in state::pkg_names() {
        register_for_all_devices(&pkg);
    }

    state::set_devices_loading(false);
    state::set_apps_loading(false);

    let device_count = state::connected_devices().len();
    let app_count = state::installed_apps().len();
    state::set_notice(format!(
        "已刷新：连接 {} 台设备 · 共 {} 个应用",
        device_count, app_count
    ));
    crate::ui::build::render_without_auto_refresh();
}

/// 刷新已连接设备列表
fn refresh_connected_devices() {
    let devices = wit_bindgen::block_on(async move {
        psys_host::device::get_connected_device_list().await
    });
    let list: Vec<(String, String)> = devices
        .into_iter()
        .map(|info| (info.addr, info.name))
        .collect();
    tracing::info!("已连接设备: {} 台", list.len());
    state::set_connected_devices(list);
}

/// 刷新所有设备上的第三方快应用列表
fn refresh_installed_apps() {
    let devices = state::connected_devices();
    let mut out: Vec<InstalledApp> = Vec::new();
    let mut queried_addrs: HashSet<String> = HashSet::new();

    for (addr, device_name) in &devices {
        let addr_for_call = addr.clone();
        let result = wit_bindgen::block_on(async move {
            thirdpartyapp::get_thirdparty_app_list(&addr_for_call).await
        });
        match result {
            Ok(apps) => {
                queried_addrs.insert(addr.clone());
                for app in apps {
                    out.push(InstalledApp {
                        addr: addr.clone(),
                        device_name: device_name.clone(),
                        package_name: app.package_name,
                        app_name: app.app_name,
                        version_code: app.version_code,
                    });
                }
            }
            Err(()) => {
                tracing::warn!("获取设备应用列表失败: {}", addr);
            }
        }
    }

    out.sort_by(|a, b| {
        a.device_name
            .cmp(&b.device_name)
            .then(a.app_name.cmp(&b.app_name))
            .then(a.package_name.cmp(&b.package_name))
    });

    tracing::info!("设备应用列表: {} 个", out.len());
    state::set_installed_apps(out);
    prune_uninstalled(&queried_addrs);
}

/// 清理已连接设备上已卸载的监听应用
fn prune_uninstalled(queried_addrs: &HashSet<String>) {
    if queried_addrs.is_empty() {
        return;
    }
    let installed = state::installed_apps();
    let installed_pairs: HashSet<(String, String)> = installed
        .iter()
        .map(|app| (app.addr.clone(), app.package_name.clone()))
        .collect();

    let to_prune: Vec<String> = state::snapshot_apps()
        .into_iter()
        .filter(|entry| match &entry.last_addr {
            Some(addr) if queried_addrs.contains(addr) => {
                !installed_pairs.contains(&(addr.clone(), entry.pkg_name.clone()))
            }
            _ => false,
        })
        .map(|entry| entry.pkg_name)
        .collect();

    for pkg in to_prune {
        tracing::info!("自动移除已卸载的监听应用: {}", pkg);
        state::remove_app(&pkg);
    }
}

/// 在所有已连接设备上注册指定包名的互联接收器，返回成功数
fn register_for_all_devices(pkg_name: &str) -> usize {
    let devices = state::connected_devices();
    if devices.is_empty() {
        if register_pair("", pkg_name) {
            return 1;
        }
        return 0;
    }
    let mut ok = 0;
    for (addr, _) in devices {
        if register_pair(&addr, pkg_name) {
            ok += 1;
        }
    }
    ok
}

fn register_pair(addr: &str, pkg_name: &str) -> bool {
    let addr_owned = addr.to_string();
    let pkg_owned = pkg_name.to_string();
    let result = wit_bindgen::block_on(async move {
        register::register_interconnect_recv(&addr_owned, &pkg_owned).await
    });
    match result {
        Ok(()) => {
            tracing::info!("注册接收器成功: addr={} pkg={}", addr, pkg_name);
            true
        }
        Err(()) => {
            tracing::error!("注册接收器失败: addr={} pkg={}", addr, pkg_name);
            false
        }
    }
}

/// 连接（监听）一个应用：注册接收器、启动快应用、发送 SF_HANDSHAKE
fn connect_app(pkg_name: &str) {
    state::ensure_app(pkg_name);
    let count = register_for_all_devices(pkg_name);

    // 查找该应用所在设备地址并启动
    let installed = state::installed_apps();
    let app_info = installed.iter().find(|a| a.package_name == pkg_name).cloned();

    if let Some(app) = app_info {
        // 启动快应用
        let addr = app.addr.clone();
        let pkg = pkg_name.to_string();
        wit_bindgen::block_on(async move {
            let _ = thirdpartyapp::launch_qa(
                &addr,
                &thirdpartyapp::AppInfo {
                    package_name: pkg.clone(),
                    fingerprint: Vec::new(),
                    version_code: 0,
                    can_remove: true,
                    app_name: app.app_name.clone(),
                },
                "/index",
            )
            .await;
            // 等待应用启动
            std::thread::sleep(Duration::from_secs(2));

            // 发送握手（插件扮演"手机端"角色，主动发起）
            send_json(
                &addr,
                &pkg,
                json!({
                    "type": "SF_HANDSHAKE",
                    "data": {}
                }),
            );
            tracing::info!("已发送SF_HANDSHAKE: addr={} pkg={}", addr, pkg);
        });
    }

    state::set_notice(format!(
        "已开始监听 {}（在 {} 台设备上注册接收器）",
        pkg_name, count
    ));
    crate::ui::build::render_without_auto_refresh();
}

/// 断开（移除）一个应用：取消SSE、移除监听
fn disconnect_app(pkg_name: &str) {
    // 取消该应用相关的所有 SSE 流（通过记录的活跃id无法直接按pkg过滤，
    // 这里取消全部活跃流作为保守处理）
    let cancels = sse_cancels().lock().unwrap_or_else(|p| p.into_inner());
    for flag in cancels.values() {
        flag.store(true, Ordering::SeqCst);
    }
    drop(cancels);

    state::remove_app(pkg_name);
    state::set_notice(format!("已断开 {}", pkg_name));
    crate::ui::build::render_without_auto_refresh();
}

// ========== UI事件处理 ==========

pub fn handle_timer_payload(payload: &str) {
    tracing::info!("timer payload: {}", payload);
}

pub fn ui_event_processor(
    event_type: crate::exports::astrobox::psys_plugin::event_v3::Event,
    event_id: &str,
    event_payload: &str,
) {
    tracing::info!("UI事件: type={:?} id={}", event_type, event_id);

    // 前缀匹配的事件
    if let Some(pkg) = event_id.strip_prefix(PKG_PICK_PREFIX) {
        connect_app(pkg);
        return;
    }
    if let Some(pkg) = event_id.strip_prefix(PKG_REMOVE_PREFIX) {
        disconnect_app(pkg);
        return;
    }
    if let Some(pkg) = event_id.strip_prefix(PKG_REREGISTER_PREFIX) {
        let count = register_for_all_devices(pkg);
        state::set_notice(format!("已为 {} 在 {} 台设备上重新注册接收器", pkg, count));
        crate::ui::build::render_without_auto_refresh();
        return;
    }
    if let Some(pkg) = event_id.strip_prefix(PKG_TOGGLE_PREFIX) {
        let enabled = parse_checked(event_payload);
        state::set_enabled(pkg, enabled);
        state::set_notice(format!(
            "已{}快应用 {} 的联网能力",
            if enabled { "启用" } else { "禁用" },
            pkg
        ));
        crate::ui::build::render_without_auto_refresh();
        return;
    }

    match event_id {
        TAB_CONNECT_EVENT => switch_tab(MainTab::Connect),
        TAB_ABOUT_EVENT => switch_tab(MainTab::About),
        EVENT_REFRESH_DEVICES => refresh_device_list(),
        EVENT_CLEAR_NOTICE => {
            state::clear_notice();
            crate::ui::build::render_without_auto_refresh();
        }
        EVENT_ADD_PKG_INPUT => {
            if let Some(value) = parse_value(event_payload) {
                state::set_pending_add(value);
            }
        }
        EVENT_ADD_PKG_SUBMIT => {
            let pkg = state::take_pending_add().trim().to_string();
            if pkg.is_empty() {
                state::set_notice("请输入要监听的快应用包名".to_string());
            } else {
                connect_app(&pkg);
            }
            crate::ui::build::render_without_auto_refresh();
        }
        OPEN_HELP_DOC_EVENT => {
            dialog::open_url("https://docs.b4qaq.cn/docs/simplefetch");
        }
        OPEN_QQ_GROUP_EVENT => {
            dialog::open_url("http://qm.qq.com/cgi-bin/qm/qr?_wv=1027&k=1vc4XKmAyGeJJTmXumfkaaxRcQl1hMaK");
        }
        _ => {}
    }
}

fn switch_tab(tab: MainTab) {
    state::with_state(|s| s.current_tab = tab);
    crate::ui::build::render_without_auto_refresh();
}

/// 解析 Switch 的 checked 字段
fn parse_checked(payload: &str) -> bool {
    if let Ok(json) = serde_json::from_str::<Value>(payload) {
        if let Some(checked) = json.get("checked").and_then(|v| v.as_bool()) {
            return checked;
        }
    }
    true
}

/// 解析输入框/控件的 value 字段
fn parse_value(payload: &str) -> Option<String> {
    if let Ok(json) = serde_json::from_str::<Value>(payload) {
        if let Some(text) = json.get("value").and_then(|v| v.as_str()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        // 兼容 detail/target 嵌套
        for key in ["detail", "target", "data"] {
            if let Some(nested) = json.get(key) {
                if let Some(text) = nested.get("value").and_then(|v| v.as_str()) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
    }
    let trimmed = payload.trim();
    if !trimmed.is_empty() && !trimmed.starts_with('{') {
        Some(trimmed.to_string())
    } else {
        None
    }
}
