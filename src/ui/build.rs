use crate::astrobox::psys_host;
use crate::astrobox::psys_host::ui_v3 as ui;
use crate::ui::icons;
use crate::ui::state::{self, AppEntry, InstalledApp, MainTab};

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

// ========== 颜色常量 ==========

const COLOR_TEXT_PRIMARY: &str = "#f4f4f5";
const COLOR_TEXT_SECONDARY: &str = "#a1a1aa";
const COLOR_TEXT_MUTED: &str = "#71717a";
const COLOR_TEXT_ACCENT: &str = "#60a5fa";
const COLOR_TEXT_DANGER: &str = "#f87171";
const COLOR_DIVIDER: &str = "#27272a";
const COLOR_BTN_PRIMARY_BG: &str = "#2563eb";
const COLOR_BTN_GHOST_BG: &str = "#27272a";
const COLOR_BTN_DANGER_BG: &str = "#3f1d1d";
const COLOR_STATUS_GREEN: &str = "#4ade80";
const COLOR_STATUS_GRAY: &str = "#71717a";
const COLOR_STATUS_RED: &str = "#f87171";

// ========== 渲染入口 ==========

pub fn render_main_ui(element_id: &str) {
    state::set_root(element_id);
    rerender();
}

pub fn rerender() {
    // 每次UI渲染时触发节流的自动刷新
    super::event_handler::auto_refresh();
    render_without_auto_refresh();
}

pub fn render_without_auto_refresh() {
    let Some(root) = state::root() else {
        return;
    };
    psys_host::ui_v3::render(&root, build_main_ui());
}

// ========== 主UI构建 ==========

fn build_main_ui() -> ui::Element {
    let current_tab = state::with_state(|s| s.current_tab);
    let tabs = build_tabs(current_tab);
    let content = match current_tab {
        MainTab::Connect => build_connect_tab(),
        MainTab::About => build_about_tab(),
    };

    let container = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .padding(20)
        .gap(16);

    container.child(tabs).child(content)
}

// ========== Tab栏 ==========

fn build_tabs(current_tab: MainTab) -> ui::Element {
    let tabs_root = ui::Element::new(ui::ElementType::TabsRoot, None)
        .flex()
        .justify_center()
        .margin_bottom(8);

    let tabs_list = ui::Element::new(ui::ElementType::TabsList, None)
        .flex()
        .bg("#1E1E1F")
        .radius(999)
        .padding(4)
        .gap(4);

    let connect_trigger = build_tab_trigger(
        "连接",
        icons::connect_tab_svg(),
        current_tab == MainTab::Connect,
        TAB_CONNECT_EVENT,
    );

    let about_trigger = build_tab_trigger(
        "关于",
        icons::about_tab_svg(),
        current_tab == MainTab::About,
        TAB_ABOUT_EVENT,
    );

    tabs_root
        .child(tabs_list.child(connect_trigger).child(about_trigger))
}

fn build_tab_trigger(label: &str, icon_svg: String, is_active: bool, event_id: &str) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(22)
        .height(22);

    let text = ui::Element::new(ui::ElementType::Span, Some(label)).size(14);

    ui::Element::new(ui::ElementType::TabsTrigger, None)
        .without_default_styles()
        .on(ui::Event::Click, event_id)
        .radius(999)
        .padding_top(10)
        .padding_bottom(10)
        .padding_left(14)
        .padding_right(14)
        .bg(if is_active { "#2A2A2A" } else { "#1E1E1F" })
        .text_color(if is_active { "#FFFFFF" } else { "#BBBBBB" })
        .flex()
        .align_center()
        .gap(5)
        .child(icon)
        .child(text)
}

// ========== 连接Tab ==========

fn build_connect_tab() -> ui::Element {
    let devices = state::connected_devices();
    let installed = state::installed_apps();
    let apps = state::snapshot_apps();
    let pending_input = state::with_state(|s| s.pending_add_pkg.clone());
    let notice = state::with_state(|s| s.last_notice.clone());
    let devices_loading = state::is_devices_loading();
    let apps_loading = state::is_apps_loading();

    let mut root = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .gap(16);

    // 通知栏
    if let Some(text) = notice {
        root = root.child(notice_line(&text));
    }

    // 设备卡片区域
    root = root.child(device_section(&devices, devices_loading));

    // 设备应用列表区域
    root = root.child(app_list_section(&installed, &apps, apps_loading));

    // 手动添加包名
    root = root.child(add_pkg_section(&pending_input));

    // 已监听的应用
    root = root.child(monitored_apps_section(&apps));

    root
}

