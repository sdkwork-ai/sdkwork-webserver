# TECH — Tunnel / FRP 功能对齐矩阵

> **基准（benchmark）**：FRP 官方文档 `gofrp.org/en/docs/features/*`（抓取于 2026-09-21，文档站自身标注
> 对应的最新版本为 frp **v0.68.0**）。
> **对象**：本仓内网穿透实现 —— `sdkwork-webserver-tunnel-core` / `-protocol` / `-transport` /
> `sdkwork-webserver-tunnel` + webserver 集成面（`sdkwork-webserver-core` 的 `[tunnel]` 配置节、
> `sdkwork-api-webserver-standalone-gateway` 的数据面中继与 operations REST）。
> **产品定位见** `docs/tunnel.md`；本文只回答一个问题：**FRP 有的，我们有没有、做到什么程度、证据在哪。**

## 0. 状态口径

| 标记 | 含义 |
|---|---|
| ✅ **完整** | 能力等价实现，且**有测试覆盖**（证据列给出用例名或 `file:line`） |
| 🟡 **部分 / 形态不同** | 本仓用另一种机制达到等价目的，或只覆盖了 FRP 该特性的一部分 |
| ❌ **未实现** | 无对应实现（按优先级见 §4） |
| ⛔ **刻意不做** | 不做是有理由的，理由写在备注里；**不是缺口** |

「刻意不做」不等于"永远不做"——它是**当前设计下的取舍已被论证**的意思。凡理由来自 FRP 自身的
设计缺陷或架构不变量，备注里都引了原文/代码原话。

---

## 1. 结论速览

| FRP 分类 | 条目数 | ✅ | 🟡 | ❌ | ⛔ |
|---|---|---|---|---|---|
| A 代理类型 | 12 | 4 | 2 | 6 | 0 |
| B 传输与通信安全 | 13 | 4 | 6 | 3 | 1 |
| C 路由与匹配 | 9 | 4 | 1 | 4 | 1 |
| D 访问控制与真实 IP | 8 | 5 | 2 | 1 | 0 |
| E 负载均衡与健康检查 | 3 | 0 | 0 | 3 | 0 |
| F 管理与运维面 | 13 | 3 | 4 | 9 | 0 |
| G 配置能力 | 6 | 2 | 2 | 2 | 0 |
| **合计** | **64** | **22** | **17** | **28** | **2** |

**一句话结论**：本仓覆盖了 FRP 在**「单网关 + 反向连接 + HTTP/TCP/UDP 中继 + Token 认证 + ACL +
Prometheus」**这条主干上的全部能力，并在三处**超过** FRP（声明式控制面、
按 host 的路由表级裁决、TCP/UDP 独立端口命名空间）；但**完全没有** FRP 的
**访问端（visitor）家族**（stcp / sudp / xtcp / tcpmux / plugins / VirtualNet），也没有
**负载均衡与健康检查**、**连接池与限速**、**SSH 隧道网关**。

这不是"实现不全"，而是**两套不同的产品边界**：FRP 是通用穿透工具（用户自带两端），本仓是
**平台边缘设施**（一端是平台托管的边缘运行时，另一端是平台下发的 agent）。§4 里逐条说明了
哪些缺口值得补、哪些属于边界差异。

---

## 2. 逐项对照

### A. 代理类型

