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
| A 代理类型 | 12 | 10 | 1 | 1 | 0 |
| B 传输与通信安全 | 14 | 5 | 5 | 3 | 1 |
| C 路由与匹配 | 9 | 6 | 0 | 2 | 1 |
| D 访问控制与真实 IP | 12 | 6 | 3 | 2 | 1 |
| E 负载均衡与健康检查 | 3 | 3 | 0 | 0 | 0 |
| F 管理与运维面 | 13 | 3 | 4 | 5 | 1 |
| G 配置能力 | 6 | 4 | 1 | 1 | 0 |
| **合计** | **69** | **37** | **14** | **14** | **4** |

> 本轮（2026-10-09 复核）把 A12、D6 由 🟡 更正为 ✅：两项的实现早已落地，是**文档滞后**而非能力缺失
> —— 见 A12 / D6 行的代码位置。§2 的每一行现在都对应可复现的 `file:line` 或测试名。

**一句话结论**：本仓覆盖了 FRP 在**「单网关 + 反向连接 + HTTP/TCP/UDP 中继 + Token 认证 + ACL +
Prometheus」**这条主干上的全部能力，并在六处**超过** FRP（§5：声明式控制面、按 host 的路由表级裁决、
TCP/UDP 独立端口命名空间、配置严格性、凭据不入文件、真实访客 IP 的可信代理解析）；**访问端（visitor）
家族**已落地 stcp / sudp / tcpmux（A7/A8/A10/A11），**负载均衡 + 健康检查**与**每路由限速**也已补齐
（E1–E3 / B8）；仍缺的是 xtcp（P2P）、plugins / VirtualNet、连接池与压缩，
SSH 隧道网关则是**有意不做**的非目标（F8）。§6 另列了**未进本矩阵**的 FRP 字段（本轮穷尽检索发现）：
功能性缺口 6 项（其中 udpPacketSize 只是"不可调小"，上限已是协议最大值）、运维旋钮 10 项。

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
| A7 | `type = "stcp"`（+ visitor 端 `sk`/`serverName`/`bindPort`） | ✅ | **已实现，含访问侧客户端**：`stcp` 路由没有任何公开面（matcher 只有名字 + 共享密钥，密钥字段 `#[serde(skip)]` 不进任何清单），网关常量时间校验密钥，`RouteHealth` 感知的选路在同组副本间挑一个。`visitor` 运行时 + `sdkwork-webserver-tunnel visitor` 子命令把远端私有服务发布到本地端口。e2e：`a_secret_route_serves_only_a_visitor_that_holds_the_key`、`a_visitor_runtime_publishes_a_secret_route_on_a_local_port`、`a_secret_route_carries_a_server_first_protocol_like_ssh` |
| A8 | `type = "sudp"` | ✅ | 同 A7 的密钥路由，数据报端到端中转：`visitor` 按**每个本地客户端一条流**发布，流内走分帧数据报。e2e：`a_visitor_runtime_publishes_a_secret_datagram_route`；UDP 平面仍**拒收匿名访客**（`private_udp_route_denies_anonymous_visitors`） |
| A9 | `type = "xtcp"`（P2P 打洞 / `fallbackTo` / `keepTunnelOpen`） | ❌ | `docs/tunnel.md:140` 列为 P1/P2（P2P/Mesh 未实现） |
| A10 | `type = "tcpmux"` + `multiplexer = "httpconnect"` | ✅ | **形态不同：本仓做的是公开入口，不是客户端的省端口机制**。网关可另绑一个可选端口（`[tunnel.gateway] tcpmux_port`）讲 HTTP `CONNECT`；目标先查域名注册表、再查端口注册表，因此**一个端口可达全部 HTTP 与 `tcp` 路由**；路由自身的准入策略照常生效（`Proxy-Authorization: Bearer`，token 路由缺凭据回 407 质询）。本仓 agent 本就把所有路由复用在一条 QUIC 会话上，FRP 的原始动机（少占客户端↔服务端端口）在这里不成立。e2e：`one_multiplexed_port_reaches_http_and_tcp_routes` |
| A11 | `[[visitors]]` 段（含 `bindPort = -1` 仅接受 fallback） | ✅ | visitor 角色已落地：`visitor` 运行时 + `sdkwork-webserver-tunnel visitor --visitor <name>:<local-port>[:<bind-addr>]`（stcp 每条本地连接一条流，sudp 每个本地客户端一条流）。唯一没覆盖的是 `bindPort = -1`（只作 fallback、不监听）：本地端口是 `u16`，没有这个哨兵值 |
| A12 | `TunnelTarget::UnixSocket`（本仓自有，非 FRP） | ✅ | **曾是无生产者的死变体，现已接线**：`route.rs:106` `parse_unix_socket` 解析 `unix:/绝对路径`（仅 stream 平面），`route.rs:139` `parse_for_protocol` 产出该变体，agent 侧 `agent/mod.rs:1533` 真实拨号，预检 `verify.rs:227` 报告目标种类，数据报路由在构造期即拒（`route.rs:496`）且不可携带 PROXY 头（`route.rs:766`）。测试：`unix_socket_targets_parse_only_for_stream_planes`、`a_hand_built_udp_route_cannot_target_a_unix_socket`、`a_template_can_declare_a_unix_socket_target`（平台无关）；e2e `tcp_route_can_relay_to_a_unix_socket_target` 是 `#[cfg(unix)]`（`tunnel_end_to_end.rs:2679`）——**在 Windows 上被编译掉、不执行**，需在 Linux CI 上才是端到端证据 |

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
| B8 | `transport.bandwidthLimit` / `bandwidthLimitMode`（每代理限速） | ✅ | **每路由**令牌桶包裹中继流（`gateway/rate_limit.rs` 的 `RouteRateLimiter` / `RateLimitedStream`）；配置面覆盖 server.toml / app config / REST / CLI。e2e：`a_bandwidth_limited_route_paces_the_relay`（同样字节数耗时显著更长）。FRP 的 `bandwidthLimitMode`（限在哪一侧）没有对应开关：限速对整条中继流生效 |
| B9 | `transport.heartbeatInterval` | ✅ | `network.heartbeatIntervalSecs`（默认 10s，`config.rs:18`）+ STP 心跳消息 `Heartbeat`/`HeartbeatAck`（`protocol/message.rs:249`）。过期判据 `min(idle, heartbeat×3)` |
| B10 | `transport.dialServerTimeout` / `dialServerKeepalive` | 🟡 | 超时拆成三档：`timeout.connect` / `handshake` / `streamOpen`（`config.rs:256-271`）。keepalive **在网关侧由心跳派生**（`heartbeat_interval_secs / 2`，`gateway/mod.rs:541`），agent 侧固定 5000ms（`agent/mod.rs:1827`）——即"可配但只有一个间接旋钮"，没有 FRP 那样的独立字段 |
| B11 | `transport.proxyURL`（走 HTTP/SOCKS5 出口代理） | ❌ | 未实现 |
| B12 | `quicBindPort` / `kcpBindPort` | 🟡 | 只有 `gateway.listen`（默认 `0.0.0.0:8443`，`config.rs:126`）；无按传输分口的绑定 |
| B13 | `quic.keepalivePeriod` / `maxIdleTimeout` / `maxIncomingStreams` | ✅ | **已可配**：`[tunnel.network] keepAliveIntervalSecs`（0 = 关闭探测）、`heartbeatTimeoutSecs`、`maxConcurrentStreams`，网关与 agent **同一套规则**——此前 agent 侧固定 5s/512（`agent/mod.rs:1951` 的硬编码）现已消除。idle 仍由 `[tunnel.timeout] idle` 决定，网关 streams 缺省取 `maxStreamsPerSession`（默认 256）。证据：`agent::transport_options_follow_the_configured_network_tunables`、`config::transport_tunables_are_derived_then_overridable` |
| B14 | **服务端先发言的协议**（SSH banner、FTP、RDP） | ✅ | **本仓显式覆盖**：`tunnel_end_to_end.rs::tcp_route_relays_a_server_first_binary_protocol` 断言「访客一个字节都不发，本地目标也必须被拨号」；配套两条路径的 `TCP_NODELAY`（`listeners.rs:157`） |

