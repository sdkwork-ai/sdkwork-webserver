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

### SSH 访问（两种方式，都不需要把 22 端口开到公网）

**私密访问（推荐；等价于 `ssh -R` 的效果，但没有公网面）** —— 属主用 `stcp` 密钥路由暴露，使用方一条 `visitor` 命令把远端服务映射到本地回环端口，然后照常用标准 `ssh` 客户端：

```bash
# 属主侧：本机 sshd 只对持有共享密钥的人可见；不监听任何公网端口
export SDKWORK_TUNNEL_SECRET_KEY=<32+ 字符随机串>
sdkwork-webserver-tunnel agent --endpoint <gw>:8443 --pin <fingerprint> \
  --route private-ssh:stcp:-:127.0.0.1:22

# 使用方侧：一条命令，之后就是普通 ssh
export SDKWORK_TUNNEL_SECRET_KEY=<同一个串>
sdkwork-webserver-tunnel visitor --endpoint <gw>:8443 --pin <fingerprint> \
  --visitor private-ssh:2222
ssh -p 2222 <user>@127.0.0.1
```

密钥只从环境变量读（`--secret-key-env` 可换变量名，命令行不接受密钥），网关侧以常量时间比较校验；路由名与密钥不对就得到与"路由不存在"完全相同的拒绝，因此该端口不会变成路由名探测器。访问端默认只绑回环，要暴露到局域网必须显式写 `<name>:<port>:<bind-addr>`。

**公网端口（`tcp` 路由）** —— 需要任何人可达时使用，端口受网关 `routePortMin/Max` 白名单约束：

```bash
sdkwork-webserver-tunnel agent --endpoint <gw>:8443 --route ssh:tcp:7022:127.0.0.1:22
ssh -p 7022 <gw-host>
```

**只要一个端口可用时的第三种走法**：网关开启多路复用入口（`[tunnel.gateway] tcpmux_port = 8444`）后，一个端口即可按 HTTP `CONNECT` 抵达所有路由，包括上面这条 `tcp` 路由：`ssh -o ProxyCommand="..."` 或任意支持 HTTP CONNECT 的客户端 `CONNECT <gw-host>:7022`。该入口的 407 质询对应路由的访客 token（`Proxy-Authorization: Bearer <token>`）。

> **关于 FRP 的 `sshTunnelGateway`（`ssh -R` 免安装接入）**：本仓**不提供 SSH 服务端**，这是有意的产品/安全取舍，不是待补的缺陷。`ssh -R` 的价值是"客户端零安装"，代价是新增一个公网 SSH 监听面、一套 SSH 凭据，以及一条**绕过"agent 只激活与本地模板匹配的声明"这条 SSRF 边界**的远端建路通道；而它的实际效果（把私网服务安全地给到指定的人）已由上面的 `stcp` + `visitor` 组合覆盖：属主不开放任何端口，使用方的 `ssh` 客户端保持原样，只有网关侧多一条 `visitor` 命令。服务端先发言（SSH banner）这一关键行为有专门用例锁定（`tunnel_end_to_end.rs::a_secret_route_carries_a_server_first_protocol_like_ssh`）。

### 离线预检（不连网关，`frpc verify` 对等）

```bash
sdkwork-webserver-tunnel verify --no-environment \
  --route web:http:demo.sdkwork.link:127.0.0.1:3000 \
  --route private-ssh:stcp:-:127.0.0.1:22 --secret-key-env SDKWORK_TUNNEL_SECRET_KEY
# 每条路由打印它对外的公开面 + 逐条 info/warning/error；有 error 时退出码非 0
# --json 供 CI 门禁消费；--private/--allowed-ip/--visitor-token 与 agent 同义
```

服务端侧对应 `sdkwork-webserver-standalone-gateway validate <config>`：编译通过后对 `[tunnel]` 段跑同一套语义预检，错误直接让 `validate` 失败。预检覆盖：密钥变量是否存在与长度、同一公开面被两条路由争抢（未共享 `loadBalancerGroup`）、同组副本却不共享公开面、组员没有 `healthCheck`（告警）、`tcpmuxPort` 落在可注册端口段内（告警）、`routePortMin > routePortMax`、重名路由等。

### 网关随 WebServer 数据面运行（原生集成）

在 `server.toml` / app 配置中启用：

