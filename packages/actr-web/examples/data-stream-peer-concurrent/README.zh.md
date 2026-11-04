# Data Stream Peer Concurrent - Legacy / Unsafe Web Example

> [!CAUTION]
> 这是一个保留用于历史 Fast Path 调试的 legacy 示例，不是当前受支持的安全 Web workload 流程。它的自定义 Service Worker 会直接获取并执行松散的 wasm-bindgen JavaScript 和 `_bg.wasm`，不会验证 `.actr` 签名或内容 hash。`start.sh` 默认拒绝启动。当前流程请使用 [`../echo/start-mock.sh`](../echo/start-mock.sh)，直到本示例完成 signed `.actr` 迁移。

这个示例把 [examples/actr/rust/data-stream-peer-concurrent](../../../../examples/actr/rust/data-stream-peer-concurrent/README.md) 的核心流程搬到了浏览器端：

- 浏览器发起 `StartStream`
- 发起方在本地 WASM handler 中发现远端对等节点
- 远端节点调回 `PrepareClientStream`
- 双方分别调用 `ctx.register_stream()` 注册 `stream_id`
- Client / Server 通过 `ctx.send_data_chunk()` 在 WebRTC DataChannel 上双向发送数据
- `test-auto.js` 用 Puppeteer 同时拉起多个 Client 页面，验证并发场景

## 目录结构

```text
data-stream-peer-concurrent/
├── client/
│   ├── public/actor.sw.js
│   ├── src/
│   └── wasm/
├── server/
│   ├── public/actor.sw.js
│   ├── src/
│   └── wasm/
├── package.json
├── start.sh
└── test-auto.js
```

## 前置要求

1. 安装：Node.js 18+、pnpm、Rust 1.95+、wasm-pack
2. `start.sh` 会自动构建并启动本仓库的 `mock-loran`，不需要外部 `loran` checkout、STUN/TURN 服务或手工写 SQLite

## Legacy 调试方式（不安全）

不要在生产环境、共享环境或处理不可信输入时运行。只有在隔离的本地环境中确认接受未验证代码执行风险后，才能显式启用：

```bash
cd packages/actr-web/examples/data-stream-peer-concurrent
ACTR_ALLOW_UNVERIFIED_LEGACY_WBG=1 ./start.sh
```

默认使用 `8081`。如果本机已有旧的 `8081` 服务，请先关闭它再运行这个示例。

`start.sh` 会：

1. 安装测试/示例依赖
2. 编译 sender/receiver WASM
3. 启动 `mock-loran` 并通过 HTTP admin API 创建测试 realm
4. 启动一个 Server Vite dev server 和两个 Client Vite dev server
5. 运行 `test-auto.js` 做并发验证

这条 legacy 路径不作为当前 Web runtime 的验收依据。它默认不启动 STUN/TURN；显式启用后的自动测试会通过 Puppeteer 使用独立浏览器上下文，并关闭 Chrome 的 WebRTC mDNS 本地地址隐藏，避免本机 DataChannel 建连受 `*.local` candidate 影响。

## 单独跑 legacy 自动化测试

下面的分步命令同样会执行未经 package 验证的松散 JS/WASM，只能用于隔离的历史调试：

```bash
cd packages/actr-web/examples/data-stream-peer-concurrent
pnpm install
pnpm install --dir client
pnpm install --dir server
./client/build.sh
./server/build.sh

# 终端 1: mock-loran
../../../../target/debug/mock-loran --port 8081

# 终端 2: seed realm
curl -fsS -X POST http://127.0.0.1:8081/admin/realms \
  -H 'content-type: application/json' \
  --data '{"realm":"data-stream@example.com"}'

# 终端 3
cd client && VITE_LORAN_HTTP_URL=http://127.0.0.1:8081 pnpm dev --host 127.0.0.1 --port 4175

# 终端 4
cd server && VITE_LORAN_HTTP_URL=http://127.0.0.1:8081 pnpm dev --host 127.0.0.1 --port 4176

# 终端 5
cd client && VITE_LORAN_HTTP_URL=http://127.0.0.1:8081 pnpm dev --host 127.0.0.1 --port 4177

# 终端 6
CLIENT_URLS=http://127.0.0.1:4175,http://127.0.0.1:4177 \
SERVER_URL=http://127.0.0.1:4176 \
node test-auto.js
```

## 手动验证注意事项

手动打开页面也可以验证，但需要保持环境干净：

1. 重启 `mock-loran`，避免之前手动开关页面留下 unbound actor。
2. 统一使用 `127.0.0.1`，不要混用 `localhost` 和 `127.0.0.1`，否则浏览器会把它们当成不同 origin。
3. 使用独立 Chrome profile，并关闭 WebRTC mDNS 本地地址隐藏：

```bash
/Applications/Google\ Chrome.app/Contents/MacOS/Google\ Chrome \
  --user-data-dir=/tmp/actr-data-stream-peer-concurrent \
  --disable-features=WebRtcHideLocalIpsWithMdns \
  http://127.0.0.1:4176/ \
  'http://127.0.0.1:4175/?autoStart=1&clientId=client-1&messageCount=3' \
  'http://127.0.0.1:4177/?autoStart=1&clientId=client-2&messageCount=3'
```

如果用普通 Chrome profile 反复刷新或关闭页面，可能残留 Service Worker、IndexedDB 或旧 actor 状态；如果出现 `RPC timeout after 30000ms`，先重启 `mock-loran` 并换一个新的 `--user-data-dir` 再试。

## 验证点

- Client 页面能看到：
  - `start_stream response`
  - `client sending N/N`
  - `client received N/N`
- Server 页面能看到：
  - `prepare_stream`
  - `server: stream <stream_id> received N/N`
  - `server sending N/N`
- Server / Client 日志不应出现：
  - `Unknown service: __fast_path_data_chunk__`
  - `Tell handler error`

## 说明

- 这里的 RPC 请求体使用 JSON 编码，重点验证的是 web 侧 `register_stream()` / `send_data_chunk()` / Fast Path 分发链路，而不是 protobuf codegen。
- 默认 ICE server 列表为空，适合本机多端口验证；如需显式 STUN/TURN，可设置 `VITE_LORAN_STUN_URL`。
- 并发测试默认启动 2 个 Client 页面，每个页面发送 3 条消息；可通过环境变量覆盖：
  - `CLIENT_URLS`
  - `MESSAGE_COUNT`
  - `SERVER_URL`