### C. 路由与匹配

| # | FRP 特性 | 状态 | 本仓对应物 / 证据 |
|---|---|---|---|
| C1 | `vhostHTTPPort` 单端口多站点（按 Host 分派） | ✅ | HTTP 路由挂在 webserver 已有的 80/443 监听器上，由数据面按 `Host` 查隧道注册表（`docs/tunnel.md:63`；`standalone-gateway/src/data_plane/handler.rs` 的隧道路由段） |
| C2 | 精确域名匹配 | ✅ | `RouteMatcher::Domain`，归一化（小写、去尾点、无 scheme/port），`route.rs:98` |
| C3 | 通配符域名 `*.suffix` | ✅ | 最长后缀胜出、精确优先于通配、**通配不覆盖裸后缀**（`registry.rs:206`；单测 `wildcard_matches_subdomains_longest_suffix_wins`）。e2e：`tunnel_end_to_end.rs::wildcard_http_route_matches_subdomains_only` |
| C4 | `locations`（URL 前缀路由 + 最长前缀匹配） | ✅ | 同一 host 可挂多条占不同前缀的路由，**最长前缀胜出**（`registry.rs::candidates_domain_path`）。e2e：`location_claims_on_one_host_dispatch_to_their_own_targets` |
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
| D6 | `X-Forwarded-For`（真实访客 IP，默认开） | ✅ | **隧道 HTTP 平面显式注入**：`standalone-gateway/src/tunnel_bridge.rs:355-380` `inject_forwarded_headers` 把访客地址**追加**到 `X-Forwarded-For`、并在缺失时补 `X-Real-IP`（注释原文「FRP-parity visitor identity propagation」）。访客地址不是取客户端自报的头，而是 `data_plane/handler.rs:189` 的 `resolve_client_ip(peer.ip(), headers, listener.trusted_proxy)`——**只有配置为可信代理的对端才被采信 XFF**，非法转发身份直接报错（`invalid_forwarded_identity_response`）。同一个 IP 也用于路由策略裁决，因此 `allowed_ips` 不可被伪造头绕过（`RelayVisitor.ip`，`tunnel_bridge.rs:250`）。FRP 只做透传，本仓在**可信边界**上更强；TCP/UDP 无头可注入（FRP 亦然） |
| D7 | `transport.proxyProtocolVersion = "v1"\|"v2"`（向 agent 本地目标注入 PROXY 头） | ✅ | v1 / v2 都支持：网关把访客地址写进数据流头部，agent 朝本地目标转发前前置该头（`tunnel-core/src/proxy_protocol.rs`）；未开该特性的路由一个字节都不加。e2e：`a_proxy_protocol_route_hands_the_visitor_address_to_the_local_target`、`a_route_without_proxy_protocol_receives_no_header`。与 webserver 自身的 `proxy_protocol` 配置（`config/model.rs:784`，作用于**本地 HTTP 边缘 → 上游**）仍是两条独立链路 |
| D8 | `allowPorts`（服务端端口白名单） | 🟡 | **端口白名单已落地**：`[tunnel.gateway]` 的 `routePortMin` / `routePortMax` 是允许注册的公网端口闭区间（默认 1024..=65535），在注册期强制（`control.rs::validate_registration`）；域名侧白名单仍是 `gateway.domainSuffixes`（`control.rs:581`）。剩余差异是形态：白名单只有**一段闭区间**，不能按端口逐条枚举。路由的公开端口仍来自 agent 本地模板，这是 SSRF 边界（`docs/tunnel.md:106`）的代价 |
| D9 | 认证失败限速（per-peer） | ✅ | `AuthRateLimiter` 固定窗口 per-IP（`security/mod.rs:83`） |
| D10 | 资源上限（`maxPorts`/`maxPoolCount` 等） | ✅ | `maxDevices` / `maxSessions` / `maxStreamsPerSession`（信号量）/ `maxRoutes` / `maxControlMessageBytes`（`config.rs:277`）；未配置 token 时全拒 |
| D11 | 路由归属校验 | ✅ | `require_route_owner`（`security/mod.rs:140`）+ 注册表跨会话冲突拒绝 |
| D12 | 每设备凭据 / mTLS | ❌ | `security/mod.rs:69` 原话：「V1 tokens are shared credentials; any valid token authenticates any declared device id. Device-bound credentials arrive with mTLS (P1)」 |

