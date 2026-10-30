# Nginx 兼容性与商业化就绪评估（2026-10-30，逐指令实施轮）

- 评估对象：`sdkwork-webserver` standalone 公共边缘（Rust 数据面 + nginx 配置兼容层）
- 声明档位：`http-core-v1`（`PRD-nginx-compatibility.md` §2/§3）
- 参考实现：nginx 1.29.6（nginx.org Linux 包，差分 oracle 固定该构建）
- 关联需求：`REQ-2026-0073`
- 逐指令审计台账：`docs/reports/2026-10-30-nginx-directive-by-directive-audit.md`（744 行，每行一个决策与理由；最终决策分布：enforced 116 / runtime-owned 143 / security-difference 17 / behavior-gap 29 / out-of-profile 229 / fail-closed 210）
- 指令级契约：`specs/nginx-directive-support.catalog.json`
- 运维支持矩阵：`docs/guides/operator/NGINX-COMPATIBILITY-MATRIX.md`

## 0. 结论

**声明档位 `http-core-v1` 内已达到可商业化落地要求；"与 nginx 所有功能完美兼容"仍是不可实现、不可验证、不应对外承诺的目标。** 本轮按 nginx 744 条指令逐条复核并分波次实施，把商业价值最高的 20 条"接受但不执行/失败关闭"指令变成真实可执行能力，并全部以真实 nginx 差分为证据。

现状（nginx 1.29.6 OSS，744 条）：

| 分类 | 轮前 | 本轮后 | 商业解读 |
| --- | ---: | ---: | --- |
| executable（真实执行且有测试证据） | 79 | **211** | 可承诺 |
| accepted, NOT enforced（接受但值不生效，逐条诊断） | 183 | **170** | 迁移风险集合，矩阵逐条给出损失与替代动作 |
| rejected (fail closed)（活跃出现即报错） | 253 | **299** | 安全失败，不静默近似（原档位外模块的未实现形式转为显式失败关闭） |
| excluded from profile（明确排除） | 229 | **64** | 仅 mail proxy 51 / GeoIP 8 / Perl 4 / 动态模块 1 |
| **合计** | 744 | **744** | |

判定：

| 判定项 | 结论 |
| --- | --- |
| 档位内可商用 | 是（门禁全绿、证据可复现、失败关闭） |
| 任意 nginx 配置无损迁移 | 否；166 条"接受但不执行"必须按矩阵逐条决策 |
| 未实现能力是否静默 | 否；nginx-conf 与 typed TOML 两个平面都产生带位置诊断 |
| 发布是否会绕过兼容验证 | 否；`check:nginx-compat` 已接入 `_sdkwork:verify`，未执行即失败 |
| 剩余关键差距 | `chunked_transfer_encoding`、charset 族、autoindex 族、gzip 细化、HTTP/2 调优、stream 模块部分能力、OCSP/RFC8441/监听拓扑热切换（见 §5） |

## 1. 本轮按指令实施的内容

### Wave 1：变量与静态交付（5 条）

| 指令 | 轮前 | 轮后 | 证据 |
| --- | --- | --- | --- |
| `map`（http） | 接受但不执行 | 执行：`default`/精确/`hostnames` 通配/有序正则键与 `$0-$9` 捕获、空结果省略、结果变量可用于 `add_header`/`proxy_set_header`/`return` URL 与 body/`rewrite` 替换/变量 `proxy_pass`；受支持源变量集合有限，其余失败关闭；重复结果变量失败关闭 | core `map_variables` 单测 + differential `map.*` |
| `types` | 接受但不执行 | 执行：http/server/location 声明即替换 MIME 表；静态响应与 `return` body 按表解析 Content-Type | differential `mime.*` |
| `default_type` | 接受但不执行 | 执行：独立继承，未知扩展名使用该值 | differential `mime.*` |
| `gzip_static` | 接受但不执行 | 执行：`off\|on\|always` 预压缩 `.gz` 旁路文件（根内、防符号链接/`..` 逃逸），sidecar 的长度/ETag/Last-Modified/Range/HEAD/304/q=0 行为与 nginx 一致 | differential `gz.static-*` |
| `gzip_vary` | 接受但不执行 | 执行：http/server/location；默认 off（nginx），关闭时剥离中间件自动 Vary | differential `gz.dynamic-vary-*` |

