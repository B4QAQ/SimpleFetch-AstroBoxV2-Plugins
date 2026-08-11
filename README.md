<div align="center">

# SimpleFetch · AstroBox 插件

**为快应用提供网络桥接，代理 HTTP / SSE 请求**

[![version](https://img.shields.io/badge/version-1.2.2-blue)](#)
[![api](https://img.shields.io/badge/API%20Level-3-orange)](#)
[![license](https://img.shields.io/badge/license-MIT-green)](LICENSE)

</div>

---

## ✨ 简介

SimpleFetch 是运行在 AstroBox 上的网络桥接插件。它在手表端充当「手机端」角色，让接入 SimpleFetch 协议的快应用，通过互联通道（interconnect）把网络请求交给插件代为执行，再把结果原路返回。

- 🌐 **任意快应用可用** —— 按包名授权，不绑定特定应用
- 🔌 **双向握手** —— 插件或快应用任一方均可发起连接
- 💓 **心跳保活** —— 自动回复 PING，长时间无心跳自动断开
- 📦 **自动分片** —— 大响应体 base64 分片回传，小数据单条直发
- 🔄 **SSE 流式转发** —— 逐事件推送，支持快应用主动关闭
- 🛡️ **错误中文化** —— `请求超时` / `网络不可达` / `DNS解析失败` 等可读描述

---

## 📸 实机演示

<p align="center">
  <img src="img/1.png" width="240" alt="连接页" />
  <img src="img/2.png" width="240" alt="设备应用列表" />
  <img src="img/3.png" width="240" alt="关于页" />
</p>

| 连接页 | 设备应用列表 | 关于页 |
|:---:|:---:|:---:|
| 当前设备卡片 + 状态圆点 | 一键连接 / 断开，查看请求统计 | 构建信息、帮助入口 |

---

## 🚀 快速开始

### 1. 初始化子模块

```bash
git submodule update --init --remote --recursive
```

### 2. 安装构建目标

```bash
rustup target add wasm32-wasip2
```

### 3. 构建

```bash
# 仅编译
cargo build --release

# 生成 dist 产物（manifest / 图标 / wasm）
python scripts/build_dist.py --release

# 同时打包成 .abp
python scripts/build_dist.py --release --package
```

产物输出到 `dist/`，发布同步到 `release/`。

---

## 📡 通信协议

所有消息为 JSON 对象，顶层结构固定为 `{ type, status, data }`。

| 方向 | type | 说明 |
|------|------|------|
| 双向 | `SF_HANDSHAKE` | 发起握手 |
| 双向 | `SF_HANDSHAKE_ACK` | 握手确认 |
| 快应用 → 插件 | `SF_PING` | 心跳 |
| 插件 → 快应用 | `SF_PONG` | 心跳回复 |
| 插件 → 快应用 | `SF_CLOSE_BRIDGE` | 关闭桥接 |
| 快应用 → 插件 | `SF_CLOSE_BRIDGE_ACK` | 关闭确认 |
| 快应用 → 插件 | `SF_REQUEST` | 网络请求（普通或 SSE） |
| 插件 → 快应用 | `SF_RESPONSE` | 普通响应（含分片） |
| 插件 → 快应用 | `SF_SSE_EVENT` | SSE 事件 |
| 插件 → 快应用 | `SF_SSE_END` | SSE 正常结束 |
| 插件 → 快应用 | `SF_SSE_ERROR` | SSE 出错 |
| 快应用 → 插件 | `SF_CLOSE` | 关闭某个 SSE |

完整协议见 [`SimpleFetch-手机端接入文档.md`](SimpleFetch-手机端接入文档.md)。

### 握手示例

```jsonc
// 发起方
{ "type": "SF_HANDSHAKE", "data": {} }
// 接收方回复
{ "type": "SF_HANDSHAKE_ACK", "status": "OK", "data": {} }
```

### 普通请求

```jsonc
// 快应用 → 插件
{
  "type": "SF_REQUEST",
  "data": {
    "id": "sf_1",
    "url": "https://api.example.com/data",
    "method": "POST",
    "headers": { "Content-Type": "application/json" },
    "body": "{\"key\":\"value\"}",
    "sse": false,
    "timeout": 10000
  }
}

// 插件 → 快应用（小数据，无分片）
{
  "type": "SF_RESPONSE",
  "status": "OK",
  "data": {
    "id": "sf_1",
    "statusCode": 200,
    "headers": { "Content-Type": "application/json" },
    "body": "{\"result\":\"ok\"}",
    "chunk": 0,
    "totalChunks": 0
  }
}
```

---

## 🧩 使用方式

1. 在 AstroBox 中连接手表 / 手环；
2. 打开 SimpleFetch 插件，**连接页**会显示当前设备和设备上的快应用列表；
3. 点击应用右侧的 `[连接]`，插件会启动快应用并发起握手；
4. 握手成功后圆点变绿，该快应用的所有 `fetch` 请求将自动走 SimpleFetch 代理；
5. 也可以由快应用端在「偏好设置 → 连接桥接网络」主动发起握手。

> 带请求体但缺少 `Content-Type` 时，插件会按内容自动补全（`{` / `[` 开头视为 JSON），与快应用原生 `@system.fetch` 行为对齐。

---

## 🛠️ 技术栈

- **Rust** + `wit-bindgen` —— Component Model 异步绑定
- **WASI HTTP** —— 真实 HTTP / SSE 请求执行
- **flate2** —— gzip / deflate 自动解压
- **base64** —— 大响应体编码分片

---

## 📄 许可证

[MIT](LICENSE)

---

<div align="center">
<sub>为 Vela / 快应用生态而作 ⌚</sub>
</div>