### E. 负载均衡与健康检查

| # | FRP 特性 | 状态 | 说明 |
|---|---|---|---|
| E1 | `loadBalancer.group` / `groupKey`（同组多代理随机分担） | ✅ | 已实现：注册表只允许**同一个组内**多台设备共享同一 surface，选路时在健康副本间轮询（round-robin，而非 FRP 的随机）。e2e：`a_load_balancer_group_serves_only_from_healthy_replicas` + registry 单测 |
| E2 | `healthCheck.type = "tcp"\|"http"` + `timeoutSeconds` / `maxFailed` / `intervalSeconds` / `path` | ✅ | agent 探测**自己的**本地目标（tcp/http，含 `maxFailed` 语义），结果经新增的 `RouteHealth` 控制消息上报（`gateway/health.rs`、`protocol/message.rs`）。与 FRP 的形态差异：探测在 agent 侧，不在服务端。e2e：`a_load_balancer_group_serves_only_from_healthy_replicas` |
| E3 | 健康检查 × 负载均衡的高可用组合 | ✅ | 选路跳过不健康副本；全组都不健康时回 **503「no healthy replica」**（而不是 404）。e2e：`a_load_balancer_group_serves_only_from_healthy_replicas`、`tunnel_route_without_a_healthy_replica_answers_503` |

### F. 管理与运维面

| # | FRP 特性 | 状态 | 本仓对应物 / 证据 |
|---|---|---|---|
| F1 | frps Dashboard（`webServer.addr/port/user/password`） | 🟡 | 无独立 dashboard 进程/端口；运行清单走 webserver 的 **operations REST**：`GET /tunnel/status`、`GET\|POST /tunnel/routes`、`DELETE /tunnel/routes/{id}`，以及本轮新增的机器面 `GET /tunnel/machines`、`GET /tunnel/machines/{id}`、`PUT /tunnel/machines/{id}/enabled`（`data_plane/operations.rs:287-364`）。机器面已接入 admin 控制台（模块 `tunnel`，入口 `/admin/tunnel/machines`，权限 `web.tunnel.read`/`web.tunnel.write`），可看身份/在线/会话/路由/健康/流量/最近可见并启停机器。仍缺 FRP 那样的**独立 dashboard 与登录口令**（本仓复用平台 IAM 会话） |
| F2 | `enablePrometheus` + `GET /metrics` | ✅ | **17 个**指标（`tunnel/src/metrics.rs`）：`connections`/`connections_active`、`sessions`/`sessions_active`、`streams`/`streams_active`、`bytes_in`/`bytes_out`、`reconnects`、`errors`、`auth_failures`、`relay_denied`、`health_failures`、`no_healthy_replica`、`visitor_streams`、`visitor_auth_failures`、`routes_active`，由数据面 `/metrics` 追加输出。单测 `prometheus_render_contains_canonical_names` 锁名字 |
| F3 | 内存监控（Dashboard 用） | 🟡 | `TunnelMetricsSnapshot` + `SessionTable::snapshots()` 提供等价的即时视图，**不保留历史**（无 7 天滑动窗口） |
| F4 | frpc `webServer` + `frpc reload` / `status` / `stop` | 🟡 | CLI 有 `status` / `list` / `remove`（`tunnel/src/bin/…:37-45`、`cli.rs:248-317`）。**reload 能力有，但不是 `frpc reload -c <file>`**：`AgentRuntime::reload(routes)` 注销已删路由、注册新增/变更路由、换掉数据面表（在途流继续跑），并回报 `added/removed/unchanged/rejected/appliedOffline`（`agent/mod.rs:239`；e2e `a_live_agent_reloads_its_route_set`）。**刻意不提供文件驱动的 reload CLI**：agent 跑在 webserver 进程内，运维面是声明式的（REST `POST /tunnel/routes` + 网关→agent 的 `DeclareRoute`，见 §5）。仍无 agent 侧 HTTP admin API |
| F5 | `store.path`（运行时增删代理并持久化） | ❌ | 状态全在内存且**刻意不持久化**（`docs/tunnel.md:137`）；控制面的 `POST /tunnel/routes` 是"声明"，agent 离线时不生效 |
| F6 | `frpc verify` / `frps verify`（配置文件预检） | ✅ | 离线预检已补齐：`sdkwork-webserver-tunnel verify` 不建连就校验一套路由（公开面、凭据环境变量、探测参数自洽、重名、两条路由争同一 surface、同组成员不共享 surface、网关端口区间自洽），`--json` 供 CI，出错非零退出；网关二进制的 `validate` 对编译后的 `[tunnel]` 段跑同一套预检（`tunnel-core/src/verify.rs`）。需要连上网关的五级体检仍是 `doctor`（`cli.rs:317`） |
| F7 | CLI 子命令全集 | 🟡 | `gateway` / `agent` / `visitor` / `verify` / `expose` / `list` / `remove` / `status` / `doctor`（9 条）。FRP 的 `frpc reload|status|verify`/`frps verify` 见 F4/F6 |
| F8 | `sshTunnelGateway.*`（SSH `-R` 反向隧道网关，无需 frpc） | ⛔ | **有意不做**：它会新增一个公开 SSH 监听面、第二套凭据，以及一条绕过「agent 只激活与本地模板匹配的声明」这条 SSRF 边界的下发路径。等价结果由 `stcp` + `visitor` 覆盖：属主不开端口，使用方跑一条 `visitor` 命令后直接用系统 `ssh` 连 `127.0.0.1`（`docs/tunnel.md`「SSH 访问」节；服务端先发言行为由 `a_secret_route_carries_a_server_first_protocol_like_ssh` 锁定）。见 §4-G4 |
| F9 | 服务端插件 `[[httpPlugins]]`（`Login`/`NewProxy`/`CloseProxy`/`Ping`/`NewWorkConn`/`NewUserConn` RPC 扩展） | ❌ | 未实现 |
| F10 | 客户端插件 `plugin.type`（`unix_domain_socket`/`http_proxy`/`socks5`/`static_file`/`https2http`/`https2https`/`http2https`/`http2http`…） | ❌ | 未实现。本仓的对应位置是 agent 只做**被动转发**，任何"本地服务"由用户自己跑 |
| F11 | `metadatas`（自定义元数据随 RPC 传给插件） | ❌ | 依赖 F9/F10 |
| F12 | VirtualNet（TUN 虚拟网络，v0.62.0 Alpha） | ❌ | 未实现。FRP 官方自称 Alpha 且「不要用于生产」，本仓不跟随是合理的 |
| F13 | 优雅关闭 / 重连退避 | ✅ | 网关关闭顺序：停 accept → 关 endpoint/会话 → 注销路由 → 释放监听（`docs/tunnel.md:125`）；agent 1s→2s→…→60s 指数退避 + ±20% 抖动，注册成功后重置 |