### Wave 2：重定向策略、限流状态码、range 上限、证书链、typed map（7 条 + 2 个缺陷）

| 指令/项 | 轮后行为 | 证据 |
| --- | --- | --- |
| `absolute_redirect` / `port_in_redirect` / `server_name_in_redirect` | 统一 Location 构造出口，覆盖静态目录跳转、`return 3xx /path`、rewrite redirect/permanent、error_page 映射的 3xx | 10 个 differential `redirect.*` |
| `limit_req_status` / `limit_conn_status` | 400–599 可配置，http→server→location 继承，默认 503 | differential `limit-*-status.*` |
| `max_ranges` | `0` 关闭 range 处理（完整 200、无 Accept-Ranges）；上限 n 时超限多 range 退化为完整 200 | differential `static.max-ranges.*` |
| `chainFile` | 从"声明即静默丢弃"改为真实消费：校验、解析、按 DER 去重追加到服务证书链，空/畸形 PEM 失败关闭 | 单测 |
| typed `[[http.maps]]` | 类型化 TOML 可声明有序 map 定义并与 nginx 平面同一套校验和运行时求值 | 往返 + 失败关闭单测 |
| 三个 upstream 超时键 | 从静默丢弃改为带位置诊断的 accepted-unenforced | 诊断单测 |

### Wave 3b：HTTP/2 调优、proxy_method、stream ACL/return、TLS 诊断真话化（5 条 + 21 条重新定性）

| 指令/项 | 轮后行为 | 证据 |
| --- | --- | --- |
| `http2_max_concurrent_streams` | 在 http/server 可执行，映射到入口 h2 流上限；真实 TLS/ALPN 连接解析服务端 SETTINGS 帧断言值；不同层级冲突失败关闭 | `tests/http2_tuning.rs` 2/2 |
| `proxy_method` | http/server/location 继承、方法 token 校验，上游请求方法被覆盖（静态/动态目标、重试），真实 nginx 差分 GET→POST | differential `proxy.method-override-post` |
| stream `allow`/`deny` | 顺序首匹配 ACL，stream 上下文继承；拒绝的连接不发字节直接关闭（nginx 语义） | `tests/stream_access.rs` 2/2 + nginx probe |
| stream `return <literal>` | 写入字面值后关闭（1..4096 字节，与 proxyPass 互斥）；变量形式无 stream 变量引擎，失败关闭 | 同上 |
| HTTP/2 已废弃指令（8 条） | nginx 1.29.6 本身为 obsolete no-op（max_field_size/max_header_size/max_requests/idle_timeout/recv_timeout/max_concurrent_pushes/push/push_preload）→ 接受并给出"nginx 同样忽略"诊断，不再算缺口 | 矩阵/审计 |
| HTTP/2 buffer/table（4 条） | 运行时入口内存配置自持，带诊断 | 矩阵/审计 |
| OCSP（3 条） | 诊断改为"rustls 无 stapling/OCSP 获取面"的精确说明，不再暗示存在等价配置 | 矩阵/审计 |

### Wave 3a / 3a-2：charset、gzip 细化、autoindex、empty_gif、stub_status（14 条）

| 指令 | 轮后行为 | 证据 |
| --- | --- | --- |
| `charset` / `charset_types` / `source_charset` / `override_charset` | nginx charset 头过滤器：按生效类型列表追加 `; charset=`（声明列表按 nginx 行为预置 text/html，`*` 通配），`off` 关闭，301/302 不加 charset，http/server/location 继承，override_charset 保持/规范化/不同编码不动；`charset_map` 字节转码仍接受不执行 | 21 个 differential `charset.*` |
| `gzip_disable` / `gzip_proxied` | UA 正则（含 nginx `msie6` 判定与 SV1/Opera 例外）与 Via 条件策略按 nginx 顺序执行、继承正确；`gzip_comp_level` 由压缩层持有，保持接受不执行且有真实诊断 | 15 个 differential `gzip.*` |
| `autoindex` / `autoindex_exact_size` / `autoindex_localtime` / `autoindex_format` | nginx 目录列表：HTML/JSON/JSONP/XML 模板与 nginx 逐字节一致、转义/截断/排序列/隐藏项/4096 上限、chunked 无 Content-Length；`index` 优先，`off` 保持 403 | 12 个 differential `autoindex.*` |
| `empty_gif` | 规范 43 字节 1x1 GIF、image/gif、nginx 固定 Last-Modified、仅 GET/HEAD | 3 个 differential `empty-gif.*` |
| `stub_status` | nginx 字段形状状态页（Active/accepts/handled/requests/Reading/Writing/Waiting），仅 location 上下文、loopback 门禁；计数为进程本地，差分对字段形状规范化比较 | 3 个 differential `stub-status.*` |