```toml
[tunnel]
enabled = true
[tunnel.gateway]
listen = "0.0.0.0:8443"
domain_suffixes = ["sdkwork.link"]
# agent_token_env 默认 ["SDKWORK_TUNNEL_GATEWAY_TOKEN"]；token 永不入配置文件
# tcp/udp 路由可注册的公网端口闭区间（默认 1024..=65535）：网关自身监听与
# 特权端口默认对 agent 不可达，确需放开时显式放宽
route_port_min = 1024
route_port_max = 65535
# 可选：单端口多路复用入口（HTTP CONNECT，见 Quick Start「SSH 访问」）。
# 不配置即不监听该端口。
tcpmux_port = 8444
[[tunnel.routes]]        # agent 侧模板；gateway 配置里仅为声明
name = "web"
protocol = "http"
domain = "demo.sdkwork.link"
target = "127.0.0.1:3000"
# 可选：路径前缀声明（FRP locations）、同组多副本 + 健康探测、限速、PROXY protocol
# locations = ["/api"]
# load_balancer_group = "web"
# bandwidth_limit = "2MB"
# proxy_protocol = "v2"
# [tunnel.routes.health_check]      # kind = "http" | "tcp"
# kind = "http"
# path = "/health"
# interval_seconds = 5
# timeout_seconds = 2
# max_failed = 3
# 密钥路由（stcp/sudp）：只写环境变量名，密钥永不入配置文件
# [[tunnel.routes]]
# name = "private-ssh"
# protocol = "stcp"
# target = "127.0.0.1:22"
# secret_key_env = "SDKWORK_TUNNEL_SECRET_KEY"
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
sdkwork-webserver-tunnel-core        领域模型：Device/Session/Route/Matcher/Policy/ID/Config/Error、
                                     PROXY protocol 头、健康检查、离线预检规则（serde-only）
        ↑
sdkwork-webserver-tunnel-protocol    STP/1 控制面消息（含 RouteHealth）、数据面流头、访客流头、
                                     长度前缀帧与数据报帧编解码（尺寸上限强制）
        ↑
sdkwork-webserver-tunnel-transport   Transport 抽象 trait + QUIC（quinn + rustls TLS 1.3）实现 + TLS 材料
        ↑
sdkwork-webserver-tunnel             组合层：Gateway（监听/控制循环/注册表+健康与负载均衡/会话表/
                                     TCP/UDP 监听/HTTP CONNECT 多路复用入口/分发/限速/访客流桥接）、
                                     Agent（连接/认证/注册/心跳/重连/热重载/本地转发/健康探针）、
                                     Visitor（密钥路由消费端）、Security、Service 门面、CLI
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
- 三种准入手段并非三面通用：**Bearer 只能随 HTTP `Authorization` 头到达**，裸 TCP/UDP 连接没有该信道。因此 "Bearer 保护 + TCP/UDP" 的组合在**注册期即被显式拒绝**（`AuthPolicy::is_satisfiable_on`，报文 `"Tcp routes cannot require bearer visitor authentication; use allowed_ips to restrict access"`），而不是注册成功后静默变成不可达路由；私有 TCP/UDP 路由以 allowedIps 白名单为主要准入手段。裸 UDP 数据报同样拒收匿名访客（FRP SUDP 语义，需访客客户端）。HTTP CONNECT 多路复用入口把同一策略映射到 `Proxy-Authorization: Bearer`，token 路由缺 token 时回 **407 + 质询**（唯一能向 CONNECT 客户端索要凭据的状态码），网络策略拒绝回 403。
- 密钥路由（`stcp`/`sudp`）的准入是**共享密钥**而不是策略：常量时间比较，路由名不存在与密钥错误返回同一种拒绝（该端点不得成为路由名探测器）；密钥 `#[serde(skip)]`，永不出现在路由列表、状态接口或日志中，只经加密控制面到达网关。
- 多路复用入口与专用监听器共用同一份策略实现（`admit()`）、同一份注册表、同一份负载均衡与健康判定——第二个入口不允许是策略的第二套实现。
- 资源上限：maxDevices / maxSessions / maxStreamsPerSession（信号量预算）/ maxRoutes / maxControlMessageBytes；同设备重复连接替换旧会话。
- 心跳（默认 10s）× 3 + idle 上限驱动过期会话清理；会话销毁级联注销路由并释放 TCP/UDP 监听端口。

## 可观测性（PRD §48–§49, §82）

- Prometheus 指标（数据面 operations `/metrics` 追加）：`sdkwork_tunnel_connections[_active]`、`sessions[_active]`、`streams[_active]`、`bytes_in/out`、`reconnects`、`errors`、`auth_failures`、`relay_denied`、`health_failures`、`no_healthy_replica`、`visitor_streams`、`visitor_auth_failures`、`routes_active`。`status.ports` 聚合 TCP 与 UDP 监听端口，`status.multiplexerPort` 上报多路复用入口**实际绑定**的端口。
- operations REST（默认回环）：`GET /tunnel/status`、`GET|POST /tunnel/routes`、`DELETE /tunnel/routes/{id}`。读取在未配置令牌时仅限回环；`POST/DELETE` 一律要求 `SDKWORK_WEBSERVER_OPERATIONS_AUTH_TOKEN_FILE` 配置的 Bearer 令牌（未配置时 403 fail-closed）。`POST /tunnel/routes` 支持 `http`/`tcp`/`udp`/`stcp`/`sudp`，可带 `allowedIps`/`auth`/`visitorTokens`/`proxyProtocol`/`locations`/`bandwidthLimit`/`loadBalancerGroup`/`healthCheck`。
- **机器（connector 节点）管理**：`GET /tunnel/machines`、`GET /tunnel/machines/{id}`、`PUT /tunnel/machines/{id}/enabled`（`{"enabled":false}`）。机器清单**跨断线保留**：身份、平台、在线状态与当前会话、首次/最近可见、连接与断开时刻、对端地址、入/出流量累计、会话次数、当前路由（协议/域名或端口或密钥名/locations/组/限速/PROXY protocol/准入策略/头改写）与健康路由计数；每条机器的公开面为「无」（`stcp`/`sudp`）或域名/端口。开关**立即生效**：关闭会终止该机器的在线会话并拒绝其重连，重新开启后即可再次注册。读按读策略、开关按变更策略（未配置令牌时 fail-closed）。

