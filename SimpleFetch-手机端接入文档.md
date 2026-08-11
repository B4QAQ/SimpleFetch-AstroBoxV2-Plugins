# SimpleFetch 手机端接入文档

## 概述

SimpleFetch 是快应用侧通过互联通道（interconnect）代理网络请求的模块。手机端需要实现对应的消息收发逻辑，充当网络代理的角色。

**通信流程**：快应用发送请求 → 手机端接收并执行真实 HTTP 请求 → 手机端将结果通过互联通道返回快应用

---

## 消息格式

所有消息为 JSON 对象，结构：`{ type, status, data }`

- `type`：消息类型，SimpleFetch 相关均以 `SF_` 开头
- `status`：`'OK'` 表示成功，错误时直接填错误原因字符串（如 `'请求超时'`）
- `data`：消息体，具体内容因类型而异

---

## 消息类型一览

| 方向 | type | 说明 |
|------|------|------|
| 手机→快应用 | `SF_HANDSHAKE` | 发起握手 |
| 快应用→手机 | `SF_HANDSHAKE_ACK` | 握手确认 |
| 快应用→手机 | `SF_PING` | 心跳 |
| 手机→快应用 | `SF_PONG` | 心跳回复 |
| 手机→快应用 | `SF_CLOSE_BRIDGE` | 手动关闭桥接 |
| 快应用→手机 | `SF_CLOSE_BRIDGE_ACK` | 关闭桥接确认 |
| 快应用→手机 | `SF_REQUEST` | 网络请求 |
| 手机→快应用 | `SF_RESPONSE` | 请求响应（含分片） |
| 手机→快应用 | `SF_SSE_EVENT` | SSE 事件 |
| 手机→快应用 | `SF_SSE_END` | SSE 流结束 |
| 手机→快应用 | `SF_SSE_ERROR` | SSE 错误 |
| 快应用→手机 | `SF_CLOSE` | 关闭 SSE/取消请求 |

---

## 1. 握手

手机端主动发起，建立桥接网络。

**手机发送：**
```json
{ "type": "SF_HANDSHAKE", "data": {} }
```

**快应用回复：**
```json
{ "type": "SF_HANDSHAKE_ACK", "status": "OK", "data": {} }
```

握手成功后，快应用会将 `NetworkStatus` 设为 `'bridge'`，`fetchAva` 设为 `true`，并启动心跳。

---

## 2. 心跳

握手成功后，快应用每 10 秒发送一次心跳。手机端必须回复，否则快应用 5 秒内未收到回复会判定超时并断开桥接。

**快应用发送：**
```json
{ "type": "SF_PING", "data": { "ts": 1700000000 } }
```

**手机回复：**
```json
{ "type": "SF_PONG", "status": "OK", "data": { "ts": 1700000000 } }
```

> `ts` 为时间戳，原样回传即可。

---

## 3. 手动关闭桥接

手机端可主动要求快应用关闭桥接网络。适用于手机端检测到自身网络不可用、用户手动断开代理等场景。

**手机发送：**
```json
{ "type": "SF_CLOSE_BRIDGE", "data": {} }
```

**快应用回复：**
```json
{ "type": "SF_CLOSE_BRIDGE_ACK", "status": "OK", "data": {} }
```

快应用收到后会：回复确认 → 停止心跳 → 拒绝所有待处理请求和 SSE → 将 `NetworkStatus` 设为 `'none'` → 调用 `getDeviceInfo()` 刷新网络状态。

---

## 4. 普通请求

### 4.1 快应用发送请求