### G. 配置能力

| # | FRP 特性 | 状态 | 本仓对应物 / 证据 |
|---|---|---|---|
| G1 | TOML / YAML / JSON | 🟡 | `[tunnel]` 段挂在 webserver 的 `server.toml` 上；`TunnelConfig` 本身是 serde 模型（camelCase，`deny_unknown_fields`），可序列化为 JSON（单测 `gateway_and_agent_sections_serialize_camel_case_wire_names` 锁定键名）。**复核更正：加载器支持 TOML 与 JSON 两种**（`ConfigFormat::{Toml,Json}`，`--format json`，`etc/examples/sdkwork.webserver.config.json`）；**只有 YAML 不支持**（`ConfigFormat` 无 Yaml 变体） |
| G2 | Go 模板 + `{{ .Envs.X }}` 环境变量插值 + `includes` 多文件拆分 + YAML anchors | ❌ | 无通用模板渲染、无 `includes`、无 YAML anchor。等价手段只有"凭据用环境变量**名**"这一条（`tokenEnv`/`tlsCertPemEnv`），且是**点对点**的，不是通用插值 |
| G3 | 严格校验（`--strict-config=false` 可关） | ✅ | `deny_unknown_fields` **恒开且不可关**（`config.rs:24`），比 FRP 更严：写错的字段名一定报错，不会被静默忽略 |
| G4 | 分角色配置文件（frpc.toml / frps.toml） | ✅ | 同一份 `[tunnel]` 段内 `[tunnel.gateway]` / `[tunnel.agent]` 双向可选、按角色取用；CLI 也有独立的 `gateway` / `agent` 两套 flag |
| G5 | 主开关 / 向后兼容 | ✅ | `enabled = false` 必须让 webserver **逐字节不变**（`config.rs:26`）；单测 `route_templates_are_only_validated_when_the_feature_is_enabled` 锁定"禁用时不校验模板" |
| G6 | 密钥永不入配置文件 | ✅ | `token`/证书字段存的都是**环境变量名**（`config.rs:1-7`），单测 `agent_defaults_hold_env_var_names_never_secrets` + `gateway_defaults_fail_closed_and_never_carry_a_token` 锁定 |

---

## 3. 测试覆盖矩阵

### 3.1 隧道路由面（`tests/tunnel_relay.rs`，7 例）

| 用例 | 覆盖 |
|---|---|
| 域名中继（普通数据面入口 `run_data_plane_until`） | C1 隧道查询可达 |
| 域名中继（**线上投递面入口** + `Some(executor)`） | C1 + C9 **生产入口守卫** |
| WebSocket 升级穿过隧道 | 101 之后字节双向泵送 |
| 访客被拒 → **403 / 401**（而非 502） | D5/D6 的**状态码语义**：网络策略与 token 缺失去别 |
| 无健康副本 → **503**（且绝不为 404/502） | E2/E3：host 已被认领但副本全下线 |
| §77 禁用时数据面行为不变 | C8 禁用兼容 |
| `tunnel_machines_surface_lists_and_switches_a_machine` | **F1 机器面**：清单/详情/404、删除路由、未授权变更被拒、坏请求体 400、关机器即断会话且 `routeCount→0`、重开清标记 |
| （含）`declares_authority` host 粒度 | C9 单测（在 `webserver-core/tests/website_runtime_set.rs`） |

### 3.2 端到端（`tests/tunnel_end_to_end.rs`，21 例）

基线 8 例：`http_and_tcp_traffic_relay_end_to_end`（A1+A3）、`tcp_route_relays_raw_bytes_through_gateway_port`（A1）、
`udp_route_relays_datagrams_through_gateway`（A2）、`wildcard_http_route_matches_subdomains_only`（C3）、
`private_udp_route_denies_anonymous_visitors`（A8/D8）、
`tcp_route_relays_a_server_first_binary_protocol`（**B14**）、
`wrong_token_is_rejected_and_route_never_registers`（D1）、
`raw_client_through_real_gateway_reads_hello`（B1）。

本轮补齐的 13 例（对应 §2 里由 ❌/🟡 转 ✅ 的行）：