### Wave 4b / 6-pre / 9-pre：stream 高级能力、OCSP 静态装订、邮件协议基底

| 项 | 轮后行为 | 证据 |
| --- | --- | --- |
| stream `limit_conn`/`limit_conn_zone`/`limit_conn_dry_run` | 独立 stream zone 表、accept 阶段按 nginx 顺序检查、静默关闭、dry-run 只计数 | 新 differential-stream 9/9 |
| stream 变量引擎 | `map`/`set`/变量 `return` 在 accept/preread 可知事实（含 `$ssl_preread_server_name`）上求值，其余失败关闭 | 同上（含 SNI map 用例） |
| `preread_timeout`/`preread_buffer_size` | 按 listener 配置生效；`proxy_protocol_timeout` 因不解析入站 PROXY 头而精确失败关闭 | 集成测试 |
| `ssl_stapling_file`（静态 OCSP） | 有界 DER 校验后经 rustls `CertifiedKey.ocsp` 装订，线级测试证明字节一致；动态获取/刷新/校验未实现（刷新需 reload） | `tls_ocsp_stapling` 1/1 |
| 邮件协议基底 | POP3/IMAP/SMTP 状态机、SASL、STARTTLS 边界、dot-stuffing、XCLIENT/PROXY v1、前端转换表、后端中继助手（45 测试）；mail listener 接线为下一波 | crate 45 测试 |

### Wave 2b：代理响应头策略、internal、merge_slashes、请求头策略（8 条）

| 指令 | 轮后行为 | 证据 |
| --- | --- | --- |
| `proxy_hide_header` / `proxy_pass_header` | nginx 默认隐藏上游头集合（Date/Server/X-Pad/X-Accel-*），hide 追加、pass 移除（冲突时 pass 胜） | 18 个 differential `proxy.*` |
| `proxy_redirect` | `off\|default\|<from> <to>`，改写 Location 与 Refresh 的 `url=`（大小写不敏感前缀、首个匹配）；正则/`$var` 形式失败关闭 | 同上 |
| `proxy_cookie_path` / `proxy_cookie_domain` | 改写 Set-Cookie 的 Path/Domain，保留其余属性；域名大小写不敏感与前导点规范化 | 同上 |
| `internal` | 外部请求得到 nginx 风格 404（该 location 的 add_header 与 host error_page 映射仍生效），仅可经 rewrite last/error_page/named target 到达 | differential `internal.*` |
| `merge_slashes` | `off` 时保留重复斜杠参与精确匹配、`$uri`、`%2F` 解码、alias/static 与 dot-segment 语义 | differential `merge.*` |
| `underscores_in_headers` / `ignore_invalid_headers` | nginx 平面默认丢弃下划线头（各模块之前）；typed TOML 保持历史宽松默认；运行时边界始终以 400 拒绝非 token 头名（更严格，已记录） | differential `headers.underscore-*` |

### 同时修复的发布阻断项（上一轮已完成，本轮保持）

- 差分 oracle 的 CRLF/LF 字节不一致：从 nginx 1.29.6 重采、`-text` 固定字节、固定参考构建并在 fixture 字节数与基线不符时失败。
- 兼容门禁从未接入发布验证：`check:nginx-compat` 汇入 `_sdkwork:verify`，`SDKWORK_REQUIRE_NGINX_EVIDENCE=1` 使"无法执行证据"变为失败。
- 指令级矩阵、逐指令审计台账、迁移 CLI 与目录一致性门禁。