### 多网关地址与故障转移（FRP 有序 `serverAddr` 列表）

一个 agent 可以配置**多个网关地址**，按顺序尝试；主地址连不上时自动切到下一个，而不是一直重试同一个死端口：

```toml
[tunnel.agent]
endpoint = "gw1.sdkwork.link:8443"                                  # 主地址
endpoints = ["gw2.sdkwork.link:8443", "gw3.sdkwork.link:8443"]      # 故障转移候选（按序）
```

命令行等价写法（`--endpoint` 可重复，也接受逗号列表）：

```bash
sdkwork-webserver-tunnel agent --endpoint gw1.sdkwork.link:8443 --endpoint gw2.sdkwork.link:8443 \
                               --route web:http:demo.sdkwork.link:127.0.0.1:3000
sdkwork-webserver-tunnel agent --endpoint "gw1.sdkwork.link:8443,gw2.sdkwork.link:8443" --route ...
```

语义（两条规则，都有测试锁定）：

- **只有"与地址有关"的失败才切换**：拨号/握手失败 → 游标前进到下一个地址（走完一圈回到开头）；会话一旦建立过，之后无论因何种原因断开（网关重启、网络抖动、排空下线），**都回到主地址**。这样网关重启后 agent 会自己回来，而不会被永久"迁移"到备用地址。
- **本地配置错误不切换地址**：TLS 材料缺失、传输参数不可用这类失败对**每个**地址都一样，走一遍备用只会增加噪声、并拖慢运维定位那唯一需要修的错——因此游标停在原地（二进制级 smoke 可见：配错 TLS 时始终停在 `gw1`）。
- **退避曲线不受影响**：切换地址仍走原来的 1s→2s→…→60s 指数退避 + 抖动，成功建立会话后才重置——避免网关整体故障时变成全舰队 ~1Hz 重连风暴（PRD §27）。
- **地址写错在部署前就失败**：每个地址都按 `host:port` 校验（拒绝 scheme、路径、空白、端口 0、非数字端口），配置文件里报 `server.toml.tunnel.agent.endpoints`，命令行直接拒绝。**一个连不上的"备用地址"比没有备用更糟**，因为它让运维以为故障转移已配置。
- **优先级**：命令行 `--endpoint` 优先于环境变量 `SDKWORK_TUNNEL_ENDPOINT`；两者合并去重后，第一个是主地址。
- 单地址配置行为与之前**逐字节一致**（列表长度为 1 时游标恒为 0）。

启动时会打印实际解析出的地址顺序（`gateway addresses: a -> b -> c`），`doctor` 只诊断主地址（它回答的是"这台机器能不能到那个网关"，静默走备用会掩盖主地址故障）。

### 自定义 DNS（FRP `dnsServer`）

```toml
[tunnel.network]
dnsServer = "10.0.0.53"        # 或 "10.0.0.53:5353"；缺省用系统解析
```

```bash
sdkwork-webserver-tunnel agent --endpoint gw.internal:8443 --dns-server 10.0.0.53 ...
```

- **为什么需要**：分域（split-horizon）或离线部署把网关名字发布在**自己的**解析器里，agent 主机的系统解析器可能根本不认识它。没有这个开关，运维只能把 IP 写进每台 agent 的配置——网关一搬家就全断。
- **只用这一个服务器**：配置了就**不回落到系统解析**。回落等于用一个运维刚刚明确排除的解析器去解析，正是这个设置要消除的歧义；解析失败按"地址相关问题"处理，于是多地址故障转移可以换下一个网关。
- **只替换地址、保留名字**：TLS 的 server name 仍是运维写的那个名字，因此证书校验与指纹固定完全不受影响。
- **只接受字面 IP**（可带端口）：解析"解析器自己"是依赖环。
- 实现范围（`tunnel/src/dns.rs`）：单服务器、单 `A`/`AAAA` 问题、UDP、5s 超时、**无缓存**、不跟随 CNAME 链、响应必须匹配事务 id 且 `rcode` 为 0；任何异常都是错误，绝不当成"空答案"。
- 证据：`dns::a_query_encodes_the_name_and_type`、`dns::an_address_response_is_parsed_through_name_compression`（含压缩指针）、`dns::unusable_responses_are_errors_never_a_silent_empty_answer`（错 id／NXDOMAIN／rcode 2／截断／非响应）、`dns::a_configured_server_is_an_ip_with_an_optional_port`、`server_toml::materializes_dns_server_and_refuses_a_name`、`cli::log_level_tests::a_dns_server_flag_is_validated_while_parsing`、e2e `dns_server_resolves_the_gateway_name_the_system_cannot`。

### 监听与出口地址（FRP `proxyBindAddr` / `transport.dialServerLocalIP`）

```toml
[tunnel.gateway]
proxyBindAddr = "127.0.0.1"        # 每个 tcp/udp 路由监听绑定的地址（缺省 0.0.0.0）

[tunnel.network]
dialServerLocalIP = "10.0.0.5"     # agent 拨号本地目标时的出口源地址（缺省由内核选择）
```