/// 设备卡片区域
fn device_section(devices: &[(String, String)], loading: bool) -> ui::Element {
    let mut col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .gap(8);

    if devices.is_empty() {
        if loading {
            col = col.child(section_hint("正在搜索设备..."));
        } else {
            col = col.child(section_hint("当前没有连接的设备，请先在 AstroBox 中连接设备。"));
        }
    } else {
        for (addr, name) in devices {
            col = col.child(device_card(name, addr));
        }
    }

    col = col.child(
        ui::Element::new(ui::ElementType::Div, None)
            .margin_top(4)
            .child(primary_button("刷新").on(ui::Event::Click, EVENT_REFRESH_DEVICES)),
    );

    col
}

/// 设备卡片：[设备图标]设备名称 / [设备图标]设备蓝牙地址(灰色小字)
fn device_card(name: &str, addr: &str) -> ui::Element {
    let card = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .bg("#1E1E1F")
        .radius(12)
        .padding(12)
        .gap(12);

    // 设备图标
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icons::device_svg()))
        .width(24)
        .height(24)
        .text_color(COLOR_TEXT_SECONDARY);

    let icon_wrap = ui::Element::new(ui::ElementType::Div, None)
        .width(24)
        .height(24)
        .flex()
        .align_center()
        .justify_center()
        .child(icon);

    // 设备名称 + 蓝牙地址
    let info_col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .gap(2)
        .flex_grow(1.0);

    let display_name = if name.is_empty() { "未知设备" } else { name };
    let name_el = ui::Element::new(ui::ElementType::P, Some(display_name))
        .size(15)
        .text_color(COLOR_TEXT_PRIMARY);

    let addr_el = ui::Element::new(ui::ElementType::P, Some(addr))
        .size(12)
        .text_color(COLOR_TEXT_MUTED);

    // 蓝牙图标
    let bt_icon = ui::Element::new(ui::ElementType::Svg, Some(&icons::bluetooth_svg()))
        .width(16)
        .height(16)
        .text_color(COLOR_TEXT_ACCENT);

    info_col.child(name_el).child(
        ui::Element::new(ui::ElementType::Div, None)
            .flex()
            .flex_direction(ui::FlexDirection::Row)
            .align_center()
            .gap(4)
            .child(bt_icon)
            .child(addr_el),
    );

    card.child(icon_wrap).child(info_col)
}

/// 设备应用列表区域
fn app_list_section(installed: &[InstalledApp], monitored: &[AppEntry], loading: bool) -> ui::Element {
    let mut col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .gap(8);

    // 标题行：设备应用列表 + 刷新（左右对齐）
    let title = ui::Element::new(ui::ElementType::P, Some("设备应用列表"))
        .size(16)
        .text_color(COLOR_TEXT_PRIMARY)
        .flex_shrink(0.0);

    let spacer = ui::Element::new(ui::ElementType::Div, None)
        .flex_grow(1.0);

    let refresh_text = if loading { "刷新中..." } else { "刷新" };
    let refresh_btn = ui::Element::new(ui::ElementType::Button, Some(refresh_text))
        .without_default_styles()
        .on(ui::Event::Click, EVENT_REFRESH_DEVICES)
        .bg("#2A2A2A")
        .text_color(COLOR_TEXT_SECONDARY)
        .radius(8)
        .padding_left(12)
        .padding_right(12)
        .padding_top(6)
        .padding_bottom(6)
        .size(14)
        .flex_shrink(0.0);

    let header = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .gap(8)
        .child(title)
        .child(spacer)
        .child(refresh_btn);

    col = col.child(header);

    if installed.is_empty() {
        if loading {
            col = col.child(section_hint("正在加载应用列表..."));
        } else {
            col = col.child(section_hint("尚未读到设备应用列表，请确认设备已连接。"));
        }
        return col;
    }

    for app in installed {
        col = col.child(installed_app_row(app, monitored));
    }

    col
}