| # | FRP 特性（官方字段） | 状态 | 本仓对应物 / 证据 |
|---|---|---|---|
| A1 | `type = "tcp"` + `localIP`/`localPort`/`remotePort` | ✅ | `TunnelProtocolKind::Tcp` + `RouteMatcher::Port`（`tunnel-core/src/route.rs:24`）；网关按端口绑定监听（`tunnel/src/gateway/listeners.rs:127` `accept_loop`）。e2e：`tunnel_end_to_end.rs::tcp_route_relays_raw_bytes_through_gateway_port` |
| A2 | `type = "udp"` | ✅ | `TunnelProtocolKind::Udp` + 独立 UDP 监听与会话表（`tunnel/src/gateway/udp_listeners.rs`）。e2e：`tunnel_end_to_end.rs::udp_route_relays_datagrams_through_gateway` |
| A3 | `type = "http"`（按 Host 路由） | ✅ | `TunnelProtocolKind::Http` + `RouteMatcher::Domain`；`connect_http_stream` 按 host 查注册表（`tunnel/src/gateway/dispatch.rs:121`）。e2e：`http_and_tcp_traffic_relay_end_to_end` |
| A4 | `type = "https"` | ✅ | 与 A3 同一条路径：访客 TLS 由 webserver 边缘终结，隧道内层再走 QUIC/TLS 1.3（`transport/lib.rs:9`）。无独立 `https` 枚举值，因为**终结位置不同**（见 C8） |
| A5 | `customDomains`（多域名） | ✅ | 每个域名一条路由；注册期拒绝跨会话冲突（`gateway/registry.rs:71-99`） |
| A6 | `subdomain` / `subdomainHost` | 🟡 | 等价能力由**通配符路由**提供（`RouteMatcher::domain("*.x")`，最长后缀胜出，`registry.rs:206`），而非「`${subdomain}.${host}` 自动拼装」。agent 侧有 `domainSuffix` 用于一键 expose 时展开域名（`tunnel-core/src/config.rs:158`）。差异：FRP 的 subdomain 由**服务端**统一后缀，本仓后缀白名单是 `gateway.domainSuffixes`（`control.rs:581`），拼装发生在 agent 模板 |
| A7 | `type = "stcp"`（+ visitor 端 `sk`/`serverName`/`bindPort`） | ❌ | 需要**访问侧客户端**。`tunnel-core/src/policy.rs:62` 原话：「FRP STCP/SUDP parity requires a visitor client, which V1 does not implement」 |
| A8 | `type = "sudp"` | ❌ | 同上。当前 UDP 平面**拒收匿名访客**（`docs/tunnel.md:113`），等价于"没有 visitor 就没有 SUDP" |
| A9 | `type = "xtcp"`（P2P 打洞 / `fallbackTo` / `keepTunnelOpen`） | ❌ | `docs/tunnel.md:140` 列为 P1/P2（P2P/Mesh 未实现） |
| A10 | `type = "tcpmux"` + `multiplexer = "httpconnect"` | ❌ | 需在单端口解析 HTTP CONNECT 并按 Host 分派；本仓无 CONNECT 解析器 |
| A11 | `[[visitors]]` 段（含 `bindPort = -1` 仅接受 fallback） | ❌ | 无 visitor 角色。**这是 §4 里唯一被判定为"架构级缺口"的一项** |
| A12 | `TunnelTarget::UnixSocket`（本仓自有，非 FRP） | 🟡 | 枚举值存在（`route.rs:40`）但**无生产者**：`parse_tcp`/`parse_udp`/`parse_for_protocol` 都不会构造它，全仓 grep 无第二处引用。属**未接线的预留变体**，见 §4-G1 |

### B. 传输与通信安全

