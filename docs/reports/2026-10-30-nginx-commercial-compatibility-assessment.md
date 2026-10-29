# Nginx 兼容性与商业化就绪评估（2026-10-30，逐指令实施轮）

- 评估对象：`sdkwork-webserver` standalone 公共边缘（Rust 数据面 + nginx 配置兼容层）
- 声明档位：`http-core-v1`（`PRD-nginx-compatibility.md` §2/§3）
- 参考实现：nginx 1.29.6（nginx.org Linux 包，差分 oracle 固定该构建）
- 关联需求：`REQ-2026-0073`
- 逐指令审计台账：`docs/reports/2026-10-30-nginx-directive-by-directive-audit.md`（744 行，每行一个决策与理由）
- 指令级契约：`specs/nginx-directive-support.catalog.json`
- 运维支持矩阵：`docs/guides/operator/NGINX-COMPATIBILITY-MATRIX.md`

## 0. 结论

**声明档位 `http-core-v1` 内已达到可商业化落地要求；"与 nginx 所有功能完美兼容"仍是不可实现、不可验证、不应对外承诺的目标。** 本轮按 nginx 744 条指令逐条复核并分波次实施，把商业价值最高的 20 条"接受但不执行/失败关闭"指令变成真实可执行能力，并全部以真实 nginx 差分为证据。

现状（nginx 1.29.6 OSS，744 条）：

| 分类 | 轮前 | 本轮后 | 商业解读 |
| --- | ---: | ---: | --- |
| executable（真实执行且有测试证据） | 79 | **99** | 可承诺 |
| accepted, NOT enforced（接受但值不生效，逐条诊断） | 183 | **166** | 迁移风险集合，矩阵逐条给出损失与替代动作 |
| rejected (fail closed)（活跃出现即报错） | 253 | **250** | 安全失败，不静默近似 |
| excluded from profile（PRD §4 明确排除） | 229 | **229** | 不承诺；实现需独立协议/安全立项 |
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
| 宽面 HTTP 差分（本轮从 87 增至 182 例） | `node tools/run-nginx-differential-http.mjs` | **182/182**，0 非预期（已知差异均有规范依据） |
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

## 5. 未实现能力、商业影响与建议

| 能力 | 现状 | 商业影响 | 优先级 |
| --- | --- | --- | --- |
| `chunked_transfer_encoding on/off` | 接受但不执行 | 低（仅影响无长度响应分帧） | P3 |
| charset 族 | Wave 3 | 中（中文/遗留站点编码声明） | P2 |
| autoindex 族 | Wave 3 | 中（文件目录发布场景） | P2 |
| gzip 细化（comp_level/disable/proxied） | Wave 3 | 低-中 | P3 |
| HTTP/2 调优（streams/field size/idle/requests） | Wave 3 | 中（大流量/防滥用） | P2 |
| stream 模块 allow/deny、limit_conn、return/set、preread | Wave 3 | 中（TCP 边缘） | P2 |
| `map` 的复合源、stream map、按需惰性求值 | 有界子集已实现，其余失败关闭 | 低 | P3 |
| OCSP stapling | 未实现，声明被诊断 | 低（LE 2025 停 OCSP；默认 off；rustls 仅 TLS1.2 式装订且无刷新任务） | 产品决策 |
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