- **`proxyBindAddr`（多宿主网关）**：决定 agent 注册的端口出现在哪张网卡上。缺省 `0.0.0.0`（所有接口，即原有行为）；公网＋内网管理两张网卡的主机用它把端口只放在指定网络上。**只接受字面 IP**：主机名会在每次注册时解析（同一网关上两条路由还可能解析出不同结果），`host:port` 的端口会被路由自身端口覆盖，两者都在配置期报错。
- **`dialServerLocalIP`（多宿主 agent）**：只作用于**拨号本地目标**（网关连接保持自己的传输）。服务或它前面的 ACL 常按源地址判断，把选择权交给内核"通常对、偶尔恰好错"；绑定后再连是唯一的指定方式。地址族不匹配（IPv4 源拨 IPv6 目标）按拨号错误处理，不会静默退回内核选择。
- 证据：`server_toml::materializes_proxy_bind_addr_and_refuses_anything_but_an_ip`、`server_toml::materializes_dial_server_local_ip_and_refuses_a_hostname`、e2e `proxy_bind_addr_places_route_listeners_on_the_configured_address`（可达 127.0.0.1 ＋ **不可达 127.0.0.2**）、e2e `dial_server_local_ip_decides_the_source_of_target_dials`（目标侧看到的源地址 127.0.0.1 → 127.0.0.2）。

### 日志级别（FRP `log.level`）

```bash
sdkwork-webserver-tunnel --log-level debug agent --endpoint gw1.sdkwork.link:8443
# 或 TUNNEL_LOG_LEVEL=debug；RUST_LOG 仍然优先
```

- 级别取 `error|warn|info|debug|trace`（`warning` 视为 `warn`），**默认 `info`**。
- **非法级别是启动错误**并列出可选值：交给过滤器一个拼错的级别只会得到一个"匹配不到任何东西"的过滤器，表现与"agent 什么都没做"一模一样。
- 该 flag 是全局的，会在派发命令前**从 argv 摘除**——否则命令派发器会把它当成命令名（`unknown command '--log-level'`）。这个缺陷是二进制的冒烟测试抓到的，已有回归测试 `cli::log_level_tests::the_flag_is_removed_from_the_command_line`。
- 网关进程沿用 webserver 自身的日志配置（`RUST_LOG`）；`log.to`/`log.maxDays` 属进程日志策略，本仓没有隧道专属日志文件面。

### 重连策略与凭据拒绝（FRP `loginFailExit`）

```toml
[tunnel.agent]
endpoint = "gw1.sdkwork.link:8443"
loginFailExit = true      # 凭据被拒即停止（缺省 false：继续按退避重试）
```

- **区分两类失败**：`address_related`（地址可能是问题 → 故障转移）与 `credentials_rejected`（网关明确拒绝凭据 → 换地址无用）。超时、网关不可达属前者，凭据被拒属后者。
- `loginFailExit = true`：收到凭据拒绝时先发 `Disconnected { reason }` 告知原因，再 `Stopped` 并退出，**不再重连**——把"运维配错了令牌"从一堆重连日志里提出来。
- 缺省 `false`，**与 FRP 相反**：本仓 agent 是"穿越网关故障继续工作"的服务（PRD §27），默认退出会让一次网关抖动变成整个机队下线；需要 FRP 行为时显式打开。
- `loginFailExit` 非布尔值（例如 `"true"` 字符串）是**配置错误**：静默按字符串为真会让人以为关掉了其实没关。
- 证据：`server_toml::materializes_login_fail_exit_and_refuses_a_non_boolean`、e2e `login_fail_exit_decides_whether_a_rejected_credential_stops_the_agent`。

### 路由注释元数据（FRP `annotations`）

```toml
[[tunnel.routes]]
name = "web"
protocol = "http"
domain = "docs.sdkwork.link"
target = "127.0.0.1:8080"

[tunnel.routes.annotations]
owner = "team-a"
ticket = "OPS-1"
```

- **只承载、不解释**：网关存储并在 `list_routes()` / 机器清单里展示，供控制台按团队、工单、环境分组筛选，不需要为每个标签改 schema；**不参与路由匹配，也不参与任何授权判断**。
- **有界**：最多 16 条、键 ≤64 字符（限定 `[A-Za-z0-9_.\-/]`）、值 ≤256 字符；agent 侧在构造路由时校验，网关侧 `with_annotations` **再校验一次**（注册来自对端，未加界的 map 等于给任何已认证 agent 一份免费存储）。
- 缺省不发（`skip_serializing_if`），老 agent 不受影响。
- 证据：`route::annotation_tests::{annotations_are_carried_and_bounded, unusable_annotations_are_refused_at_construction}`、`server_toml::materializes_route_annotations_and_refuses_unusable_ones`、e2e `route_annotations_travel_with_the_registration`。

### 团队（用户）隔离与配额

```toml
[tunnel.agent]
endpoint = "gw1.sdkwork.link:8443"
user = "team-a"                       # 本机服务哪个团队（可省略）

[[tunnel.gateway.users]]              # 每个团队自己的令牌与上限
name = "team-a"
tokenEnv = "TEAM_A_TOKEN"
maxRoutes = 20
maxPorts = 5
maxDevices = 3
```