| # | FRP 特性（官方字段） | 状态 | 本仓对应物 / 证据 |
|---|---|---|---|
| B1 | `transport.protocol = "quic"` | ✅ | **唯一传输**（`transport/quic.rs`，quinn + rustls TLS 1.3，ALPN `sdkwork-tunnel/1`，`transport/lib.rs:26`） |
| B2 | `transport.protocol = "tcp"` / `"kcp"` / `"websocket"` / `"wss"` | 🟡 | 抽象已隔离（`TunnelClientTransport` / `TunnelServerTransport` trait，`transport/lib.rs:107-129`；crate 文档明写「a future TCP or WebSocket transport can slot in without touching callers」），但**未实现第二个实现**。缺 TCP 传输在受限网络（仅放行 443/TCP）下是真实可感知的差距 —— 见 §4-G2 |
| B3 | `transport.tls.enable` / 全局加密 | ✅ | 强制，且**无明文档位**：QUIC 必然 TLS 1.3。`transport/lib.rs:9` 原话「the tunnel speaks QUIC over TLS 1.3 only」 |
| B4 | `transport.useEncryption`（每代理 aes-128-cfb） | ⛔ | FRP 官方自己说「When TLS is enabled between frpc and frps, traffic will be globally encrypted, and encryption on individual proxies is no longer needed」。本仓全链路已加密 ⇒ 逐代理加密**无增量价值** |
| B5 | `transport.useCompression`（snappy） | ❌ | 未实现，全仓无 `compress`/`snappy`/`zstd` 引用。QUIC 层无内置压缩 |
| B6 | `transport.tcpMux`（连接多路复用，默认开） | 🟡 | **能力等价、形式不同**：QUIC 的 bidi stream **天然多路复用**，每个访客连接 = 一条 stream，不需要开关也不需要额外的 mux 协议。上限是 `TransportOptions::max_concurrent_bidi_streams`（默认 512，`transport/lib.rs:76`） |
| B7 | `transport.poolCount` / `maxPoolCount`（连接池） | ❌ | 未实现。FRP 官方注明「When TCP multiplexing is enabled, the improvement from connection pooling is limited」⇒ 优先级低（§4-G5） |
| B8 | `transport.bandwidthLimit` / `bandwidthLimitMode`（每代理限速） | ❌ | 未实现；`docs/tunnel.md:140` 列为 P1/P2 |
| B9 | `transport.heartbeatInterval` | ✅ | `network.heartbeatIntervalSecs`（默认 10s，`config.rs:18`）+ STP 心跳消息 `Heartbeat`/`HeartbeatAck`（`protocol/message.rs:249`）。过期判据 `min(idle, heartbeat×3)` |
| B10 | `transport.dialServerTimeout` / `dialServerKeepalive` | 🟡 | 超时拆成三档：`timeout.connect` / `handshake` / `streamOpen`（`config.rs:223`）；keepalive 是 `TransportOptions::keep_alive_interval_ms`（默认 5000ms，`transport/lib.rs:76`）—— **可配但未从配置文件暴露** |
| B11 | `transport.proxyURL`（走 HTTP/SOCKS5 出口代理） | ❌ | 未实现 |
| B12 | `quicBindPort` / `kcpBindPort` | 🟡 | 只有 `gateway.listen`（默认 `0.0.0.0:8443`，`config.rs:135`） |
| B13 | `quic.keepalivePeriod` / `maxIdleTimeout` / `maxIncomingStreams` | 🟡 | 等价参数存在但**硬编码为默认值**，未进 `[tunnel]` 配置节：`TransportOptions::default()`（idle 300s / keepalive 5s / 512 streams） |
| B14 | **服务端先发言的协议**（SSH banner、FTP、RDP） | ✅ | **本仓显式覆盖**：`tunnel_end_to_end.rs::tcp_route_relays_a_server_first_binary_protocol` 断言「访客一个字节都不发，本地目标也必须被拨号」；配套两条路径的 `TCP_NODELAY`（`listeners.rs:157`） |

### C. 路由与匹配

| # | FRP 特性 | 状态 | 本仓对应物 / 证据 |
|---|---|---|---|
| C1 | `vhostHTTPPort` 单端口多站点（按 Host 分派） | ✅ | HTTP 路由挂在 webserver 已有的 80/443 监听器上，由数据面按 `Host` 查隧道注册表（`docs/tunnel.md:63`；`standalone-gateway/src/data_plane/handler.rs` 的隧道路由段） |
| C2 | 精确域名匹配 | ✅ | `RouteMatcher::Domain`，归一化（小写、去尾点、无 scheme/port），`route.rs:98` |
| C3 | 通配符域名 `*.suffix` | ✅ | 最长后缀胜出、精确优先于通配、**通配不覆盖裸后缀**（`registry.rs:206`；单测 `wildcard_matches_subdomains_longest_suffix_wins`）。e2e：`tunnel_end_to_end.rs::wildcard_http_route_matches_subdomains_only` |
| C4 | `locations`（URL 前缀路由 + 最长前缀匹配） | ❌ | 隧道层**无路径匹配**：全仓 `locations`/`location_match`/`path_prefix` 零命中。隧道只裁决到 host，路径语义留在投递面（`CompiledWebsiteRuntimeSet::select_route`） |
| C5 | `routeByHTTPUser` | ❌ | 未实现 |
| C6 | `remotePort`（一端口一目标） | ✅ | 注册表按 `(is_datagram, port)` 建索引，**同一端口一个路由**，跨会话冲突即拒（`registry.rs:90`）；`listeners.rs:28` 文档解释了为什么不能多路由共享端口（裸 TCP 无路由键） |
| C7 | 端口范围映射（`parseNumberRangePair` 模板） | ❌ | 无 Go 模板能力（见 G2），因此无从实现 |
| C8 | 端口复用（`vhostHTTPSPort == bindPort` 同端口） | ⛔ | **架构上不需要**：隧道 QUIC 监听是独立端口，HTTP 路由复用 webserver 的边缘监听器，两者从不争抢同一端口 |
| C9 | 按 Host 的路由表级裁决（本仓自有） | ✅ | `CompiledWebsiteRuntimeSet::declares_authority`：投递面已声明的 host 不被隧道遮蔽。**比 FRP 强**（FRP 的 vhost 表与 tcpmux 表是并列独立面，无冲突裁决）。见 `docs/tunnel.md:65-83` |