```json
{
  "type": "SF_REQUEST",
  "data": {
    "id": "sf_1",
    "url": "https://api.example.com/data",
    "method": "POST",
    "headers": { "Content-Type": "application/json" },
    "body": "{\"key\":\"value\"}",
    "sse": false,
    "timeout": 15000
  }
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | string | 请求ID，响应时原样回传，用于关联请求和响应 |
| `url` | string | 完整请求地址 |
| `method` | string | HTTP 方法：GET / POST / PUT / DELETE |
| `headers` | object | 请求头 |
| `body` | string\|null | 请求体（GET 时为 null） |
| `sse` | boolean | 是否为 SSE 请求（普通请求为 false） |
| `timeout` | number | 超时时间(ms) |

### 4.2 小数据响应（≤16KB）

单条消息返回完整数据，`totalChunks` 为 0：

```json
{
  "type": "SF_RESPONSE",
  "status": "OK",
  "data": {
    "id": "sf_1",
    "statusCode": 200,
    "headers": { "Content-Type": "application/json" },
    "body": "{\"result\":\"success\"}",
    "chunk": 0,
    "totalChunks": 0
  }
}
```

### 4.3 大数据分片响应（>16KB）

当响应体超过 16KB 时，手机端需将 body 进行 **base64 编码**，然后分片发送。每条消息的 `totalChunks` > 0，`chunk` 从 1 递增到 `totalChunks`。

**第 1 片：**
```json
{
  "type": "SF_RESPONSE",
  "status": "OK",
  "data": {
    "id": "sf_1",
    "statusCode": 200,
    "headers": { "Content-Type": "application/json" },
    "body": "base64编码的分片1...",
    "chunk": 1,
    "totalChunks": 5
  }
}
```

**第 2~4 片：**（statusCode 和 headers 可省略或忽略）
```json
{
  "type": "SF_RESPONSE",
  "status": "OK",
  "data": {
    "id": "sf_1",
    "body": "base64编码的分片N...",
    "chunk": 2,
    "totalChunks": 5
  }
}
```

**最后一片（chunk === totalChunks）：**
```json
{
  "type": "SF_RESPONSE",
  "status": "OK",
  "data": {
    "id": "sf_1",
    "body": "base64编码的分片5...",
    "chunk": 5,
    "totalChunks": 5
  }
}
```

快应用收到 `chunk === totalChunks` 的消息后，会按 index 顺序拼接所有分片的 body，然后 base64 解码得到完整响应。

**分片要点：**
- `chunk` 从 **1** 开始，到 `totalChunks` 结束
- `totalChunks: 0` 表示无分片（完整数据）
- `totalChunks: N (>0)` 表示有分片
- 第 1 片必须包含 `statusCode` 和 `headers`
- body 需 base64 编码（快应用侧使用 `crypto.atob` 解码）
- 分片必须按顺序发送

### 4.4 错误响应

`status` 直接填错误原因：

```json
{
  "type": "SF_RESPONSE",
  "status": "请求超时",
  "data": {
    "id": "sf_1",
    "statusCode": 0,
    "error": "请求超时"
  }
}
```

常见错误 status：
- `请求超时` — HTTP 请求超时
- `网络不可达` — 手机无网络
- `DNS解析失败` — 域名无法解析
- `连接被拒绝` — 服务器拒绝连接

> `statusCode: 0` 表示网络层错误（非 HTTP 响应），有 HTTP 响应时填实际状态码。

---

## 5. SSE 流式请求

### 5.1 快应用发起 SSE

```json
{
  "type": "SF_REQUEST",
  "data": {
    "id": "sf_2",
    "url": "https://api.example.com/stream",
    "method": "GET",
    "headers": {},
    "body": null,
    "sse": true,
    "timeout": 30000
  }
}
```

`sse: true` 标识此请求为 SSE 流式请求。

### 5.2 手机端转发 SSE 事件

手机端建立 SSE 连接后，每收到一个服务端事件就转发一条消息：

```json
{
  "type": "SF_SSE_EVENT",
  "data": {
    "id": "sf_2",
    "event": "message",
    "data": "事件数据内容"
  }
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | string | 请求ID |
| `event` | string\|null | SSE 事件类型，默认 `"message"` |
| `data` | string | SSE 事件数据 |

### 5.3 SSE 流结束

服务端关闭 SSE 连接时发送：

```json
{ "type": "SF_SSE_END", "data": { "id": "sf_2" } }
```

### 5.4 SSE 错误

SSE 连接出错时发送，`status` 填错误原因：

```json
{
  "type": "SF_SSE_ERROR",
  "status": "连接断开",
  "data": { "id": "sf_2", "error": "连接断开" }
}
```

### 5.5 快应用主动关闭 SSE

快应用发送 `SF_CLOSE`，手机端应关闭对应的 SSE 连接：

```json
{ "type": "SF_CLOSE", "data": { "id": "sf_2" } }
```

---

## 6. 完整交互时序

### 普通请求

```
手机                          快应用
 │                              │
 │──── SF_HANDSHAKE ──────────→│  手机发起握手
 │←─── SF_HANDSHAKE_ACK ──────│  快应用确认，启动心跳
 │                              │
 │←─── SF_PING ───────────────│  每10秒心跳
 │──── SF_PONG ──────────────→│  手机回复
 │                              │
 │←─── SF_REQUEST ────────────│  快应用发起请求
 │     (id=sf_1, sse=false)    │
 │                              │  手机执行HTTP请求
 │──── SF_RESPONSE ──────────→│  返回结果
 │     (id=sf_1, totalChunks=0)│
 │                              │
 │←─── SF_PING ───────────────│  心跳持续
 │──── SF_PONG ──────────────→│
```

### 分片请求

```
手机                          快应用
 │                              │
 │←─── SF_REQUEST ────────────│  请求大数据
 │     (id=sf_3)               │
 │                              │  手机执行HTTP请求，响应>16KB
 │──── SF_RESPONSE ──────────→│  chunk:1/5
 │──── SF_RESPONSE ──────────→│  chunk:2/5
 │──── SF_RESPONSE ──────────→│  chunk:3/5
 │──── SF_RESPONSE ──────────→│  chunk:4/5
 │──── SF_RESPONSE ──────────→│  chunk:5/5 (快应用组装解码)
```

### SSE 请求

```
手机                          快应用
 │                              │
 │←─── SF_REQUEST ────────────│  SSE请求
 │     (id=sf_4, sse=true)     │
 │                              │  手机建立SSE连接
 │──── SF_SSE_EVENT ─────────→│  转发事件1
 │──── SF_SSE_EVENT ─────────→│  转发事件2
 │──── SF_SSE_EVENT ─────────→│  转发事件3
 │──── SF_SSE_END ───────────→│  流结束
 │                              │
 │  或快应用主动关闭：           │
 │←─── SF_CLOSE ──────────────│  快应用关闭SSE
 │     (手机关闭SSE连接)        │
```

### 心跳超时断连

```
手机                          快应用
 │                              │
 │←─── SF_PING ───────────────│  心跳
 │  (5秒内未回复PONG)          │
 │                              │  快应用判定超时
 │                              │  NetworkStatus='none'
 │                              │  reject所有待处理请求
 │                              │  调用getDeviceInfo()刷新
```

### 手动关闭桥接

```
手机                          快应用
 │                              │
 │──── SF_CLOSE_BRIDGE ───────→│  手机要求关闭桥接
 │←─── SF_CLOSE_BRIDGE_ACK ────│  快应用确认
 │                              │  停止心跳
 │                              │  reject所有待处理请求
 │                              │  NetworkStatus='none'
 │                              │  调用getDeviceInfo()刷新
```

---

## 7. 注意事项

1. **包名和签名**：手机端 App 的包名必须与快应用 `manifest.json` 中的 `package` 字段一致（`moe.mcns.ResonaUI`），且签名匹配
2. **心跳必须回复**：5 秒内未回复 PONG，快应用会自动断开桥接
3. **分片必须按序**：chunk 从 1 到 totalChunks 必须按顺序发送，不可乱序
4. **分片 body 需 base64 编码**：快应用侧使用 `crypto.atob` 解码
5. **错误 status 直接是原因**：不要用错误码，直接用可读的中文错误描述
6. **请求 ID 关联**：响应中的 `id` 必须与请求中的 `id` 一致，快应用靠此关联请求和响应
7. **互联连接断开**：当互联通道断开时，快应用会自动停用桥接，手机端无需额外处理
8. **手动关闭桥接**：手机端可发送 `SF_CLOSE_BRIDGE` 主动要求关闭，快应用回复确认后停用桥接