- **用户来自令牌，不来自声明**。FRP 的 `user` 只是标签：任何客户端都能自称任意名字，所以它能分组，**不能**约束某团队在共享网关上消耗多少。这里每个用户有自己的令牌（`tokenEnv`，只写变量名，不写令牌），会话的用户由**令牌**解析；agent 可以在 `Hello.user` 里声明归属，但网关会拿令牌解析出的用户比对，**不一致即拒**。共享令牌**不允许**声明任何用户——否则一台机器就能花掉别的团队配额、并出现在别人的清单里。
- **路由 id 按用户加命名空间**。路由 id 与端口/域名不同，本身是全局唯一的，所以两个团队各自有一个叫 `web` 的路由时会撞车。现在 id 派生为 `route_{user}_{name}`（如 `route_team-a_web`）：agent 与网关用**同一个派生函数**，网关还会校验注册里的 id 是否落在该令牌所属用户的命名空间内——机器无法注册到别人的 id 空间里去占位或消耗其配额。用户名因此限制为 `[A-Za-z0-9_-]`，与路由 id 字符集一致，派生结果无歧义。
- **配额按用户计数**：`maxRoutes`（在线路由数）、`maxPorts`（占用的公网端口数）、`maxDevices`（并发在线机器数）。计数直接从注册表/会话表按用户过滤得到，不维护第二份索引；**重注册同一路由算替换而非新增**；准入机器时会把自己排除在计数外（否则滚动重启会被自己的上一个会话挡住）。未在用户表声明的用户没有按用户配额，退回全局上限（即原有行为）。
- **可见性**：路由与机器都带 `user`，网关赋值，管理面清单据此显示归属。
- 证据：`security::user_bound_tokens_decide_the_user_and_refuse_a_foreign_claim`、`security::authenticator_verifies_tokens`（共享令牌不得声明用户）、`server_toml::materializes_gateway_users_with_their_quotas`、`server_toml::an_unenforceable_user_entry_is_a_configuration_error`、`server_toml::materializes_the_agent_user_claim`、e2e `users_isolate_their_routes_and_their_quotas_are_enforced`（team-a 第二条路由被 `maxRoutes` 拒、team-b 独立预算下同名模板 `web` 正常注册且 id 落在自己命名空间、冒名者被拒）。

### 令牌复验（FRP `auth.additionalScopes`）

```toml
[tunnel.gateway]
authScopes = ["heartbeats"]   # 每次心跳前必须重新出示令牌；缺省为空（不启用）
```

- **作用**：会话只在握手时认证一次，此后连接内的一切都被隐式信任。开启 `heartbeats` 后，agent 在**每次心跳之前**重发一次令牌，网关常量时间比对；不匹配或未出示 → 计一次认证失败、回 `AuthenticationFailed` 并关闭会话。这样"凭据轮换后老会话仍被无限续命"的窗口被关掉，也让被接管的会话在下一个心跳周期内失效。
- **网关告知、agent 遵从**：所需作用域随 `AuthResult.requiredScopes` 下发给 agent，**不需要两端各配一次**——避免"只有一边开了加固"把全部 agent 锁在门外。老 agent（不认识该字段）在开启后会被拒绝，所以默认不启用，且启用属于**显式的顺序升级**。
- **令牌只出现在控制流里**：不写日志、不回显、不进错误文本（e2e 断言拒绝报文里不含令牌）。
- **`newWorkConns` 在配置期被拒绝**，理由是它在本仓**没有对应物**：FRP 的 work connection 是 frpc 另开的一条 TCP 连接，需要自己的凭据；本仓所有中继流都是网关在**已认证的 QUIC 连接内部**打开的，不存在"攻击者可以无凭据另开"的第二条通道。给它塞一个流头令牌只会看起来像防护。这条拒绝信息写在错误里（`auth scope 'newWorkConns' has no equivalent here …`），而不是静默忽略。
- 证据：`policy::scope_labels_round_trip_and_accept_the_frp_spelling`、`policy::new_work_conns_is_refused_with_an_explanation`、`policy::scope_lists_reject_duplicates_and_sort`、`server_toml::materializes_gateway_auth_scopes`、`server_toml::new_work_conns_is_a_configuration_error_not_a_silent_no_op`、`protocol::auth_scopes_are_advertised_and_re_presented_additively`、e2e `a_required_heartbeat_reauth_is_advertised_and_enforced`（真实 agent 跨 3 个心跳周期存活；裸客户端未复验即被拒且拒绝报文不含令牌）。

### 命令行面（CLI）与离线预检

命令行的旗标与 `server.toml` 是**同一套校验**（都走 core 模型），所以"命令行接受、运行时拒绝"不会发生。