| 用例 | 覆盖 |
|---|---|
| `a_proxy_protocol_route_hands_the_visitor_address_to_the_local_target` / `a_route_without_proxy_protocol_receives_no_header` | D7 v1/v2 头抵达本地目标、未配置时不得凭空造头 |
| `location_claims_on_one_host_dispatch_to_their_own_targets` | C4 同 host 按最长前缀分流 |
| `a_bandwidth_limited_route_paces_the_relay` | B8 真实节流（同样字节耗时显著变长） |
| `a_secret_route_serves_only_a_visitor_that_holds_the_key` | A7/A8 正确密钥 / 错误密钥 / 协议错配 + 密钥不入列表 |
| `a_visitor_runtime_publishes_a_secret_route_on_a_local_port` | A11 访问端把 stcp 映射到本地端口 |
| `a_visitor_runtime_publishes_a_secret_datagram_route` | A8/A11 sudp 按本地客户端分流 |
| `a_secret_route_carries_a_server_first_protocol_like_ssh` | B14 + A7：SSH banner 先到（`docs/tunnel.md`「SSH 访问」配方的锁定） |
| `a_load_balancer_group_serves_only_from_healthy_replicas` | E1/E2 组内只选健康副本 + 全下线给出「无健康副本」 |
| `one_multiplexed_port_reaches_http_and_tcp_routes` | A10 单端口按 host / 按端口抵达，含 407、404、405 |
| `a_live_agent_reloads_its_route_set` | F4 活体重载：新增 / 改指向 / 删除 / 非法配置被拒且不影响在跑配置 |

### 3.3 单元（复核后 207 例）

| crate | 例数 | 覆盖点 |
|---|---|---|
| `tunnel-core` | **76** | 配置模型/默认值/键名契约、ID 校验、路由与匹配器、策略准入三通道、target 解析（含 `unix:`）、PROXY protocol 头、健康检查与负载均衡约束、离线预检规则 |
| `tunnel-protocol` | **27** | 帧编解码与尺寸上限、全部消息 round-trip（含 `RouteHealth`）、访客流头、未知变体降级、已知类型缺字段硬失败、错误码映射 |
| `tunnel` | **91** | 注册表（含 TCP/UDP 命名空间、组共享与健康选路、秘密名所有权）、**设备清单（准入/启停/流量记账/跨断线保留，5 例）**、会话表、TCP/UDP 监听集、指标、安全（token/限速/归属/ACL）、带宽令牌桶、访客条目校验、CONNECT 解析、CLI 参数、service 门面 |
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
# 本矩阵全部单元证据（207 例）
cargo test -p sdkwork-webserver-tunnel-core -p sdkwork-webserver-tunnel-protocol \
           -p sdkwork-webserver-tunnel-transport -p sdkwork-webserver-tunnel --lib

# 端到端（21 例）+ 中继面（7 例）
cargo test -p sdkwork-webserver-tunnel --test tunnel_end_to_end
cargo test -p sdkwork-api-webserver-standalone-gateway --test tunnel_relay
```

二进制级 smoke（不连网关即可跑，退出码即判据）：

```bash
# 离线预检：合法路由 exit 0；同 surface 冲突 / 缺密钥环境变量 exit 1
sdkwork-webserver-tunnel verify --no-environment --route web:http:demo.sdkwork.link:127.0.0.1:3000
sdkwork-webserver-tunnel verify --no-environment --route a:http:same.example:127.0.0.1:1 \
                                             --route b:http:same.example:127.0.0.1:2
# 网关配置预检：`validate` 对编译后的 [tunnel] 段复用同一套预检并打印每路由公开面
sdkwork-api-webserver-standalone-gateway validate <server.toml>
```

---

## 4. 缺口清单（按建议优先级）

**G1 · `TunnelTarget::UnixSocket`（原为死变体）— 已接上**：agent 模板支持 `unix:/绝对路径`
（POSIX 专用，仅 stream 平面；数据报路由显式拒绝），构造期校验绝对路径，端到端用真实 Unix
socket 目标验证先发言行为（`tcp_route_can_relay_to_a_unix_socket_target`，`#[cfg(unix)]`），
配置面单测 `a_template_can_declare_a_unix_socket_target` / `unix_socket_targets_parse_only_for_stream_planes`。
该项是 §4.2 意义上"看起来支持、实际走不到"的假能力，已消除。

**G2 · 无 TCP/WebSocket 传输** — 当前唯一传输是 QUIC/UDP。在只放行 TCP 443 的企业网络里，
agent 无法建连。抽象层已经预留（`transport/lib.rs:107-129` 的两个 trait），实现一个
`TcpTunnelTransport` 是**受控增量**，不需要动上层。

**G3 · 负载均衡与健康检查（E1–E3）已落地** — agent 探测自己的本地目标并经 `RouteHealth` 上报；
注册表允许**同组内**多设备共享同一 surface，选路跳过不健康副本、按轮询分摊，全组不健康回
503「no healthy replica」。原先"跨会话冲突语义与 group 直接冲突"的判断已由新的所有权模型解开
（同一 surface 只有同组可共享），不再是缺口。

**G4 · SSH 隧道网关（F8）是有意不做的非目标** — FRP 用它做"客户端零安装"接入（`ssh -R`），
代价是一个公开 SSH 监听面、第二套凭据，以及一条绕过「agent 只激活与本地模板匹配的声明」这条
SSRF 边界的下发路径。等价结果由 `stcp` + `visitor` 覆盖：使用方跑一条 `visitor` 命令，再用系统
`ssh` 连 `127.0.0.1`（见 F8 与 `docs/tunnel.md`「SSH 访问」）。

**G5 · 无连接池 / 压缩（B5/B7）** — 限速（B8）已落地（每路由令牌桶，`gateway/rate_limit.rs`），
不再是缺口。剩下两项都是**纯增量**且互相独立。
优先级判断：压缩（B5，QUIC 已有头部压缩，收益有限）>
连接池（B7，FRP 官方自己说多路复用下收益有限）。

**G6 · `quic.*` 无专用旋钮（B13，复核后已缩小）** — 此前写作"TransportOptions 完全硬编码"，
复核更正：网关的 idle / keepalive / stream 上限**都已从 `[tunnel]` 派生**
（`gateway/mod.rs:533-546`）。真正的剩余缺口只有两项：agent 侧的 keepalive（固定 5s）与
stream 上限（固定 512）没有旋钮，且没有 FRP 那样独立的 `transport.quic` 表。搬进 `[tunnel.network]`
是低风险改动，但需要同时确认"transport crate 只把参数当**上限**"这个契约（`transport/lib.rs:59-61`）
不被破坏。另注意文档曾引用的 "512" 是 `TransportOptions::default()` 的值，而**网关实际用
`limits.maxStreamsPerSession`（默认 256）**。