/// 单个已安装应用行：
/// [绿色/灰色/红色小圆点指示状态]应用名称        [连接/断开]
/// 应用包名(灰色小字)                              [连接/断开]
fn installed_app_row(app: &InstalledApp, monitored: &[AppEntry]) -> ui::Element {
    let is_monitored = monitored.iter().any(|e| e.pkg_name == app.package_name);

    // 状态圆点颜色
    let (dot_color, _status_text) = if is_monitored {
        // 检查是否启用
        let enabled = monitored
            .iter()
            .find(|e| e.pkg_name == app.package_name)
            .map(|e| e.enabled)
            .unwrap_or(true);
        if enabled {
            (COLOR_STATUS_GREEN, "已连接")
        } else {
            (COLOR_STATUS_RED, "已断开")
        }
    } else {
        (COLOR_STATUS_GRAY, "未连接")
    };

    // 状态圆点
    let dot = ui::Element::new(ui::ElementType::Div, None)
        .width(8)
        .height(8)
        .radius(4)
        .bg(dot_color)
        .flex_shrink(0.0);

    // 应用名称
    let display_name = if app.app_name.is_empty() {
        app.package_name.as_str()
    } else {
        app.app_name.as_str()
    };
    let name_el = ui::Element::new(ui::ElementType::P, Some(display_name))
        .size(15)
        .text_color(COLOR_TEXT_PRIMARY);

    // 应用包名（灰色小字）
    let pkg_el = ui::Element::new(ui::ElementType::P, Some(&app.package_name))
        .size(12)
        .text_color(COLOR_TEXT_MUTED);

    let info_col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .gap(2)
        .flex_grow(1.0);

    // 第一行：圆点 + 应用名称
    let row1 = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .gap(8)
        .child(dot)
        .child(name_el);

    // 第二行：包名
    let row2 = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .padding_left(16) // 对齐圆点后面的文字
        .child(pkg_el);

    info_col.child(row1).child(row2);

    // 连接/断开按钮
    let action = if is_monitored {
        // 已监听 → 显示"断开"
        danger_button("断开").on(
            ui::Event::Click,
            &format!("{}{}", PKG_REMOVE_PREFIX, app.package_name),
        )
    } else {
        // 未监听 → 显示"连接"
        primary_button("连接").on(
            ui::Event::Click,
            &format!("{}{}", PKG_PICK_PREFIX, app.package_name),
        )
    };

    let action_col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .align_end()
        .justify_center()
        .child(action);

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .bg("#1E1E1F")
        .radius(12)
        .padding(12)
        .gap(12)
        .child(info_col)
        .child(action_col)
}

/// 手动添加包名区域
fn add_pkg_section(pending: &str) -> ui::Element {
    let input = ui::Element::new(ui::ElementType::Input, None)
        .prop("placeholder", "手动输入包名")
        .prop("value", pending)
        .flex_grow(1.0)
        .padding(10)
        .text_color(COLOR_TEXT_PRIMARY)
        .bg("#2A2A2A")
        .radius(8)
        .on(ui::Event::Input, EVENT_ADD_PKG_INPUT);

    let submit = primary_button("添加").on(ui::Event::Click, EVENT_ADD_PKG_SUBMIT);

    let row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .gap(8)
        .align_center()
        .child(input)
        .child(submit);

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .gap(8)
        .child(section_title("手动添加包名"))
        .child(section_hint("若设备应用列表里没有目标快应用，可在此手动填写包名后添加。"))
        .child(row)
}

/// 已监听的应用列表
fn monitored_apps_section(apps: &[AppEntry]) -> ui::Element {
    let mut col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .gap(8)
        .child(section_title("已监听的快应用"))
        .child(section_hint("下列快应用已被授权通过本插件联网。关闭开关即禁用，点击「移除」则停止监听。"));

    if apps.is_empty() {
        col = col.child(section_hint(
            "尚未添加监听项。从上方列表中点「连接」，或手动填入包名后添加。",
        ));
        return col;
    }

    for (i, entry) in apps.iter().enumerate() {
        if i > 0 {
            col = col.child(thin_divider());
        }
        col = col.child(monitored_app_row(entry));
    }

    col
}