| 旋钮 | CLI | 说明 |
|---|---|---|
| 多网关地址 | `agent --endpoint a [--endpoint b]`（也接受逗号分隔） | 顺序即故障转移顺序，启动时打印 `gateway addresses: a -> b` |
| 自定义 DNS | `agent --dns-server 10.0.0.53[:5353]` | 只接受字面 IP；启动时打印 |
| 出口源地址 | `agent` 由 `verify --dial-server-local-ip` 预检（运行时经 `[tunnel.network]`） | 见上表 |
| 用户归属 | `agent --user team-a` | 名字在解析期校验；启动时打印 `user claim: …` |
| 凭据拒绝即停 | `agent --login-fail-exit` | 启动时打印 `loginFailExit: enabled` |
| 代理监听地址 | `gateway --proxy-bind-addr 127.0.0.1` | 只接受字面 IP（主机名/`host:port` 均报错） |
| 日志级别 | 全局 `--log-level error\|warn\|info\|debug\|trace` | 或 `TUNNEL_LOG_LEVEL`；`RUST_LOG` 优先 |

**离线预检**：`verify` 现在同时回答"路由对不对"和"连得上、以谁的身份连"两个问题：

```bash
sdkwork-webserver-tunnel verify \
  --route web:http:docs.sdkwork.link:127.0.0.1:8080 \
  --endpoint gw1.sdkwork.link:8443 --endpoint gw2.sdkwork.link:8443 \
  --dns-server 10.0.0.53 --dial-server-local-ip 10.0.0.5 \
  --user team-a --login-fail-exit --json
```

```
info    connection.endpoints     gw1.sdkwork.link:8443 -> gw2.sdkwork.link:8443
info    connection.dnsServer     10.0.0.53
info    connection.dialServerLocalIP 10.0.0.5
info    connection.user          team-a
info    connection.loginFailExit a rejected token stops the agent
ok: 0 error(s), 0 warning(s)
```

`--json` 在原有 `findings` 之外多一个 `connection` 对象（`endpoints`／`dnsServer`／`dialServerLocalIP`／`user`／`loginFailExit`），控制台可直接渲染。三项都**在预检期就用运行时同一套模型校验**：非字面 IP 的解析器、非字面 IP 的出口地址、非法用户名都是用法错误（退出码 1 并指名旗标），不会拖到第一次重连才暴露。

证据：`tests/cli_preflight.rs`（**跑真实二进制**，6 条）——连接预检的 JSON 与文本形态、三类用法错误的拒绝、agent 启动打印、网关 `--proxy-bind-addr` 的两种拒绝、日志级别的启动失败。

## 传输与 UDP 可调参数（FRP `quic.*` / `transport.heartbeatTimeout` / `transport.udpPacketSize`）

```toml
[tunnel.network]
heartbeat_interval = 10           # 心跳节奏（秒）
keepAliveIntervalSecs = 2         # QUIC 保活探测间隔；0 = 关闭探测；缺省 = 心跳/2
heartbeatTimeoutSecs = 45         # 会话静默多久判为失效；缺省 = min(idle, 心跳x3)
udp_packet_size = 1200            # UDP 单包上限（字节，512..=65507）；缺省 = 协议最大值
maxConcurrentStreams = 64         # 每条连接的双向流上限；缺省 = 网关取 maxStreamsPerSession、agent 取 512
```

- **两个角色同一套规则**：网关与 agent 都读这些键——此前 agent 侧的保活间隔（5s）与流上限（512）是硬编码的，连运维面都改不了。
- **UDP 上限是"接收判定"，不是"缓冲大小"**：接收缓冲永远按协议最大值（65,507 + 帧长前缀）分配，因此**任何平台都不会把大数据报截断后再转发**（Linux 的 `recv_from` 会静默截断，Windows 会返回 `WSAEMSGSIZE`——两种平台行为不同，截断转发等于把脏数据交给后端服务）。超过配置上限的数据报**整包丢弃并计数**：`sdkwork_tunnel_udp_datagrams_too_large`（Prometheus）。上限对**两个方向**都生效。
- **校验在配置期**：`keepAliveIntervalSecs` ≤ 3600（0 表示关闭探测）、`heartbeatTimeoutSecs` ≥ 心跳间隔（否则会话在首个心跳到达前就过期，看起来像网络故障，其实是单位写错）、`udpPacketSize` ∈ 512..=65507、`maxConcurrentStreams` ∈ 16..=65536（低于 16 连控制流都放不下）。不合法的值在 `validate` / 配置加载时即报错，不会被静默夹到边界。
- 证据：`config::transport_tunables_are_derived_then_overridable`、`config::unusable_transport_tunables_are_configuration_errors`、`server_toml::materializes_tunnel_transport_tunables`、`server_toml::an_unusable_transport_tunable_is_a_configuration_error`、`agent::transport_options_follow_the_configured_network_tunables`、e2e `a_datagram_above_the_configured_packet_size_is_dropped_and_counted`（512B 上限：512B 数据报回显成功、900B 被丢弃并计数）。

### HTTP 头改写（FRP `hostHeaderRewrite` / `requestHeaders.set` / `responseHeaders.set`）

FRP 允许在转发链路上改写 `Host` 并增删改请求/响应头；本仓在**网关**（唯一解析 HTTP 的一跳）执行同样的改写，agent 仍然是纯字节转发：