### D. 访问控制与真实 IP

| # | FRP 特性（官方字段） | 状态 | 本仓对应物 / 证据 |
|---|---|---|---|
| D1 | `auth.method = "token"` + `auth.token` | ✅ | `TokenAuthenticator`：从**环境变量名**解析（`agentTokenEnv`，`config.rs:120`），常量时间比较，无 token 时 fail closed（`security/mod.rs:38-78`） |
| D2 | `auth.tokenSource.type = "file"`（v0.64.0） | 🟡 | 环境变量机制覆盖同一诉求（凭据不进配置文件），且**不含文件读取面**。等价安全性、更小攻击面 |
| D3 | `auth.tokenSource.type = "exec"`（v0.66.0） | ⛔ | FRP 自己要求 `--allow-unsafe=TokenSourceExec` 才允许；本仓不引入"配置驱动执行外部命令"这个面 |
| D4 | `auth.method = "oidc"`（Client Credentials Grant） | ❌ | 未实现。平台侧身份由自家 IAM 承担，OIDC 到 frps 的直连认证无对应场景 |
| D5 | `httpUser` / `httpPassword`（HTTP BasicAuth 保护代理） | 🟡 | 等价能力是 `RoutePolicy.auth = bearer_token` + `visitorTokens`（`policy.rs:85`），走 `Authorization: Bearer` 而非 `WWW-Authenticate: Basic` 挑战。**同为 HTTP 平面限定**，与 FRP 的「仅 http 类型代理」边界一致 |
| D6 | `X-Forwarded-For`（真实访客 IP，默认开） | 🟡 | webserver 本地边缘有完整的客户端 IP 解析（`sdkwork-webserver-core/src/config/model.rs:883` 的 `XForwardedFor` 枚举 + `ClientIpSource`），**隧道链路本身不额外注入**：`RelayVisitor.ip` 来自调用方已解析的 IP（`dispatch.rs:28`）。链路正确性依赖调用方传入 |
| D7 | `transport.proxyProtocolVersion = "v1"\|"v2"`（向 agent 本地目标注入 PROXY 头） | ❌ | 隧道链路不注入。webserver 自身的 `proxy_protocol` 配置（`config/model.rs:784`）作用于**本地 HTTP 边缘 → 上游**，与隧道是两条独立链路，不可混为一谈 |
| D8 | `allowPorts`（服务端端口白名单） | 🟡 | 等价约束是 `gateway.domainSuffixes`（**域名**白名单，`control.rs:581`）。端口不由服务端分配 —— 路由的公开端口来自 agent 本地模板，这是 SSRF 边界（`docs/tunnel.md:106`）的代价。**无端口白名单**：一个持有合法 token 的 agent 可以占用任意未被占用的端口 |
| D9 | 认证失败限速（per-peer） | ✅ | `AuthRateLimiter` 固定窗口 per-IP（`security/mod.rs:83`） |
| D10 | 资源上限（`maxPorts`/`maxPoolCount` 等） | ✅ | `maxDevices` / `maxSessions` / `maxStreamsPerSession`（信号量）/ `maxRoutes` / `maxControlMessageBytes`（`config.rs:277`）；未配置 token 时全拒 |
| D11 | 路由归属校验 | ✅ | `require_route_owner`（`security/mod.rs:140`）+ 注册表跨会话冲突拒绝 |
| D12 | 每设备凭据 / mTLS | ❌ | `security/mod.rs:69` 原话：「V1 tokens are shared credentials; any valid token authenticates any declared device id. Device-bound credentials arrive with mTLS (P1)」 |