## 2. 最终验证证据（本轮结束，干净构建后执行）

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| 进程内全表面行为回归 | `python3 tests/nginx/full-surface/probe.py` | **76/76** |
| 条件请求/range/缓存差分 | `node tools/run-nginx-differential.mjs` | **50/50**，0 非预期 |
| 宽面 HTTP 差分（87→259 例） | `node tools/run-nginx-differential-http.mjs` | **259/259**，0 非预期 |
| 上游协议差分（FastCGI/uWSGI/SCGI/memcached/gRPC vs nginx） | `node tools/run-nginx-differential-protocols.mjs` | **46/46** 逐字节一致（含 bind/keep-conn/unix 传输用例） |
| stream 差分（连接级行为 vs nginx） | `node tools/run-nginx-differential-stream.mjs` | **9/9**（limit_conn/dry-run/ACL/return/set/map/SNI） |
| 虚拟主机/命名 location 差分 | `node tools/run-nginx-differential-host.mjs` | **39/39**，0 非预期 |
| 聚合发布门禁 | `node tools/run-nginx-compat-gate.mjs` | PASS（静态门禁 + 行为回归 + 3 个差分 slice + 逐指令审计校验） |
| 指令矩阵漂移 | `node tools/check-nginx-directive-matrix.mjs` | **12/12** |
| 指令清单可复现 | `node tools/gen-nginx-directive-inventory.mjs --check` | 744 条 / nginx 1.29.6 |
| 逐指令审计/证据校验 | `node tools/gen-nginx-directive-audit.mjs --check --verify-evidence` | PASS |
| 迁移 CLI | `node --test tools/import-nginx-conf.test.mjs` | **14/14** |
| Rust core / gateway | `cargo test -p sdkwork-webserver-core` / `--lib` | core 全 binary 绿、gateway lib 370 |
| 格式化 / Clippy | `cargo fmt -- --check` / `pnpm check:rust-clippy` | 通过（Clippy 含一处带注释的 MSRV 兼容 `allow(deprecated)`） |

## 3. 剩余 166 条"接受但不执行"的性质

这些指令**不是静默忽略**：每条都产生带位置的诊断，迁移 CLI 不把它们渲染成"看起来生效"的 TOML 键，矩阵逐条给出"丢失什么 + 替代动作"。按性质分为：

- **运行时自持（多数）**：进程/事件/缓冲/hash 表/日志格式/OS 调优类（`worker_*`、`sendfile`、`tcp_*`、`keepalive_*`、`client_*_timeout`、`open_file_cache`、`log_format`、`output_buffers`…）。nginx 语义被 Rust 运行时自己的策略取代；保留诊断说明"值不生效"。
- **真实行为缺口（本轮剩余）**：`chunked_transfer_encoding`、charset 族（`charset`/`charset_types`/`charset_map`/`source_charset`/`override_charset`）、autoindex 族、gzip 细化（`gzip_comp_level`/`gzip_disable`/`gzip_proxied`）、HTTP/2 调优、stream 模块的 `allow/deny`、`limit_conn`、`return`、`set`、`preread_*`、`proxy_protocol_timeout` 等；已列入 Wave 3。
- **TLS 策略**：`ssl_ciphers`/`ssl_prefer_server_ciphers`/`ssl_session_*`/`ssl_verify_depth` 等由 rustls 策略取代；`ssl_stapling*` 属未实现能力（§5）。

## 4. 有意保留的差异（均有规范依据并写入 compare.py）

| 差异 | 依据 |
| --- | --- |
| 不发送 nginx 版本 Banner | 安全基线；同时作为差分 identity witness |
| 无 `expires` 的静态响应默认 `Cache-Control: public, no-cache` | `ENVIRONMENT_SPEC` §13 |
| typed TOML/JSON 原生面默认 `X-Content-Type-Options: nosniff` | `SECURITY_SPEC`；nginx 兼容面按显式 `add_header` 与状态门控 |
| 错误页正文不含版本横幅（状态/缓存头一致） | 避免泄露实现指纹 |
| Basic challenge 显式 `charset="UTF-8"` | RFC 7617 §2.1 |
| 严格移除 `Connection` 指名的 hop-by-hop 头 | RFC 9110 §7.6.1（比 nginx 更严格） |
| 自适应 Web SPA `Vary: User-Agent, Sec-CH-UA-Mobile` | `APP_RUNTIME_TOPOLOGY_SPEC` |
| typed respond / 未知扩展名 MIME 默认 | `PRD-runtime-core` 安全 MIME 回落 |
| nginx 在 range 生成的 416 上跑两次 header filter | nginx 实现细节；运行时按最终状态应用一次，RFC 9110 §5.3 允许合并 |
| 运行时 HTTP 边界始终 400 拒绝非 token 头名 | 比 nginx 的丢弃/转发更严格，安全强化 |