```toml
[[tunnel.routes]]
name = "web"
protocol = "http"                       # 仅 HTTP 平面支持
domain = "demo.sdkwork.link"
target = "127.0.0.1:3000"
hostHeaderRewrite = "backend.internal:8080"   # 本地服务看到的 Host

[tunnel.routes.requestHeaders]                # 转给 agent 的请求上设置
set = { "X-From" = "edge", "X-Tenant" = "acme" }

[tunnel.routes.responseHeaders]               # 回给访客的响应上设置
set = { "X-Served-By" = "sdkwork" }
```

- **只允许 `set`（替换或新增）**，与 FRP 的配置形状一致（`set` 是 map，因此不存在重复名）。
- **构造期强校验，而不是运行期兜底**：名字必须是 RFC 9110 token，值不得含 CR/LF/NUL 等控制字符（HTAB 除外），单方向最多 32 个、值最长 2048；**帧控与逐跳头一律拒绝**（`Content-Length`、`Transfer-Encoding`、`Connection`、`Keep-Alive`、`Upgrade`、`TE`、`Trailer`、`Expect`、`Proxy-*`）——改写它们会让中继两侧的解析器与线上字节失去同步，属请求走私类缺陷而不是配置风格问题。`Host` 只能通过 `hostHeaderRewrite` 设置，不能塞进 `set` 里。
- **只对 HTTP 生效**：`tcp`/`udp`/`stcp`/`sudp` 上声明改写会**在配置期报错**（`HTTP-only`），而不是被静默忽略。
- **顺序**：先剥离逐跳头、再注入 `X-Forwarded-For`/`X-Real-IP`，**最后**套用路由自己的改写——运营方显式写的值优先于派生的值。
- **不放大权限**：准入（`allowed_ips` / visitor token）读的是**连接**解析出的访客地址与凭据，不读这些头，因此改写无法绕过 ACL。改写后的值会出现在 `GET /tunnel/routes` 与机器清单里，便于核对线上实际策略。
- **离线可预检**：`sdkwork-webserver-tunnel verify` 会打印 `request Host rewritten to ...` 与 `request/response headers set at the gateway: ...`，非法名字/值/平面在预检阶段即报错并非零退出。

### admin 控制台里的机器管理

管理面 API（`/backend/v3/api/tunnel_machines*`）把这套能力交给控制台：

| 操作 | 端点 | 权限 |
|---|---|---|
| 机器清单 | `GET /backend/v3/api/tunnel_machines` | `web.tunnel.read` |
| 机器详情 | `GET /backend/v3/api/tunnel_machines/{deviceId}` | `web.tunnel.read` |
| 启停开关 | `PUT /backend/v3/api/tunnel_machines/{deviceId}/enabled` | `web.tunnel.write` |
| 声明路由 | `POST /backend/v3/api/tunnel_machines/{deviceId}/routes` | `web.tunnel.write` |
| 删除路由 | `DELETE /backend/v3/api/tunnel_machines/{deviceId}/routes/{routeId}` | `web.tunnel.write` |

要点：

- **数据来自数据面，不落库**：机器状态是运行时事实（只有持有会话的进程答得准），因此管理面按配置的 `SDKWORK_WEBSERVER_TUNNEL_OPERATIONS_URL` 把请求转发到**本机**数据面 operations 监听器，令牌复用 `SDKWORK_WEBSERVER_OPERATIONS_AUTH_TOKEN_FILE`（同机只配一次）。未配置该 URL 时接口返回 **503 并说明原因**，不会返回"空机器列表"——那会让人误以为"没有机器"而排查错方向。机器历史与多实例扇出（落库版本）是后续增量，不在当前实现里。
- **路由声明只声明，不指定目标**：请求体没有 target 字段，机器仅在声明与它**本地配置的模板**匹配时才激活，并始终拨号自己的目标（PRD §45 SSRF 边界）。因此 `active:false` 是正常答案（离线排队，或机器不认识该模板），不是错误。
- **权限点独立**：`web.tunnel.read` / `web.tunnel.write` 与 `web.cluster.*` 分离——能看集群不等于能关机器。IAM 权限目录需在 IAM 侧登记这两个新码（本仓只声明与校验 `permission_scope`）。
- 组织为**平台基础设施**：租户上下文的调用被 403 拒绝而不是被收窄，避免"看起来像没有机器"。
- 控制台入口：`/admin/tunnel/machines`（模块 `tunnel`），清单页提供启停（关闭需二次确认）与"声明路由/删除路由"动作；关闭方向按机器当前状态出现，不会给出无意义操作。
- CLI：`sdkwork-webserver-tunnel status|list|remove --url <ops-base>`、`verify`（离线预检）、`visitor`（消费密钥路由）；诊断用 `doctor`（端点 → TLS → QUIC → STP 握手 → 认证 分级检查，PRD §83）。

## 重连与优雅关闭（PRD §27, §105）

Agent 断线后按 1s→2s→4s…60s 指数退避 + ±20% 抖动自动重连；只有稳定运行过一个初始退避周期的会话才重置曲线——拨号/握手失败不重置，网关宕机时机队不会退化成 ~1Hz 重连风暴。路由**热重载**经 `AgentRuntime::reload`（FRP `frpc reload` 对等）：先注销被删路由、再注册新增/变更路由、最后切换数据面路由表，**在传输中的流不受影响**；无会话时重载会发布新配置并在下次连接生效（报告 `appliedOffline=true`），且退避等待期间受理重载不会缩短退避。网关关闭顺序：停止 accept → 关闭 endpoint/会话 → 注销路由 → 释放 TCP 监听。