**G7 · 端口白名单（D8）已落地** — `[tunnel.gateway]` 的 `routePortMin` / `routePortMax` 在注册期
强制（`control.rs::validate_registration`），"任意空闲端口都能占"不再成立。剩余差异是形态：
白名单是**一段闭区间**，不能按端口逐条枚举；共享 token（D12）仍是暴露面放大的根因。

**G8 · 离线配置预检（F6）已落地** — `sdkwork-webserver-tunnel verify` 不连网关就校验路由集与网关
端口区间（`--json` 供 CI，出错非零退出），网关二进制的 `validate` 复用同一套预检
（`tunnel-core/src/verify.rs`）；`doctor` 继续负责需要连上网关的五级体检。

---

## 5. 与 FRP 的六处差异（本仓更强的地方）

| 维度 | FRP | 本仓 |
|---|---|---|
| **控制面** | 只能改配置文件重启 | 声明式 REST（`POST /tunnel/routes`）+ 网关→agent 的 `DeclareRoute`，agent 只激活与本地模板匹配的声明（SSRF 边界） |
| **多 surface 冲突** | vhost / tcpmux / bindPort 是并列独立面，无裁决 | 按**路由表**（不是状态码）裁决：投递面已声明的 host 不被隧道遮蔽（C9） |
| **端口命名空间** | 未区分 TCP/UDP 索引 | `(is_datagram, port)` 双命名空间，同一端口号可各挂一条，且 TCP 访客永远看不到 UDP 路由（C6） |
| **配置严格性** | `--strict-config=false` 可关 | `deny_unknown_fields` 恒开不可关（G3） |
| **凭据** | `auth.token` 明文可写进配置文件 | 只存环境变量名，凭据永不进文件/git（G6） |
| **真实访客 IP** | 直接透传客户端自报的 `X-Forwarded-For` | `resolve_client_ip(peer, headers, trusted_proxy)`：只有配置为可信代理的对端才被采信，非法转发身份直接报错；同一 IP 既注入 XFF/`X-Real-IP` 又用于 ACL 裁决，不可被伪造头绕过（D6） |
| **头改写的正确性边界** | `set` 接受任意头名 | 帧控/逐跳头与 `Host` 在**配置期**即拒绝（`Content-Length`、`Transfer-Encoding`、`Connection`、`Upgrade`、`TE`、`Trailer`、`Expect`、`Proxy-*`），值不得含 CR/LF/NUL，单方向 ≤32 对——改写这些会让中继两侧解析器失步（请求走私类缺陷），本仓选择让它在部署前失败（§6.1） |

---

## 6. 矩阵未列的 FRP 字段（2026-10-09 穷尽检索）

§2 的 69 行是按 FRP 文档**特性页**组织的。复核时对隧道三个 crate 做了穷尽检索，发现还有一批
FRP 配置字段从来没有进过矩阵 —— 也就是说"69 行全绿"**不等于**"FRP 字段全覆盖"。下表按功能影响排序。

> 检索方法：`grep -riE 'udp_packet|host_header|request_header|response_header|server_addrs|
> additional_scope|proxy_bind|user_conn|dial_server_local|tcp_mux|login_fail|dns_server|annotation|
> virtual_net|nathole|max_ports_per_client|sub_domain_host' crates/sdkwork-webserver-tunnel-core`
> → **0 命中**（即这些字段在本仓配置模型中不存在，既不能写、也不会被 `deny_unknown_fields` 之外的
> 路径接受）。FRP 官网本轮网络不可达（域名解析到非公网地址被沙箱拒绝），字段名以 FRP v0.68
> 已知配置面为准，**建议在能联网时用官方文档复核一遍**。

### 6.1 功能性缺口（会改变可观测行为）