### E. 负载均衡与健康检查

| # | FRP 特性 | 状态 | 说明 |
|---|---|---|---|
| E1 | `loadBalancer.group` / `groupKey`（同组多代理随机分担） | ❌ | 未实现。**架构上被注册表语义排除**：`registry.rs` 的跨会话冲突检查意味着「同一 host / 同一端口只能有一个所有者」，天然不支持"同组多副本"（见 §4-G3） |
| E2 | `healthCheck.type = "tcp"\|"http"` + `timeoutSeconds` / `maxFailed` / `intervalSeconds` / `path` | ❌ | 未实现 |
| E3 | 健康检查 × 负载均衡的高可用组合 | ❌ | 依赖 E1+E2 |

### F. 管理与运维面

| # | FRP 特性 | 状态 | 本仓对应物 / 证据 |
|---|---|---|---|
| F1 | frps Dashboard（`webServer.addr/port/user/password`） | 🟡 | 无独立 dashboard 进程/端口；运行清单走 webserver 的 **operations REST**：`GET /tunnel/status`、`GET|POST /tunnel/routes`、`DELETE /tunnel/routes/:id`（`standalone-gateway/src/data_plane/operations.rs:174-188`） |
| F2 | `enablePrometheus` + `GET /metrics` | ✅ | 12 个指标（`tunnel/src/metrics.rs`），由数据面 `/metrics` 追加输出。单测 `prometheus_render_contains_canonical_names` 锁名字 |
| F3 | 内存监控（Dashboard 用） | 🟡 | `TunnelMetricsSnapshot` + `SessionTable::snapshots()` 提供等价的即时视图，**不保留历史**（无 7 天滑动窗口） |
| F4 | frpc `webServer` + `frpc reload` / `status` / `stop` | 🟡 | CLI 有 `status` / `list` / `remove`（`tunnel/src/bin/…:37-45`、`cli.rs:248-317`）；**无 `reload`**（agent 断线重连即按新配置重建路由，见 `docs/tunnel.md:125`）；无 agent 侧 HTTP admin API |
| F5 | `store.path`（运行时增删代理并持久化） | ❌ | 状态全在内存且**刻意不持久化**（`docs/tunnel.md:137`）；控制面的 `POST /tunnel/routes` 是"声明"，agent 离线时不生效 |
| F6 | `frpc verify` / `frps verify`（配置文件预检） | 🟡 | 有 `sdkwork-webserver-tunnel doctor`：端点 → TLS → QUIC → STP 握手 → 认证 五级检查（`cli.rs:317`）。**缺纯离线语法预检** |
| F7 | CLI 子命令全集 | 🟡 | `gateway` / `agent` / `expose` / `list` / `remove` / `status` / `doctor`（7 条）。FRP 的 `frpc reload|status|verify`/`frps verify` 见 F4/F6 |
| F8 | `sshTunnelGateway.*`（SSH `-R` 反向隧道网关，无需 frpc） | ❌ | 未实现。**唯一一个"客户端零安装"的接入方式**，见 §4-G4 |
| F9 | 服务端插件 `[[httpPlugins]]`（`Login`/`NewProxy`/`CloseProxy`/`Ping`/`NewWorkConn`/`NewUserConn` RPC 扩展） | ❌ | 未实现 |
| F10 | 客户端插件 `plugin.type`（`unix_domain_socket`/`http_proxy`/`socks5`/`static_file`/`https2http`/`https2https`/`http2https`/`http2http`…） | ❌ | 未实现。本仓的对应位置是 agent 只做**被动转发**，任何"本地服务"由用户自己跑 |
| F11 | `metadatas`（自定义元数据随 RPC 传给插件） | ❌ | 依赖 F9/F10 |
| F12 | VirtualNet（TUN 虚拟网络，v0.62.0 Alpha） | ❌ | 未实现。FRP 官方自称 Alpha 且「不要用于生产」，本仓不跟随是合理的 |
| F13 | 优雅关闭 / 重连退避 | ✅ | 网关关闭顺序：停 accept → 关 endpoint/会话 → 注销路由 → 释放监听（`docs/tunnel.md:125`）；agent 1s→2s→…→60s 指数退避 + ±20% 抖动，注册成功后重置 |