/// 已监听应用的详细行
fn monitored_app_row(entry: &AppEntry) -> ui::Element {
    // 状态圆点
    let (dot_color, status_label) = if entry.enabled {
        (COLOR_STATUS_GREEN, "已允许联网")
    } else {
        (COLOR_STATUS_RED, "已禁用")
    };

    let dot = ui::Element::new(ui::ElementType::Div, None)
        .width(8)
        .height(8)
        .radius(4)
        .bg(dot_color)
        .flex_shrink(0.0);

    let pkg_label = ui::Element::new(ui::ElementType::P, Some(&entry.pkg_name))
        .size(15)
        .text_color(COLOR_TEXT_PRIMARY);

    let status_label_el = ui::Element::new(ui::ElementType::P, Some(status_label))
        .size(12)
        .text_color(dot_color);

    // 第一行：圆点 + 包名 + 状态
    let row1 = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .gap(8)
        .child(dot)
        .child(pkg_label)
        .child(status_label_el);

    // 统计信息
    let stats = format!(
        "请求 {} · 成功 {} · 失败 {}",
        entry.request_count, entry.success_count, entry.error_count
    );
    let stats_el = ui::Element::new(ui::ElementType::P, Some(&stats))
        .size(12)
        .text_color(COLOR_TEXT_SECONDARY);

    // 最近状态
    let last_status = entry
        .last_status
        .clone()
        .unwrap_or_else(|| "尚未发起请求".to_string());
    let last_status_el = ui::Element::new(
        ui::ElementType::P,
        Some(&format!("最近状态: {}", last_status)),
    )
    .size(11)
    .text_color(COLOR_TEXT_MUTED);

    // 最近URL
    let last_url = entry
        .last_url
        .clone()
        .unwrap_or_else(|| "—".to_string());
    let last_url_el = ui::Element::new(
        ui::ElementType::P,
        Some(&format!("最近 URL: {}", last_url)),
    )
    .size(11)
    .text_color(COLOR_TEXT_MUTED);

    // 最近活动
    let last_seen_el = ui::Element::new(
        ui::ElementType::P,
        Some(&format!(
            "最近活动: {}  ·  设备: {}",
            format_time(entry.last_seen_unix_ms),
            entry.last_addr.clone().unwrap_or_else(|| "—".to_string())
        )),
    )
    .size(11)
    .text_color(COLOR_TEXT_MUTED);

    let info_col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .gap(4)
        .child(row1)
        .child(stats_el)
        .child(last_status_el)
        .child(last_url_el)
        .child(last_seen_el)
        .flex_grow(1.0);

    // 右侧操作区
    let switch = ui::Element::new(ui::ElementType::Switch, None)
        .prop("checked", if entry.enabled { "true" } else { "false" })
        .on(
            ui::Event::Change,
            &format!("{}{}", PKG_TOGGLE_PREFIX, entry.pkg_name),
        );

    let reregister_btn = ghost_button("重新注册").on(
        ui::Event::Click,
        &format!("{}{}", PKG_REREGISTER_PREFIX, entry.pkg_name),
    );

    let remove_btn = danger_button("移除").on(
        ui::Event::Click,
        &format!("{}{}", PKG_REMOVE_PREFIX, entry.pkg_name),
    );

    let actions = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .gap(8)
        .align_end()
        .child(switch)
        .child(reregister_btn)
        .child(remove_btn);

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_start()
        .gap(16)
        .padding_top(8)
        .padding_bottom(8)
        .child(info_col)
        .child(actions)
}

// ========== 关于Tab ==========