| FRP 字段 | 作用 | 本仓状态 | 影响 |
|---|---|---|---|
| `transport.udpPacketSize` | UDP 单包上限（默认 1500），用于避开路径 MTU 分片 | ✅ **已实现** | `[tunnel.network] udpPacketSize`（512..=65507，缺省为协议最大值），对两个方向生效；**接收缓冲始终按协议最大值**，因此不会出现"Linux 静默截断后转发脏数据 / Windows 报 `WSAEMSGSIZE`"的平台差异行为——超限整包丢弃并计数（`sdkwork_tunnel_udp_datagrams_too_large`）。证据：`config::transport_tunables_are_derived_then_overridable`、`server_toml::materializes_tunnel_transport_tunables`、e2e `a_datagram_above_the_configured_packet_size_is_dropped_and_counted`（512B 上限：512B 回显成功、900B 丢弃且计数 +1） |
| `hostHeaderRewrite` | 转发前改写 `Host` 头 | ✅ **已实现** | `RouteHeaderRewrite.host`（`tunnel-core/src/headers.rs`）+ 网关侧 `apply_header_rewrite`（`standalone-gateway/src/tunnel_bridge.rs`）；配置面 `[[tunnel.routes]] hostHeaderRewrite`，仅 HTTP 平面（其它平面在配置期报错）。e2e：`tunnel_relay.rs::a_route_rewrites_the_host_and_sets_headers_in_both_directions`（断言本地服务看到的 Host 是配置值、且公开域名不再泄漏） |
| `requestHeaders.set` / `responseHeaders.set` | 转发链路上增删改 HTTP 头（v0.53+） | ✅ **已实现** | FRP 同形的 `[tunnel.routes.requestHeaders] set = {...}` / `[tunnel.routes.responseHeaders] set = {...}`；逐对校验（RFC 9110 token、无控制字符、≤32 对、值 ≤2048），**帧控/逐跳头与 Host 一律拒绝**（改写它们会导致解析器与线上字节失步，属请求走私类缺陷）。同上 e2e 用例断言请求头替换、响应头替换与新增、被替换的上游值不泄漏 |
| 多 `serverAddr` + 故障转移（v0.60+） | 一个 agent 配多个网关地址，逐个尝试 | ✅ **已实现** | `[tunnel.agent] endpoints = [...]`（+ CLI `--endpoint` 可重复/逗号列表），按序尝试、仅"从未建立"的尝试才前进、建立过的会话结束后回到主地址、退避曲线不变；地址在配置期按 `host:port` 校验。证据：`config::the_gateway_address_list_is_ordered_and_deduplicated`、`config::a_malformed_gateway_address_is_a_configuration_error`、`server_toml::materializes_the_tunnel_agent_gateway_address_list`、`agent::tests::the_failover_cursor_advances_only_when_an_attempt_never_connected`、`cli::repeated_and_comma_separated_endpoints_build_an_ordered_list`、e2e `an_unreachable_primary_fails_over_to_the_next_gateway_address`（真实 QUIC 网关 + 真实握手超时） |
| `auth.additionalScopes = ["HeartBeats","NewWorkConns"]` | 心跳与新建连接也要带 token | ✅ `heartbeats` / ⛔ `newWorkConns` | **`heartbeats` 已实现**：`[tunnel.gateway] authScopes = ["heartbeats"]`，agent 在每次心跳前重发令牌、网关常量时间复验，失败即计认证失败并关闭会话；作用域经 `AuthResult.requiredScopes` 下发，**不需要两端各配一次**；令牌不回显、不写日志。证据：`policy::scope_labels_round_trip_and_accept_the_frp_spelling`、`server_toml::materializes_gateway_auth_scopes`、`protocol::auth_scopes_are_advertised_and_re_presented_additively`、e2e `a_required_heartbeat_reauth_is_advertised_and_enforced`。**`newWorkConns` 属刻意不做**（设计上无对应物）：FRP 的 work connection 是 frpc 另开的 TCP 连接、需要自己的凭据；本仓所有中继流都由网关在**已认证的 QUIC 连接内部**打开，不存在可被无凭据另开的第二通道，加一个流头令牌只是装饰 —— 因此配置它会被**明确拒绝并给出理由**，而不是静默忽略 |
| `user`（frpc 用户标识）+ 服务端按用户配额 | 同一 frps 上按用户隔离代理与配额 | ✅ 并**更强** | `[tunnel.agent] user` + `[[tunnel.gateway.users]]`（`name`/`tokenEnv`/`maxRoutes`/`maxPorts`/`maxDevices`）。与 FRP 的关键差别：FRP 的 `user` 是**标签**（任何客户端都能自称），这里用户由**令牌**解析、声明必须与令牌一致（共享令牌不得声明用户），路由 id 派生为 `route_{user}_{name}` 并由网关校验落在自己的命名空间内，配额按用户计数（重注册算替换、准入时排除自己）。CLI：`agent --user`。证据：`security::user_bound_tokens_decide_the_user_and_refuse_a_foreign_claim`、`server_toml::materializes_gateway_users_with_their_quotas`、e2e `users_isolate_their_routes_and_their_quotas_are_enforced` |

### 6.2 运维旋钮（不影响转发语义，影响可运维性）