### G. 配置能力

| # | FRP 特性 | 状态 | 本仓对应物 / 证据 |
|---|---|---|---|
| G1 | TOML / YAML / JSON | 🟡 | `[tunnel]` 段挂在 webserver 的 `server.toml` 上；`TunnelConfig` 本身是 serde 模型（camelCase，`deny_unknown_fields`），可序列化为 JSON（单测 `gateway_and_agent_sections_serialize_camel_case_wire_names` 锁定键名）。**webserver 配置加载器只读 TOML，不支持 YAML** |
| G2 | Go 模板 + `{{ .Envs.X }}` 环境变量插值 + `includes` 多文件拆分 + YAML anchors | ❌ | 无通用模板渲染、无 `includes`、无 YAML anchor。等价手段只有"凭据用环境变量**名**"这一条（`tokenEnv`/`tlsCertPemEnv`），且是**点对点**的，不是通用插值 |
| G3 | 严格校验（`--strict-config=false` 可关） | ✅ | `deny_unknown_fields` **恒开且不可关**（`config.rs:24`），比 FRP 更严：写错的字段名一定报错，不会被静默忽略 |
| G4 | 分角色配置文件（frpc.toml / frps.toml） | ✅ | 同一份 `[tunnel]` 段内 `[tunnel.gateway]` / `[tunnel.agent]` 双向可选、按角色取用；CLI 也有独立的 `gateway` / `agent` 两套 flag |
| G5 | 主开关 / 向后兼容 | ✅ | `enabled = false` 必须让 webserver **逐字节不变**（`config.rs:26`）；单测 `route_templates_are_only_validated_when_the_feature_is_enabled` 锁定"禁用时不校验模板" |
| G6 | 密钥永不入配置文件 | ✅ | `token`/证书字段存的都是**环境变量名**（`config.rs:1-7`），单测 `agent_defaults_hold_env_var_names_never_secrets` + `gateway_defaults_fail_closed_and_never_carry_a_token` 锁定 |

---

## 3. 测试覆盖矩阵

### 3.1 隧道路由面（`tests/tunnel_relay.rs`，4 例）

| 用例 | 覆盖 |
|---|---|
| 域名中继（普通数据面入口 `run_data_plane_until`） | C1 隧道查询可达 |
| 域名中继（**线上投递面入口** + `Some(executor)`） | C1 + C9 **生产入口守卫** |
| WebSocket 升级穿过隧道 | 101 之后字节双向泵送 |
| （含）`declares_authority` host 粒度 | C9 单测（在 `webserver-core/tests/website_runtime_set.rs`） |

### 3.2 端到端（`tests/tunnel_end_to_end.rs`，8 例）

`http_and_tcp_traffic_relay_end_to_end`（A1+A3）、`tcp_route_relays_raw_bytes_through_gateway_port`（A1）、
`udp_route_relays_datagrams_through_gateway`（A2）、`wildcard_http_route_matches_subdomains_only`（C3）、
`private_udp_route_denies_anonymous_visitors`（A8/D8）、
`tcp_route_relays_a_server_first_binary_protocol`（**B14**）、
`wrong_token_is_rejected_and_route_never_registers`（D1）、
`raw_client_through_real_gateway_reads_hello`（B1）。

### 3.3 单元（本轮补强后 111 例）

