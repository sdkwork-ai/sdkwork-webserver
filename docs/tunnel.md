# SDKWORK WebServer Tunnel（网络穿透 / 反向连接）

SDKWORK WebServer 的内置网络穿透能力（FRP 同类能力的原生实现，PRD《SDKWORK WebServer Tunnel》v1.0）。WebServer 因此成为 **Web + Reverse Proxy + Tunnel + Edge Runtime** 的统一边缘设施，为本地计算机、云服务器、Sandbox、AI Agent 与开发环境提供安全的反向连接。

## 定位

```text
Internet → SDKWORK Gateway (HTTP :80/:443) → Tunnel (QUIC) → Agent → 本地服务
```

- **Gateway**（公网边缘）：QUIC 监听、设备认证、会话/路由管理、HTTP/TCP 分发、ACL、限速、Metrics。
- **Agent**（内网侧）：主动连接网关、Token 认证、路由注册、心跳、自动重连、本地目标转发。

## Quick Start

### Gateway（CLI 独立进程）

```bash
export SDKWORK_TUNNEL_GATEWAY_TOKEN=<token>
sdkwork-webserver-tunnel gateway --listen 0.0.0.0:8443 --domain-suffix sdkwork.link
```

开发环境未配置 `SDKWORK_TUNNEL_TLS_CERT` / `SDKWORK_TUNNEL_TLS_KEY` 时自动生成自签证书并打印 SHA-256 指纹（agent 用 `--pin` 锁定）。生产环境通过环境变量提供真实证书。

### Agent

```bash
export SDKWORK_TUNNEL_TOKEN=<token> SDKWORK_TUNNEL_ENDPOINT=<gateway-host>:8443
sdkwork-webserver-tunnel agent \
  --endpoint "$SDKWORK_TUNNEL_ENDPOINT" \
  --route web:http:demo.sdkwork.link:127.0.0.1:3000 \
  --pin <gateway-fingerprint>     # 或 --ca <pem>；仅开发可用 --insecure
```

### Expose（一键暴露本地端口，PRD §85）

```bash
sdkwork-webserver-tunnel expose --endpoint <gw>:8443 --domain demo.sdkwork.link \
  --pin <fingerprint> 3000
# → tunnel ready: https://demo.sdkwork.link -> local web
```

### 网关随 WebServer 数据面运行（原生集成）

在 `server.toml` / app 配置中启用：

```toml
[tunnel]
enabled = true
[tunnel.gateway]
listen = "0.0.0.0:8443"
domain_suffixes = ["sdkwork.link"]
# agent_token_env 默认 ["SDKWORK_TUNNEL_GATEWAY_TOKEN"]；token 永不入配置文件
[[tunnel.routes]]        # agent 侧模板；gateway 配置里仅为声明
name = "web"
protocol = "http"
domain = "demo.sdkwork.link"
target = "127.0.0.1:3000"
[tunnel.routes.policy]
allow_public = true
```

数据面启动时同步拉起 QUIC 监听；`Host: demo.sdkwork.link` 的请求在虚拟主机未命中该域名时经隧道中继到 agent 的本地目标，响应原路返回（含 WebSocket 升级）。`enabled = false` 或缺省时运行时行为与无 Tunnel 完全一致（PRD §77）。

## 架构与 Crate 划分（高内聚低耦合，PRD §43）

```text
sdkwork-webserver-tunnel-core        领域模型：Device/Session/Route/Matcher/Policy/ID/Config/Error（serde-only）
        ↑
sdkwork-webserver-tunnel-protocol    STP/1 控制面消息、数据面流头、长度前缀帧编解码（尺寸上限强制）
        ↑
sdkwork-webserver-tunnel-transport   Transport 抽象 trait + QUIC（quinn + rustls TLS 1.3）实现 + TLS 材料
        ↑
sdkwork-webserver-tunnel             组合层：Gateway（监听/控制循环/注册表/会话表/TCP 监听/分发）、
                                     Agent（连接/认证/注册/心跳/重连/本地转发）、Security、Service 门面、CLI
        ↑
webserver 集成                       sdkwork-webserver-core（[tunnel] 配置节）、
                                     standalone-gateway（数据面启停 / HTTP 分发 / operations REST / metrics）
```