fn build_about_tab() -> ui::Element {
    let root = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .gap(8);

    // 更多内容
    let more_title = build_section_title("更多内容");

    let help_card = build_settings_card(
        icons::help_svg(),
        "帮助文档",
        Some("操作步骤与常见问题解答"),
        Some(build_more_link_icon()),
        Some(OPEN_HELP_DOC_EVENT),
    );

    let qq_card = build_settings_card(
        icons::qq_group_svg(),
        "QQ交流群",
        Some("1076096725"),
        Some(build_more_link_icon()),
        Some(OPEN_QQ_GROUP_EVENT),
    );

    // 构建信息
    let build_title = build_section_title("构建信息");

    let build_time_raw = option_env!("AB_BUILD_TIME").unwrap_or("unknown");
    let build_user = option_env!("AB_BUILD_USER").unwrap_or("unknown");
    let build_branch = option_env!("AB_BUILD_GIT_BRANCH").unwrap_or("unknown");
    let build_hash = short_git_hash(option_env!("AB_BUILD_GIT_HASH").unwrap_or("unknown"));
    let build_time = format_beijing_time(build_time_raw);

    let build_time_row = build_settings_card(
        icons::time_svg(),
        "构建时间",
        None,
        Some(build_value_text(&build_time)),
        None,
    );
    let build_user_row = build_settings_card(
        icons::user_svg(),
        "构建用户",
        None,
        Some(build_value_text(build_user)),
        None,
    );
    let build_branch_row = build_settings_card(
        icons::branch_svg(),
        "当前分支",
        None,
        Some(build_value_text(build_branch)),
        None,
    );
    let build_hash_row = build_settings_card(
        icons::hash_svg(),
        "当前hash",
        None,
        Some(build_value_text(&build_hash)),
        None,
    );

    root.child(more_title)
        .child(help_card)
        .child(qq_card)
        .child(build_title)
        .child(build_time_row)
        .child(build_user_row)
        .child(build_branch_row)
        .child(build_hash_row)
}

// ========== 辅助函数 ==========

fn section_title(text: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::P, Some(text))
        .size(16)
        .text_color(COLOR_TEXT_PRIMARY)
}

fn section_hint(text: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::P, Some(text))
        .size(12)
        .text_color(COLOR_TEXT_MUTED)
}

fn notice_line(text: &str) -> ui::Element {
    let label = ui::Element::new(ui::ElementType::P, Some(text))
        .size(13)
        .text_color(COLOR_TEXT_ACCENT)
        .flex_grow(1.0);

    let close_btn = text_button("收起", COLOR_TEXT_MUTED).on(ui::Event::Click, EVENT_CLEAR_NOTICE);

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .gap(12)
        .child(label)
        .child(close_btn)
}

fn thin_divider() -> ui::Element {
    ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .height(1)
        .bg(COLOR_DIVIDER)
        .opacity(0.6)
}

fn primary_button(text: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::Button, Some(text))
        .padding_top(8)
        .padding_bottom(8)
        .padding_left(16)
        .padding_right(16)
        .radius(8)
        .bg(COLOR_BTN_PRIMARY_BG)
        .text_color(COLOR_TEXT_PRIMARY)
        .size(13)
}

fn ghost_button(text: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::Button, Some(text))
        .padding_top(6)
        .padding_bottom(6)
        .padding_left(12)
        .padding_right(12)
        .radius(6)
        .bg(COLOR_BTN_GHOST_BG)
        .text_color(COLOR_TEXT_SECONDARY)
        .size(12)
}

fn danger_button(text: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::Button, Some(text))
        .padding_top(8)
        .padding_bottom(8)
        .padding_left(16)
        .padding_right(16)
        .radius(8)
        .bg(COLOR_BTN_DANGER_BG)
        .text_color(COLOR_TEXT_DANGER)
        .size(13)
}

fn text_button(text: &str, color: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::Button, Some(text))
        .without_default_styles()
        .text_color(color)
        .size(12)
}

fn build_settings_card(
    icon_svg: String,
    title: &str,
    desc: Option<&str>,
    right: Option<ui::Element>,
    click_event: Option<&str>,
) -> ui::Element {
    build_settings_card_colored(icon_svg, title, desc, right, click_event, "#1E1E1F", "#FFFFFF")
}