| crate | 例数 | 覆盖点 |
|---|---|---|
| `tunnel-core` | **45** | 配置模型/默认值/键名契约、ID 校验、路由与匹配器、策略准入三通道、target 解析 |
| `tunnel-protocol` | **25** | 帧编解码与尺寸上限、全部消息 round-trip、未知变体降级、已知类型缺字段硬失败、错误码映射 |
| `tunnel` | **41** | 注册表（含 TCP/UDP 命名空间）、会话表（预算/过期/替换）、TCP 监听集、UDP 监听集、指标、安全（token/限速/归属/ACL）、service 门面 |
| `tunnel-transport` | 13 | TLS 材料与指纹、QUIC 选项 |

### 3.4 本轮新增的 30 例（补齐"已实现但无覆盖"的不变量）

| 新增用例 | 锁定的不变量 | 变异验证 |
|---|---|---|
| `route::udp_route_requires_port_matcher` | UDP 路由必须匹配端口 | — |
| `route::bearer_visitor_auth_is_rejected_on_tcp_and_udp_routes` | **D5/D8 的构造期拒绝**（文档明写但无测试） | ✅ `is_satisfiable_on` 恒真 ⇒ 仅此例 + 谓词单测红 |
| `route::zero_port_target_is_rejected` | target 端口 0 不可路由 | — |
| `route::route_name_length_is_bounded` | 名字 1..=128 | — |
| `route::protocol_decides_the_target_kind_and_its_label` | `parse_for_protocol` 单一入口 | — |
| `config::limits_defaults_match_prd_ceilings` | D10 的默认上限 | — |
| `config::agent_defaults_hold_env_var_names_never_secrets` | G6 | — |
| `config::gateway_defaults_fail_closed_and_never_carry_a_token` | D1/G6 | — |
| `config::udp_template_resolves_a_datagram_target_and_requires_a_port` | A2 模板语义 | — |
| `config::a_template_name_must_be_usable_as_a_route_id` | 模板名 → route id 的派生耦合 | — |
| `config::route_templates_are_only_validated_when_the_feature_is_enabled` | G5 | — |
| `config::gateway_and_agent_sections_serialize_camel_case_wire_names` | G1 跨组件键名契约 | — |
| `registry::tcp_and_udp_port_namespaces_are_independent` | **C6 的双命名空间** | ✅ `is_datagram` 恒假 ⇒ 3 例红 |
| `registry::a_udp_only_port_is_invisible_to_tcp_visitors` | C6 跨平面隔离 | ✅ 同上 |
| `registry::port_conflicts_are_per_namespace_and_per_session` | C6 冲突粒度 | ✅ 同上 |
| `registry::a_route_without_an_owning_session_is_rejected` | D11 前提 | — |
| `registry::version_changes_on_every_mutation_and_not_on_a_no_op` | 版本单调（分派器缓存判据） | — |
| `registry::list_covers_every_index_family_sorted_and_without_duplicates` | F1 输出稳定性 | — |
| `registry::get_resolves_routes_from_every_index_family` | 按 id 反查三索引 | — |
| `registry::hot_update_moves_a_route_between_matcher_families` | 热更新释放废弃索引项 | — |
| `sessions::reconnect_replacement_closes_the_superseded_connection` | F13 不泄漏连接 | — |
| `sessions::idle_expiry_clears_the_device_index_and_the_stream_budget` | D10/F13 级联清理 | — |
| `sessions::touch_rejects_an_unknown_session` | 失败不改状态 | — |
| `sessions::snapshots_are_sorted_and_carry_their_route_ids` | F3/F1 | — |
| `sessions::device_budget_trips_at_the_limit` | D10 | — |
| `sessions::remove_reports_the_entry_and_clears_the_device_index` | 幂等移除 | — |
| `message::a_known_type_with_a_missing_field_is_a_hard_protocol_error` | 只有未知**变体**才降级 | — |
| `message::unknown_fields_inside_a_known_message_are_tolerated` | N-1 前向兼容缝隙 | — |
| `message::every_domain_error_maps_to_a_wire_category` | 错误码映射全表 | — |
| `message::authenticate_token_is_present_only_on_the_authenticate_message` | 凭据不回流 | — |