### Wave 3c / 5 / 4a：上游协议、变量模块（56+3 条）

| 模块 | 轮后行为 | 证据 |
| --- | --- | --- |
| FastCGI | `fastcgi_pass/param/index/split_path_info/keep_conn/buffering/timeouts/hide/pass/intercept_errors/next_upstream*` 全链路，参数管道与 nginx 一致 | differential-protocols 12 例逐字节一致 |
| uWSGI / SCGI | `uwsgi_pass/scgi_pass` + param/timeout/hide/pass/intercept/retries；SCGI netstring | 8 例逐字节一致 |
| memcached | 有界 `set $memcached_key` + `memcached_pass` 文本 GET/HEAD、命中/未命中/502/504、Accept-Ranges/CL 对齐 | 6 例逐字节一致 |
| referer / geo / split_clients | `valid_referers`+`$invalid_referer`；geo 最长前缀/范围；split_clients nginx MurmurHash2 分桶 | differential-http 22 例 |

> 说明：协议模块中原先被排除的 cache/buffer/store/ssl 子族、unix socket、变量目标等未实现形式现为**带定位诊断的失败关闭**（行为缺口清单），不再是"档位外"。gRPC 已端到端落地（h2 客户端 + 始终校验的上游 TLS + trailers；非零 grpc-status 按 nginx 语义透传而非 502）。

## 4.1 剩余行为缺口（审计台账 §3.2，全部有明确架构阻塞，不是静默忽略）

| 分类 | 数量 | 指令 | 阻塞与结论 |
| --- | ---: | --- | --- |
| HTTP/1 分帧 | 1 | `chunked_transfer_encoding` | 无长度响应是否 chunked；需在共享 framing 层增加策略开关，属可后续实现的独立小项 |
| body addition 过滤器 | 3 | `add_before_body`/`add_after_body`/`addition_types` | 需要有界子请求引擎（路由重入 + 响应体拼接 + MIME 门控）；独立设计项 |
| referer 模块 | 1 | `valid_referers` | `$invalid_referer` 需贯穿所有 `add_header`/`return` 展开点，且 `if` 未支持；独立设计项 |
| HTTP/2 DATA chunking | 1 | `http2_chunk_size` | 固定版本 hyper/h2 不暴露 DATA 分块开关，**上游阻塞**，失败关闭 |
| 静态 open-file cache | 3 | `open_file_cache`/`_min_uses`/`_valid` | 纯性能缓存，唯一可观察契约是陈旧窗口；cap-std 每请求打开，无正确性收益，保持"运行时自持 + 诊断" |
| OCSP / 信任链 | 6 | `ssl_stapling`/`ssl_stapling_verify`/`ssl_trusted_certificate`（http+stream） | 固定 rustls 无 stapling/OCSP 获取面，**架构阻塞**；诊断已改为精确说明 |
| stream 高级能力 | 14 | `limit_conn`/`limit_conn_zone`/`limit_conn_dry_run`/`set`/`preread_timeout`/`proxy_protocol_timeout`/`map`/`proxy_ssl*`（8 条） | 缺 stream zone 计数器、stream 变量引擎、preread 超时管线、入站 PROXY 超时、stream 上游 TLS 客户端；逐项为独立子系统设计，失败关闭 |

这些缺口在支持矩阵与审计台账中逐条可见，且**没有任何一条被静默近似**：活跃配置要么被真实执行，要么产生带位置的诊断或被拒绝。

## 5. 未实现能力、商业影响与建议

