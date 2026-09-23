# SDKWORK WebServer Tunnel（网络穿透 / 反向连接）

SDKWORK WebServer 的内置网络穿透能力（FRP 同类能力的原生实现，PRD《SDKWORK WebServer Tunnel》v1.0）。WebServer 因此成为 **Web + Reverse Proxy + Tunnel + Edge Runtime** 的统一边缘设施，为本地计算机、云服务器、Sandbox、AI Agent 与开发环境提供安全的反向连接。

## 定位

```text
Internet → SDKWORK Gateway (HTTP :80/:443) → Tunnel (QUIC) → Agent → 本地服务
```

- **Gateway**（公网边缘）：QUIC 监听、设备认证、会话/路由管理、HTTP/TCP/UDP 分发、ACL、限速、Metrics。
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

数据面启动时同步拉起 QUIC 监听；`Host: demo.sdkwork.link` 的请求按「先查注册表」语义中继到 agent 的本地目标，响应原路返回（含 WebSocket 升级）。`enabled = false` 或缺省时运行时行为与无 Tunnel 完全一致（PRD §77）。

**HTTP 路由的优先级与判定方式**（同一监听器上隧道与本地面共存）：

| 数据面入口 | 谁在用 | 隧道查询时机 |
|---|---|---|
| `run_data_plane_until` / `…_with_cluster_overlay` | CLI `serve-nginx-compat`、集成测试 | 本地路由**之前**（该入口没有应用发布投递面） |
| `run_website_data_plane_until` / `…_with_(tls_)operations_until`、`…_from_config_until` | **线上边缘运行时 `sdkwork-webserver-website-delivery-edge-runtime serve`** | 同上（同一段代码），但多一道守卫：**投递面已声明该 host 时不接管** |

守卫问的是投递面的**路由表**（`CompiledWebsiteRuntimeSet::declares_authority`：精确 host 或一级通配后缀），不是它将要返回的状态码。这个区别是必须的 —— 两组状态互相重合并语义相反：

| 情形 | 状态 | 谁拥有 host |
|---|---|---|
| runtime set 里没有该 host（隧道域名的生产情形） | 404 | 无人 → 交给隧道 |
| runtime set 有该 host，但该路径/资源缺失 | 404 | 投递面 |
| 节点上没有任何 runtime set | 503 | 无人 → 交给隧道 |
| 有该 host，但其 provider 暂时超时 | 503 | 投递面 |

按状态码回退（例如"404 就交给隧道"）既漏掉 503 的无人情形，又会把正在服务的站点的瞬时 5xx 改道给隧道。按**路由表**判定则两个方向都精确。

优先级取向是安全的：隧道 agent 凭据是**共享凭据**（`TokenAuthenticator` 的文档明写 "V1 tokens are shared credentials; any valid token authenticates any declared device id"），所以隧道路由**不得遮蔽应用发布面已声明的 host**。代价是同一 host 既由应用面服务、又注册了隧道路由时应用面赢 —— 属运维配置冲突。应用面未声明但控制面可能有（Deploy 回退）的 host 由隧道接管：进程内的显式注册优先于一次控制面查询。

## 架构与 Crate 划分（高内聚低耦合，PRD §43）