### 3.5 复现命令

```bash
# 本矩阵全部单元证据（111 例）
cargo test -p sdkwork-webserver-tunnel-core -p sdkwork-webserver-tunnel-protocol \
           -p sdkwork-webserver-tunnel --lib

# 端到端 + 中继面（12 例）
cargo test -p sdkwork-webserver-tunnel --test tunnel_end_to_end --test transport_bisect
cargo test -p sdkwork-api-webserver-standalone-gateway --test tunnel_relay
```

---

## 4. 缺口清单（按建议优先级）

**G1 · `TunnelTarget::UnixSocket` 是死变体** — 枚举值存在但无任何生产者（无解析路径、无测试、
全仓仅一处引用）。两个选择：接上（agent 模板支持 `unix:/path`，POSIX 专用）或删掉。
现状是"看起来支持、实际永远走不到"，属 §4.2 意义上的**假能力**。

**G2 · 无 TCP/WebSocket 传输** — 当前唯一传输是 QUIC/UDP。在只放行 TCP 443 的企业网络里，
agent 无法建连。抽象层已经预留（`transport/lib.rs:107-129` 的两个 trait），实现一个
`TcpTunnelTransport` 是**受控增量**，不需要动上层。

**G3 · 无负载均衡与健康检查（E1–E3）** — 取决于产品是否要"多副本高可用"。注意这**不是纯增量**：
注册表的跨会话冲突语义（同一 host/端口一个所有者）与 group 语义直接冲突，要做必须先改注册表
的所有权模型。属**设计级**改动。

**G4 · 无 SSH 隧道网关（F8）** — FRP 用它做"客户端零安装"接入（`ssh -R`）。
本仓的 agent 是平台托管的，纯"临时接入"场景目前无路径。属产品定位差异，非缺陷。

**G5 · 无连接池 / 限速 / 压缩（B5/B7/B8）** — 三项都是**纯增量**且互相独立。
优先级判断：限速（B8，多租户公平性）> 压缩（B5，QUIC 已有头部压缩，收益有限）>
连接池（B7，FRP 官方自己说多路复用下收益有限）。

**G6 · `TransportOptions` 不可配（B13）** — idle 300s / keepalive 5s / 512 streams 是硬编码默认值。
搬进 `[tunnel.network]` 是低风险改动，但需要同时确认"transport crate 只把参数当**上限**"这个
契约（`transport/lib.rs:59-61`）不被破坏。

**G7 · 无端口白名单（D8）** — 一个持有合法共享 token 的 agent 可占用任意空闲端口。
当前只有**域名**白名单（`domainSuffixes`）。如果 token 是共享凭据（D12 确认是），
这条的暴露面比 FRP 的 `allowPorts` 更大。**建议评估**是否补一个端口段白名单。

**G8 · 无离线配置预检（F6）** — `doctor` 需要连上网关才能体检；纯语法/语义预检（等价 `frpc verify`）
在 agent 侧排错时更早失效、更省事。

---

## 5. 与 FRP 的三处差异（本仓更强的地方）

| 维度 | FRP | 本仓 |
|---|---|---|
| **控制面** | 只能改配置文件重启 | 声明式 REST（`POST /tunnel/routes`）+ 网关→agent 的 `DeclareRoute`，agent 只激活与本地模板匹配的声明（SSRF 边界） |
| **多 surface 冲突** | vhost / tcpmux / bindPort 是并列独立面，无裁决 | 按**路由表**（不是状态码）裁决：投递面已声明的 host 不被隧道遮蔽（C9） |
| **端口命名空间** | 未区分 TCP/UDP 索引 | `(is_datagram, port)` 双命名空间，同一端口号可各挂一条，且 TCP 访客永远看不到 UDP 路由（C6） |
| **配置严格性** | `--strict-config=false` 可关 | `deny_unknown_fields` 恒开不可关（G3） |
| **凭据** | `auth.token` 明文可写进配置文件 | 只存环境变量名，凭据永不进文件/git（G6） |