关键边界：

- **Domain ≠ Protocol**：领域对象不经网络传输，协议层持有独立 wire 类型（PRD §102）。
- **QUIC/TLS 只存在于 transport crate**；HTTP 语义只存在于 webserver 侧。中继入口是字节流（`connect_http_stream` / `connect_tcp_stream`），与协议无关。
- **Control Plane / Data Plane 分离**（PRD §21）：每会话一条长驻控制流（STP/1 消息），每访客连接一条数据流（首帧 `DataStreamHeader`，随后为原始字节）。
- **SSRF 边界**（PRD §45）：目标地址永远来自 agent 自身配置——数据流头只携带 `route_id`，agent 查本地路由表拨号；网关 REST 的 create-route 仅是"声明"，agent 只激活与其本地模板匹配的声明。

## 安全（PRD §24–§25, §44–§47, §106–§108）

- QUIC over **TLS 1.3**（rustls，进程级唯一 CryptoProvider）；agent 支持 CA / 指纹锁定 / 显式 skip-verify（仅开发，启动时告警）。
- Agent 认证：环境变量解析的 Bearer Token（`SDKWORK_TUNNEL_GATEWAY_TOKEN` / `SDKWORK_TUNNEL_TOKEN`），常量时间比较；未配置 token 的网关拒绝所有 agent（fail closed）。
- 认证失败按源 IP 固定窗口限速；访问者按路由策略（allowPublic / allowedIps / BearerToken）准入。
- 资源上限：maxDevices / maxSessions / maxStreamsPerSession（信号量预算）/ maxRoutes / maxControlMessageBytes；同设备重复连接替换旧会话。
- 心跳（默认 10s）× 3 + idle 上限驱动过期会话清理；会话销毁级联注销路由并释放 TCP 监听端口。

## 可观测性（PRD §48–§49, §82）

- Prometheus 指标（数据面 operations `/metrics` 追加）：`sdkwork_tunnel_connections[_active]`、`sessions[_active]`、`streams[_active]`、`bytes_in/out`、`reconnects`、`errors`、`auth_failures`、`routes_active`。
- operations REST（回环）：`GET /tunnel/status`、`GET|POST /tunnel/routes`、`DELETE /tunnel/routes/{id}`。
- CLI：`sdkwork-webserver-tunnel status|list|remove --url <ops-base>`；诊断用 `doctor`（端点 → TLS → QUIC → STP 握手 → 认证 分级检查，PRD §83）。

## 重连与优雅关闭（PRD §27, §105）

Agent 断线后按 1s→2s→4s…60s 指数退避 + ±20% 抖动自动重连，成功注册后重置；路由热注册/注销经控制面完成，agent 无需重启。网关关闭顺序：停止 accept → 关闭 endpoint/会话 → 注销路由 → 释放 TCP 监听。

## 测试

- 单元：领域模型、协议 round-trip（`decode(encode(m)) == m`）、未知消息降级、帧尺寸上限、TLS 材料与指纹、注册表冲突、会话预算、退避。
- 集成：QUIC 回环、gateway+agent+本地 HTTP 端到端、数据面 `[tunnel]` 配置中继、§77 禁用兼容、认证拒绝、证书指纹拒绝。

## 已知限制（V1，PRD §87/§133 对齐）

- 状态存于内存（PRD §116）；网关重启后 agent 自动重连并重建路由，REST create-route 的离线设备声明不持久化。
- Gateway→agent 的 remove（REST 删除）不通知 agent；agent 重连后会重新注册其模板路由。
- mTLS、ACME、带宽限制、UDP、多网关、P2P/Mesh 为 P1/P2（PRD §8），未实现。