## 测试

- 单元：领域模型、协议 round-trip（`decode(encode(m)) == m`）、未知消息降级、帧尺寸上限、TLS 材料与指纹、注册表冲突、会话预算、退避、健康检查与负载均衡选路、带宽令牌桶、访客条目校验、CONNECT 请求解析、离线预检规则。
- 集成：QUIC 回环、gateway+agent+本地 HTTP/TCP/UDP 端到端（含 UDP 数据报往返、私有 UDP 拒绝匿名）、通配符域名子域路由、数据面 `[tunnel]` 配置中继、§77 禁用兼容、认证拒绝、证书指纹拒绝。**服务端先发言的协议（SSH 这类）另有专门用例**：访客连上后一个字节都不发，断言本地目标在访客开口前就被拨号并收到字节 —— 懒拨号（等访客先字节）只让该用例红，「访客先说话」的 TCP 用例抓不到；**密钥路由版本**（`a_secret_route_carries_a_server_first_protocol_like_ssh`）走 `stcp` + `visitor` 全链路，锁定上文那条 SSH 配方。
- 集成（对齐矩阵缺口补全，均在 `tunnel_end_to_end.rs`）：PROXY protocol v1/v2 头抵达本地目标、`locations` 同 host 按最长前缀分流、带宽上限真实节流（同样字节耗时显著变长）、密钥路由的正确/错误密钥与协议错配三种结局、`visitor` 运行时把 stcp/sudp 映射到本地端口（sudp 按本地客户端分流）、健康检查 + 负载均衡组只把流量给健康副本且全下线时给出「无健康副本」、单端口多路复用入口按 host 与按端口各自抵达 HTTP/tcp 路由（含 407 质询、404 与 405 拒绝）、活体 agent 热重载（新增/改指向/删除/非法配置被拒且不影响在跑配置）。
- 离线预检：`verify` 模块单测（公开面陈述、同面争抢、组面不一致、组员无探针告警、重名与坏目标、网关区间自洽）+ CLI 用例（成功/冲突/缺密钥变量/JSON/用法错误）+ 网关 `validate` 用例（健康通过、同面冲突失败、disabled 通过）。
- webserver 集成（`tests/tunnel_relay.rs`）：同一份 `[tunnel]` 配置分别经**两个数据面入口**各跑一遍域名中继 —— 普通入口（`run_data_plane_until`）与**线上边缘运行时用的投递面入口**（`run_website_data_plane_with_operations_until` + `Some(executor)`）；另有 WebSocket 升级穿过隧道的用例（断言 101 之后的字节双向泵送）、访客被拒时 403/401（而非 502）、无健康副本时 503。
- webserver 集成（集群 hop）：轮询/离线剔除/全离线 503、防环标记的两种失效模式（伪造标记不得被平衡、已带标记的兄弟必须本地服务）、**WebSocket 升级泵**（实例侧断言收到的握手携带 `x-served-by-cluster-hop` 与访客真实地址，101 之后的字节双向泵送）。
- 集群成员端到端（`cluster_member_e2e`）：LAN 直连与 **TUNNEL 中继**两条传输各跑一遍「注册 → 心跳 → 拉清单 → 应用 → ack」，TUNNEL 用例断言上线模式确实以 `joinMode=TUNNEL` + `routeDomain` 到达注册表；另有 cordon/drain 完成闭环与「工作未结束不得报完成」两例。

## 已知限制（V1，PRD §87/§133 对齐）

- 状态存于内存（PRD §116）；网关重启后 agent 自动重连并重建路由，REST create-route 的离线设备声明不持久化。
- Gateway→agent 的 remove（REST 删除）不通知在线 agent 的本地模板；agent 重连后会重新注册其模板路由（模板是本地真相）。运行中改配置用 `AgentRuntime::reload`（新增/删除/改指向都生效，见「重连与优雅关闭」）。
- **隧道运行清单无控制台消费面**：会话 / 路由 / 端口 / 字节 / 重连这些数据只有 operations REST（`GET /tunnel/status`、`GET|POST /tunnel/routes`）与 Prometheus 指标两个出口，`apps/**` 无任何消费方。集群管理页呈现的是**每实例**的 `joinMode` 与隧道路由域名（`WebserverWorkspace.tsx`），不是隧道网关自身的运行清单——两者不要混为一谈。
- **不提供 SSH 服务端（FRP `sshTunnelGateway` / `ssh -R` 免安装接入）**：有意取舍，理由与替代路径见「SSH 访问」一节；`stcp` + `visitor` 覆盖同样的效果，且不新增公网 SSH 监听面、不绕过"agent 只激活与本地模板匹配的声明"这条边界。
- **代理压缩（FRP `transport.tcpMux`/压缩，矩阵 B5/B7）未实现**：QUIC 自带头部压缩与多路复用，收益有限；连接池同理。
- mTLS（agent 侧证书）、隧道专属 ACME、多网关集群路由、P2P/Mesh（xtcp）为 P1/P2（PRD §8），未实现。