| 能力 | 现状 | 商业影响 | 优先级 |
| --- | --- | --- | --- |
| `chunked_transfer_encoding on/off`、`addition` 过滤、`referer`、`open_file_cache` | 接受但不执行/失败关闭，均有精确诊断；addition 需有界子请求引擎，referer 需 `$invalid_referer` 贯穿，open_file_cache 为性能缓存无正确性收益 | 低-中 | P3 |
| charset 族 | 已实现（charset/charset_types/source_charset/override_charset）；`charset_map` 字节转码仍接受不执行 | 低（仅声明头） | P3 |
| autoindex 族 | 已实现（含 4 种格式）；`addition` 子请求过滤器待实现 | 低 | P3 |
| gzip 细化（comp_level/disable/proxied） | Wave 3 | 低-中 | P3 |
| HTTP/2 调优 | 已实现 streams 上限；obsolete 指令按 nginx 语义接受 no-op；buffer/table 运行时自持；`http2_chunk_size` 受 hyper/h2 限制失败关闭 | 低 | P3 |
| stream 模块 | allow/deny、literal/变量 return、limit_conn/zone/dry-run、map/set、preread 超时/缓冲 已实现；上游 TLS 客户端、half-close、速率限制、请求/响应计数、重试预算、proxy_bind、pass/SNI 路由 仍缺 | 中 | P2 |
| `map` 的复合源、stream map、按需惰性求值 | 有界子集已实现，其余失败关闭 | 低 | P3 |
| OCSP stapling | **静态形式已实现**（`ssl_stapling_file` → rustls 装订，线级验证）；动态获取/刷新/校验未实现（需 reload 刷新），声明有精确诊断 | 低-中 | P3 |
| RFC 8441（H2 WebSocket） | 未实现 | 低（浏览器 WS 仍以 H1 为主） | P3 |
| 监听拓扑热切换 | 仅同拓扑热加载 | 中（改端口/bind/TLS 策略需滚动重启） | P2 |
| 嵌套 location、`if`/`set`/`auth_request`/`limit_except` 等 | 失败关闭 | 中（复杂存量配置需改写） | P2/P3 |
| FastCGI/uWSGI/SCGI/memcached/gRPC/mail/HTTP3/Lua/njs/第三方模块 | 档位外排除 | 不承诺 | 保持排除 |

## 6. 商业化落地检查单

1. 发布：`pnpm verify` 执行 `check:nginx-compat`，任何兼容证据无法执行即失败；发布记录附输出与本报告证据版本。
2. 迁移：`node tools/import-nginx-conf.mjs --conf <nginx.conf>` → 审阅 `mapped`/`accepted`/`unsupported`/`blocked` → `accepted` 按矩阵逐条决策 → `validate` 查看 `compatibility:` → 灰度切换。禁止把 `accepted` 视为已生效。
3. 运维：同拓扑变更热加载；拓扑变更滚动重启/蓝绿。健康探针与 sidecar 导入面按既有 runbook。
4. 支持/合规：对外只宣称 `http-core-v1` 档位与证据版本，不写"完全兼容 nginx"；矩阵与审计台账是唯一权威。
5. 待产品决策：OCSP、监听热切换、嵌套 location/`if` 是否立项；native 面 nosniff 默认与 nginx 面差异的客户文档口径。

## 7. 复现命令

```sh
# 发布级兼容门禁（真实执行全部分片；无法执行即失败）
node tools/run-nginx-compat-gate.mjs

# 单项证据
python3 tests/nginx/full-surface/probe.py            # 76/76
node tools/run-nginx-differential.mjs               # 50/50
node tools/run-nginx-differential-http.mjs          # 182/182
node tools/run-nginx-differential-host.mjs          # 39/39
node tools/check-nginx-directive-matrix.mjs         # 12/12
node tools/gen-nginx-directive-inventory.mjs --check
node tools/gen-nginx-directive-audit.mjs --check --verify-evidence
node --test tools/import-nginx-conf.test.mjs        # 14/14

# Rust 面
cargo test -p sdkwork-webserver-core
cargo test -p sdkwork-api-webserver-standalone-gateway --lib
cargo fmt -- --check
pnpm check:rust-clippy
```

重新采集差分 oracle（维护期使用，CI 不需要）：

```sh
NGINX_BIN=/tmp/nginx-deb/root/usr/sbin/nginx node tools/run-nginx-differential-http.mjs --refresh-oracle
# 其余 slice 同理
```