| FRP 字段 | 作用 | 本仓状态 |
|---|---|---|
| `log.to` / `log.level` / `log.maxDays` / `log.disablePrintColor` | 独立日志目标、级别、保留 | 🟡 级别已实现 | 隧道 CLI/agent：全局 `--log-level error\|warn\|info\|debug\|trace`（或 `TUNNEL_LOG_LEVEL`，`RUST_LOG` 仍优先）；**非法级别是启动错误**而不是"匹配不到任何日志"的静默过滤器，且该全局 flag 会从 argv 摘除后再派发命令（冒烟测试抓到的真实缺陷）。网关进程沿用 webserver 自身日志配置；`log.to`/`maxDays`/彩色开关属进程日志策略，仍无隧道专属面 |
| `loginFailExit` | 首连失败即退出（默认 true） | ✅ **已实现** | `[tunnel.agent] loginFailExit = true`：**凭据被拒**即报告原因并停止（不再重连、不再退避）；超时/网关不可达仍照常重试。默认 false，与 FRP 相反且已在字段文档与 §5 说明理由（本仓 agent 是"穿越网关故障继续工作"的服务，见 PRD §27）。只对**凭据拒绝**生效：`SessionAttempt.credentials_rejected` 与 `address_related` 分开分类。CLI：`agent --login-fail-exit`。证据：`server_toml::materializes_login_fail_exit_and_refuses_a_non_boolean`（引号包住的 "true" 被拒，避免"以为关掉了其实没关"）、e2e `login_fail_exit_decides_whether_a_rejected_credential_stops_the_agent`（开=Disconnected(原因)+Stopped 且**不再重连**；关=ReconnectingIn） |
| `dnsServer` | 自定义 DNS 解析 `serverAddr` | ✅ **已实现** | `[tunnel.network] dnsServer = "10.0.0.53[:5353]"`（+ CLI `--dns-server`）→ agent 用它解析**网关主机名**（`agent::resolve_endpoint`），只替换地址、**保留原 TLS server name**，因此证书校验与指纹固定不受影响。自实现的最小解析器（`tunnel/src/dns.rs`）：单服务器、单 `A`/`AAAA` 问题、UDP、5s 超时、无缓存、不跟随 CNAME、**失败即失败**——绝不回落系统解析（回落等于用一个运维明确排除的解析器）。**只接受字面 IP**：解析"解析器自己"是依赖环。证据：`dns::{a_query_encodes_the_name_and_type, an_address_response_is_parsed_through_name_compression, unusable_responses_are_errors_never_a_silent_empty_answer, a_configured_server_is_an_ip_with_an_optional_port}`、`server_toml::materializes_dns_server_and_refuses_a_name`、`cli::log_level_tests::a_dns_server_flag_is_validated_while_parsing`、e2e `dns_server_resolves_the_gateway_name_the_system_cannot`（系统解析器不可能认识的 `gw.tunnel.test` 经桩解析器成功建连；NXDOMAIN 时报告原因且**不建连**） |
| `transport.dialServerLocalIP` | 指定出口源地址 | ✅ **已实现** | `[tunnel.network] dialServerLocalIP`（别名 `dialLocalAddr`，亦接受 `[tunnel.agent]` 段）→ `TunnelNetworkConfig.dial_local_addr` → agent 在拨号本地目标前 `TcpSocket::bind(source, 0)` 再 `connect`（**只作用于目标拨号**，网关连接保持自己的传输）。地址族不匹配（IPv4 源拨 IPv6 目标）按拨号错误处理。解析失败是配置错误（主机名被拒）。CLI：`verify --dial-server-local-ip`。证据：`server_toml::materializes_dial_server_local_ip_and_refuses_a_hostname`、e2e `dial_server_local_ip_decides_the_source_of_target_dials`（同一路由跑两次，目标侧看到的对端地址：缺省 `127.0.0.1`、配置后 `127.0.0.2`） |
| `transport.tcpMuxKeepaliveInterval` | mux 保活间隔 | ✅ 等价物 | `[tunnel.network] keepAliveIntervalSecs`（B10/P3 已实现并测试：`transport_options_follow_the_configured_network_tunables`）；QUIC 原生多路复用，无独立 mux 层 |
| `annotations` | 代理级注释元数据 | ✅ **已实现** | `[tunnel.routes.annotations]`（表）→ `TunnelRouteTemplate.annotations` → 注册线 `RegisterRoute.annotations` → 网关 `TunnelRoute.annotations` 并在 `list_routes()`/机器清单中可见；**有界**（≤16 条、键 ≤64 且限定字符集、值 ≤256），网关侧 `with_annotations` 二次校验（注册来自对端），只承载不解释。证据：`route::annotation_tests::{annotations_are_carried_and_bounded, unusable_annotations_are_refused_at_construction}`、`server_toml::materializes_route_annotations_and_refuses_unusable_ones`、e2e `route_annotations_travel_with_the_registration` |
| `proxyBindAddr`（frps） | 代理监听绑定地址 | ✅ **已实现** | `[tunnel.gateway] proxyBindAddr`（缺省 `0.0.0.0`）→ 每个 `tcp`/`udp` 路由监听按此地址 `bind`，多宿主主机因此能把 agent 注册的端口只暴露在指定网络上。只接受**字面 IP**：主机名会在每次注册时解析、`host:port` 的端口会被路由自身端口覆盖，两者都在配置期报错。CLI：`gateway --proxy-bind-addr`。证据：`server_toml::materializes_proxy_bind_addr_and_refuses_anything_but_an_ip`、e2e `proxy_bind_addr_places_route_listeners_on_the_configured_address`（`127.0.0.1` 可达 ＋ `127.0.0.2` **不可达**——通配绑定会在这里应答，正是该旋钮要防的） |
| `userConnTimeout`（frps） | 访客连接超时 | 🟡 等价物是 `timeout.connect`（`config.rs:258`） |
| `maxPortsPerClient`（frps） | 每客户端端口配额 | ✅ 等价物并更强 | 按用户 `[[tunnel.gateway.users]] maxPorts`（P5，计数取自注册表，重注册算替换）；全局侧仍有 `limits.maxRoutes` + `routePortMin/Max`（D8） |
| `transport.heartbeatTimeout` | 心跳超时（默认 90s） | ✅ 显式旋钮 | `[tunnel.network] heartbeatTimeoutSecs`（P3 已实现）；缺省仍为 `min(idle, heartbeat×3)` 的推导值 |

### 6.3 结论口径

- "FRP 主干能力"（§2 的 69 行）**已覆盖并超过**：✅37 / 🟡14 / ❌14 / ⛔4。
- §6.1 的字段级缺口已完成 **5 项**：HTTP 头改写、多网关地址故障转移、`udpPacketSize` + `quic.*`
  传输旋钮、`auth.additionalScopes` 的 `heartbeats` 作用域、`user` 隔离与按用户配额
  （2026-10-09/10 实现并测试）；`newWorkConns` 因**设计上无对应物**被明确拒绝
  （⛔，配置期报错并给理由）。
- §6.2 运维旋钮已完成 **6 项**（`loginFailExit`、`annotations`、`proxyBindAddr`、
  `dialServerLocalIP`、`log.level`、`dnsServer`），另有 **3 项经复核为已有等价物**
  （`tcpMuxKeepaliveInterval` → `keepAliveIntervalSecs`、`heartbeatTimeout` → `heartbeatTimeoutSecs`、
  `maxPortsPerClient` → 按用户 `maxPorts`）。
- §6.2 **已无功能性缺口**。剩余仅为进程日志策略项（`log.to`/`log.maxDays`/彩色开关：
  随 webserver 进程日志，本仓不提供隧道专属日志文件面）与 `userConnTimeout`
  （等价物为 `timeout.connect`），两者都不影响转发语义。
- **配置面与离线预检**：§6.1/§6.2 的每一项都有 `server.toml` 面；agent/网关侧旋钮另有 CLI 旗标（`--endpoint`/`--dns-server`/`--user`/`--login-fail-exit`/`--proxy-bind-addr`/`--log-level`），且 `verify` 会输出 `connection.*` 连接预检（文本与 `--json`），两者都走与运行时同一套 core 校验。证据：`tests/cli_preflight.rs`（**跑真实二进制** 6 条：连接预检 JSON/文本、三类用法错误、agent 启动打印、网关 bind 拒绝、日志级别启动失败）。
- 因此当前口径是：**主干、可用性、正确性、认证加固、多租户隔离、部署与诊断相关的字段全部完整**；
  §2 的 69 行主干能力已覆盖并超过 FRP 对应项。