fn build_settings_card_colored(
    icon_svg: String,
    title: &str,
    desc: Option<&str>,
    right: Option<ui::Element>,
    click_event: Option<&str>,
    bg_color: &str,
    text_color: &str,
) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(22)
        .height(22)
        .text_color(text_color);

    let icon_wrap = ui::Element::new(ui::ElementType::Div, None)
        .width(22)
        .height(22)
        .flex()
        .align_center()
        .justify_center()
        .child(icon);

    let mut text_col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full();

    let title_el = ui::Element::new(ui::ElementType::P, Some(title))
        .size(15)
        .text_color(text_color);
    text_col = text_col.child(title_el);

    if let Some(desc_text) = desc {
        let desc_el = ui::Element::new(ui::ElementType::P, Some(desc_text))
            .size(13)
            .text_color(text_color);
        text_col = text_col.child(desc_el);
    }

    let mut row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .bg(bg_color)
        .radius(12)
        .padding_left(12)
        .padding_right(12)
        .padding_top(10)
        .padding_bottom(10)
        .gap(10)
        .child(icon_wrap)
        .child(text_col);

    if let Some(right_el) = right {
        let right_wrap = ui::Element::new(ui::ElementType::Div, None)
            .flex()
            .align_center()
            .justify_end()
            .child(right_el);
        row = row.child(right_wrap);
    }

    if let Some(event_id) = click_event {
        row = row.on(ui::Event::Click, event_id);
    }

    row
}

fn build_section_title(text: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::P, Some(text))
        .size(13)
        .text_color("#888888")
        .margin_left(12)
        .margin_top(10)
}

fn build_more_link_icon() -> ui::Element {
    let svg = icons::more_link_svg();
    ui::Element::new(ui::ElementType::Svg, Some(&svg))
        .width(18)
        .height(18)
        .text_color("#0088FF")
}

fn build_value_text(value: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::P, Some(value))
        .size(13)
        .text_color("#BBBBBB")
}

fn format_time(unix_ms: Option<u128>) -> String {
    match unix_ms {
        None => "—".to_string(),
        Some(ms) => {
            let secs = ms / 1000;
            let h = (secs / 3600) % 24;
            let m = (secs / 60) % 60;
            let s = secs % 60;
            format!("{:02}:{:02}:{:02} UTC", h, m, s)
        }
    }
}

fn format_beijing_time(raw: &str) -> String {
    if let Some((y, m, d, hh, mm, ss)) = parse_iso_utc(raw) {
        let (y2, m2, d2, hh2) = add_hours(y, m, d, hh, 8);
        return format!(
            "{:04}‑{:02}‑{:02}_{:02}:{:02}:{:02}",
            y2, m2, d2, hh2, mm, ss
        );
    }
    raw.to_string()
}

fn parse_iso_utc(raw: &str) -> Option<(i32, i32, i32, i32, i32, i32)> {
    if raw.len() < 19 {
        return None;
    }
    let base = &raw[..19];
    let mut parts = base.split('T');
    let date = parts.next()?;
    let time = parts.next()?;
    let mut dparts = date.split('-');
    let y: i32 = dparts.next()?.parse().ok()?;
    let m: i32 = dparts.next()?.parse().ok()?;
    let d: i32 = dparts.next()?.parse().ok()?;
    let mut tparts = time.split(':');
    let hh: i32 = tparts.next()?.parse().ok()?;
    let mm: i32 = tparts.next()?.parse().ok()?;
    let ss: i32 = tparts.next()?.parse().ok()?;
    Some((y, m, d, hh, mm, ss))
}

fn add_hours(mut y: i32, mut m: i32, mut d: i32, mut hh: i32, add: i32) -> (i32, i32, i32, i32) {
    hh += add;
    while hh >= 24 {
        hh -= 24;
        d += 1;
        let dim = days_in_month(y, m);
        if d > dim {
            d = 1;
            m += 1;
            if m > 12 {
                m = 1;
                y += 1;
            }
        }
    }
    (y, m, d, hh)
}

fn days_in_month(y: i32, m: i32) -> i32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(y) { 29 } else { 28 }
        }
        _ => 30,
    }
}

fn is_leap_year(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

fn short_git_hash(hash: &str) -> String {
    let trimmed = hash.trim();
    if trimmed.is_empty() || trimmed == "unknown" {
        return "unknown".to_string();
    }
    trimmed.chars().take(7).collect()
}