```text
sdkwork-webserver-tunnel-core        领域模型：Device/Session/Route/Matcher/Policy/ID/Config/Error（serde-only）
        ↑
sdkwork-webserver-tunnel-protocol    STP/1 控制面消息、数据面流头、长度前缀帧编解码（尺寸上限强制）
        ↑
sdkwork-webserver-tunnel-transport   Transport 抽象 trait + QUIC（quinn + rustls TLS 1.3）实现 + TLS 材料
        ↑
sdkwork-webserver-tunnel             组合层：Gateway（监听/控制循环/注册表/会话表/TCP/UDP 监听/分发）、
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
- 认证失败按源 IP 固定窗口限速；访问者按路由策略（allowPublic / allowedIps / BearerToken）准入，HTTP / TCP / UDP 三个中继面走同一准入门 `admit()`，其中 allowPublic 与 allowedIps 相互独立（私网白名单可在 `allowPublic = false` 下单独放行）。
- 三种准入手段并非三面通用：**Bearer 只能随 HTTP `Authorization` 头到达**，裸 TCP/UDP 连接没有该信道。因此 "Bearer 保护 + TCP/UDP" 的组合在**注册期即被显式拒绝**（`AuthPolicy::is_satisfiable_on`，报文 `"Tcp routes cannot require bearer visitor authentication; use allowed_ips to restrict access"`），而不是注册成功后静默变成不可达路由；私有 TCP/UDP 路由以 allowedIps 白名单为主要准入手段。裸 UDP 数据报同样拒收匿名访客（FRP SUDP 语义，需访客客户端）。
- 资源上限：maxDevices / maxSessions / maxStreamsPerSession（信号量预算）/ maxRoutes / maxControlMessageBytes；同设备重复连接替换旧会话。
- 心跳（默认 10s）× 3 + idle 上限驱动过期会话清理；会话销毁级联注销路由并释放 TCP/UDP 监听端口。

## 可观测性（PRD §48–§49, §82）

- Prometheus 指标（数据面 operations `/metrics` 追加）：`sdkwork_tunnel_connections[_active]`、`sessions[_active]`、`streams[_active]`、`bytes_in/out`、`reconnects`、`errors`、`auth_failures`、`routes_active`。`status.ports` 聚合 TCP 与 UDP 监听端口。
- operations REST（回环）：`GET /tunnel/status`、`GET|POST /tunnel/routes`、`DELETE /tunnel/routes/{id}`。
- CLI：`sdkwork-webserver-tunnel status|list|remove --url <ops-base>`；诊断用 `doctor`（端点 → TLS → QUIC → STP 握手 → 认证 分级检查，PRD §83）。

## 重连与优雅关闭（PRD §27, §105）

Agent 断线后按 1s→2s→4s…60s 指数退避 + ±20% 抖动自动重连，成功注册后重置；路由热注册/注销经控制面完成，agent 无需重启。网关关闭顺序：停止 accept → 关闭 endpoint/会话 → 注销路由 → 释放 TCP 监听。

## 测试

- 单元：领域模型、协议 round-trip（`decode(encode(m)) == m`）、未知消息降级、帧尺寸上限、TLS 材料与指纹、注册表冲突、会话预算、退避。
- 集成：QUIC 回环、gateway+agent+本地 HTTP/TCP/UDP 端到端（含 UDP 数据报往返、私有 UDP 拒绝匿名）、通配符域名子域路由、数据面 `[tunnel]` 配置中继、§77 禁用兼容、认证拒绝、证书指纹拒绝。**服务端先发言的协议（SSH 这类）另有专门用例**：访客连上后一个字节都不发，断言本地目标在访客开口前就被拨号并收到字节 —— 懒拨号（等访客首字节）只让该用例红，「访客先说话」的 TCP 用例抓不到。
- webserver 集成（`tests/tunnel_relay.rs`）：同一份 `[tunnel]` 配置分别经**两个数据面入口**各跑一遍域名中继 —— 普通入口（`run_data_plane_until`）与**线上边缘运行时用的投递面入口**（`run_website_data_plane_with_operations_until` + `Some(executor)`）；另有 WebSocket 升级穿过隧道的用例（断言 101 之后的字节双向泵送）。
- webserver 集成（集群 hop）：轮询/离线剔除/全离线 503、防环标记的两种失效模式（伪造标记不得被平衡、已带标记的兄弟必须本地服务）、**WebSocket 升级泵**（实例侧断言收到的握手携带 `x-served-by-cluster-hop` 与访客真实地址，101 之后的字节双向泵送）。
- 集群成员端到端（`cluster_member_e2e`）：LAN 直连与 **TUNNEL 中继**两条传输各跑一遍「注册 → 心跳 → 拉清单 → 应用 → ack」，TUNNEL 用例断言上线模式确实以 `joinMode=TUNNEL` + `routeDomain` 到达注册表；另有 cordon/drain 完成闭环与「工作未结束不得报完成」两例。

## 已知限制（V1，PRD §87/§133 对齐）

- 状态存于内存（PRD §116）；网关重启后 agent 自动重连并重建路由，REST create-route 的离线设备声明不持久化。
- Gateway→agent 的 remove（REST 删除）不通知 agent；agent 重连后会重新注册其模板路由。
- **隧道运行清单无控制台消费面**：会话 / 路由 / 端口 / 字节 / 重连这些数据只有 operations REST（`GET /tunnel/status`、`GET|POST /tunnel/routes`）与 Prometheus 指标两个出口，`apps/**` 无任何消费方。集群管理页呈现的是**每实例**的 `joinMode` 与隧道路由域名（`WebserverWorkspace.tsx`），不是隧道网关自身的运行清单——两者不要混为一谈。
- mTLS（agent 侧证书）、隧道专属 ACME、每路由带宽限制、多网关集群路由、P2P/Mesh（xtcp）为 P1/P2（PRD §8），未实现。
