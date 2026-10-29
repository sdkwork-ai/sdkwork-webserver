# Nginx 兼容性与商业化就绪评估（2026-10-30）

- 评估对象：`sdkwork-webserver` standalone 公共边缘（Rust 数据面 + nginx 配置兼容层）
- 声明档位：`http-core-v1`（`PRD-nginx-compatibility.md` §2/§3）
- 参考实现：nginx 1.29.6（nginx.org Linux 包，差分 oracle 固定为该构建）
- 关联需求：[REQ-2026-0073](../product/requirements/REQ-2026-0073-nginx-commercial-compatibility-closure.md)
- 指令级契约：[nginx-directive-support.catalog.json](../../specs/nginx-directive-support.catalog.json)
- 运维支持矩阵：[NGINX-COMPATIBILITY-MATRIX.md](../guides/operator/NGINX-COMPATIBILITY-MATRIX.md)

## 0. 结论

**在声明档位 `http-core-v1` 内，本产品达到可商业化落地的兼容性要求**：发布门禁可执行且"无法执行即失败"、真实 nginx 差分证据齐备、所有档位外/未实现行为一律失败关闭并产出诊断、指令级支持矩阵与迁移 CLI 由同一目录驱动。

**"与 nginx 所有功能完美兼容"不是一个可实现、可验证、可商业承诺的目标**，本次评估也不这样宣称。原因是客观的：

- nginx OSS 1.29.6 共 **744** 条指令，本产品档位内实际执行 **79** 条、接受但不执行 **183** 条、失败关闭 **253** 条、档位外排除 **229** 条（明细见支持矩阵）。
- 第三方模块、OpenResty/Lua/njs、FastCGI/uWSGI/SCGI/memcached、mail、HTTP/3、任意可执行配置属于 `PRD §4` 明确排除项；这些能力即使投入实现也无法与"任意 nginx 配置"形成可验证的等价关系。
- 商业化可行的定义是：**声明档位 + 指令级矩阵 + 真实 nginx 差分 + 失败关闭 + 有据可查的已知差异**。本仓库现在具备这五项，并有可执行门禁防止回退。

因此本次交付的判定是：

| 判定项 | 结论 |
| --- | --- |
| 档位内可商用 | 是（门禁全绿，证据可复现） |
| 任意 nginx 配置可直接迁移 | 否；183 条指令"接受但不执行"，迁移必须按矩阵处理，矩阵已标明每条损失与替代动作 |
| 未实现能力是否静默 | 否；nginx-conf 与 typed TOML 两个平面都会产生带位置的诊断 |
| 发布是否会绕过兼容性验证 | 否；`check:nginx-compat` 已接入 `_sdkwork:verify`，未执行即失败 |
| 剩余关键差距 | OCSP stapling、RFC 8441、监听拓扑热切换、嵌套 location、`map` 执行、generated-error gzip（见 §5） |

## 1. 本次修复的发布阻断项

以下问题在本次评估开始时真实存在，均已修复并有可执行证据：

### 1.1 真实 nginx 差分门禁在干净检出下必然失败（发布阻断）

- 现象：`tests/nginx/differential/nginx-oracle.json` 是从 CRLF 工作区采集的，而仓库内 fixture 为 LF；干净 Linux 检出下 ETag/长度/range 断言全部不匹配，`check:nginx-differential` 实测 **4/50 匹配、103 处非预期差异**，等于发布门禁长期不可用。
- 修复：用 nginx 1.29.6 对 LF fixture 重新采集 oracle；`.gitattributes` 将各差分 fixture 目录固定为 `-text`；`run-nginx.sh` 支持无 root 运行的包内 nginx（注入 http 临时目录、`TZ=UTC`）；`run-nginx-differential.mjs` 固定 `REFERENCE_NGINX`、拒绝来自其他构建的基线、并在 fixture 字节数与基线记录不一致时失败。
- 现状：**50/50 匹配，0 非预期差异**，仅保留 44 处规范依据的已知差异。

### 1.2 nginx 兼容门禁从未接入发布验证（审计 T8）

- 修复：新增 `tools/run-nginx-compat-gate.mjs`（`pnpm check:nginx-compat`），接入 `_sdkwork:verify`。它执行 gap catalog、corpus manifest、指令矩阵、进程内行为回归、以及从 manifest 自动发现的全部差分 orchestrator，并统一设置 `SDKWORK_REQUIRE_NGINX_EVIDENCE=1`：任何"本机无法执行而跳过"的兼容性证据都会让发布失败，而不是静默通过。

### 1.3 PRD §5/§6 要求的 nginx 能力未实现

- `server_name` 现在完整支持：精确、`*.` 前导通配（任意深度、最长优先）、`prefix.*` 后导通配（最长优先）、`.suffix`（同时认领裸名与通配）、`~` 正则（按书写顺序、nginx 真实的"含大写字母则忽略大小写"规则）、监听器默认 server；同名重复认领失败关闭。真实 nginx 差分：`tests/nginx/differential-host` **39/39**。
- 命名 location：`try_files … @name`、`error_page … = @name` 支持静态/proxy/return 目标，恰好一跳内部重定向，编译期拒绝链式/环、请求路径不可达；handler 按 nginx 相位重新执行 access/auth/limit/secure_link 门禁，命名 `return` 先于 access（与 nginx 实测一致），响应过滤在最终响应上执行。
- TLS SNI 仍是"精确 + 一级通配"索引，因此 `ssl` 监听器上的正则/后导通配 `server_name` 在导入期失败关闭，而不是用错证书应答（已在矩阵与 PRD 标注）。

### 1.4 `add_header` 语义不完整（安全头风险）

- 修复前：http 级 `add_header` 直接启动失败；location 级被接受但静默不输出；server 级被提升为无条件安全头，且无 `always`/状态门控/变量/重复顺序。
- 修复后：http/server/location 完整 nginx 语义（声明即替换继承数组、保持书写顺序与重复头、默认状态列表 200/201/204/206/301/302/303/304/307/308、`always` 全状态、受限变量展开、所有响应类型统一出口应用），未知变量/非 ASCII/超长声明在配置加载期失败关闭。真实 nginx 差分中新增 `headers.*` 用例全部匹配。

### 1.5 其他真实差异（由宽面差分暴露并修复）

| # | 修复前 | 修复后 | 证据 |
| --- | --- | --- | --- |
| 1 | `proxy_pass` 非斜杠 URI 拼接时多插入 `/` | 与 nginx 完全一致的拼接 | `differential-http` proxy 用例 + 单测 |
| 2 | 多 range 请求返回 416 | 206 `multipart/byteranges`，帧/边界与 nginx 一致 | 单测 + 差分 |
| 3 | 目录重定向 307 | 301（nginx） | 差分 |
| 4 | `.js` MIME `text/javascript` | `application/javascript`（nginx mime.types） | 差分 |
| 5 | `$proxy_add_x_forwarded_for` 重复运行时默认值 | 仅追加到入站头（nginx） | 单测 |
| 6 | gzip 压缩响应保留强 ETag | 弱化为 `W/"…"`（nginx） | 差分 |
| 7 | `limit_req` 拒绝带 `Retry-After` | 不发送（nginx） | 集成测试 |
| 8 | 目录无 index 返回 404 | 403（nginx autoindex off），不进 SPA 回落 | 差分 + 单测 |
| 9 | `error_page` 不作用于 `return`/重定向状态 | 同样映射（保留 Location，非 2xx 去掉表示校验头） | 差分 + 单测 |

### 1.6 指令级契约与迁移路径缺失

- 新增 `specs/nginx-directive-inventory.json`：从 nginx 1.29.6 真实 `ngx_command_t` 表解析的 744 条指令（上下文、参数语法、模块、参考构建），可用 `tools/gen-nginx-directive-inventory.mjs --check --source <dir>` 从源码树逐字节复现。
- 新增 `specs/nginx-directive-support.catalog.json`：每条指令的分类、运行时字段、importer 映射、风险、迁移影响/动作、证据。
- 新增 `tools/check-nginx-directive-matrix.mjs`（11 项检查、离线、变异测试）：目录/实现/mapping.rs/fixture/迁移 CLI/生成文档任何一方漂移即失败。
- 新增生成式运维矩阵 `docs/guides/operator/NGINX-COMPATIBILITY-MATRIX.md`（PRD §13 要求）。
- 迁移 CLI `tools/import-nginx-conf.mjs` 不再维护第二套分类器：消费目录、正确识别 `types`/`charset_map`/`map` 块体、不再把 `expires`/`default_type` 误报为 unsupported、绝不把"接受但不执行"的 nginx 指令渲染成看似生效的 typed TOML 键。

### 1.7 typed TOML 静默忽略行为键

- 新增 `server_toml::UNENFORCED_BEHAVIOR_TOML_KEYS` 与 `accepted_unenforced_toml_diagnostics`：TLS cipher/session/OCSP、proxy buffering/retry/header、`map`、autoindex、charset、MIME 默认等行为键被接受时输出 `compatibility:` 诊断；`validate` 摘要新增 `acceptedButUnenforced=` 计数。与 nginx-conf 导入平面的逐指令诊断对齐（PRD §11）。

## 2. 最终验证证据（2026-10-30 本机，干净构建后执行）

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| 进程内全表面行为回归 | `python3 tests/nginx/full-surface/probe.py` / `tools/run-nginx-behavioral-corpus.mjs` | **76/76 通过** |
| 条件请求/range/缓存差分 | `node tools/run-nginx-differential.mjs` | **50/50 匹配**，44 已知差异，0 非预期 |
| 宽面 HTTP 差分 | `node tools/run-nginx-differential-http.mjs` | **87/87 匹配**，227 已知差异，0 非预期 |
| 虚拟主机/命名 location 差分 | `node tools/run-nginx-differential-host.mjs` | **39/39 匹配**，3 已知差异，0 非预期 |
| 聚合发布门禁 | `node tools/run-nginx-compat-gate.mjs`（`pnpm check:nginx-compat`） | **PASS**（静态门禁 + 行为回归 + 3 个差分 slice，全部真实执行） |
| 指令矩阵漂移 | `node tools/check-nginx-directive-matrix.mjs` | **11/11 通过** |
| nginx 指令清单可复现 | `node tools/gen-nginx-directive-inventory.mjs --check` | 744 条、nginx 1.29.6、摘要校验通过 |
| 迁移 CLI | `node --test tools/import-nginx-conf.test.mjs` | **14/14 通过** |
| Rust 测试 | `cargo test -p sdkwork-webserver-core`（13 targets） | **438 通过，0 失败**（266 lib + 172 integration） |
| Gateway 测试 | `cargo test -p sdkwork-api-webserver-standalone-gateway --lib` | **347 通过** |
| 数据面集成 | `cargo test -p ... --test data_plane_integration` | **59 通过** |
| 格式化 | `cargo fmt -- --check` | 通过 |
| Clippy 门禁 | `pnpm check:rust-clippy`（8 crate，`-D warnings`） | 通过（本机 clippy 1.99 下新增一处带注释的 `#![allow(deprecated)]`，因 MSRV 1.89 不允许 `Atomic::try_update`，与本次行为无关） |
| 文档/脚本规范 | `check-repository-docs-standard.mjs`、`check-pnpm-script-standard.mjs` | 通过 |

说明：所有差分 oracle 均随仓库提交并固定参考构建；本机 nginx 1.29.6 二进制只用于**重新采集/复核**，CI 无 nginx 也可执行比较，但"无法执行证据"在发布门禁中会失败。

## 3. 兼容性现状（http-core-v1，744 条 nginx OSS 指令）

| 分类 | 数量 | 含义与商业解读 |
| --- | ---: | --- |
| executable | 79 | 运行时真实执行，每条都有可执行测试证据；可以承诺 |
| accepted, NOT enforced | 183 | 为让存量配置可加载而接受，但值不生效，逐条产生诊断；**这是迁移风险集合**，矩阵逐条给出"丢失什么 + 应当改用哪个 enforced 键"，不存在"配置看起来生效实际没有"的静默 |
| rejected (fail closed) | 253 | 活跃上下文出现即报错；比静默近似安全，迁移需改用 typed TOML 或移出 |
| excluded from profile | 229 | PRD §4 明确排除（第三方模块、Lua/njs、FastCGI 系、mail、HTTP/3 等） |
| sdkwork-only extensions | 2 | 非 nginx 指令的运行时扩展，已在目录中显式标注 |

最高风险的"接受但不执行"指令（迁移前必须处理）：`map`（变量解析为空）、`types`/`default_type`（MIME 策略不生效）、`gzip_static`（预压缩文件不服务）、`ssl_ciphers`/`ssl_prefer_server_ciphers`/`ssl_session_*`（rustls 策略优先）、`ssl_stapling*`（不装订 OCSP）、`proxy_next_upstream`/`proxy_method`/`proxy_hide_header`（重试/方法/响应头策略由运行时决定）、`absolute_redirect`/`port_in_redirect`/`server_name_in_redirect`（重定向构造策略）、`merge_slashes`（URI 规范化策略）、`limit_req_status`/`limit_conn_status`、`autoindex`。

## 4. 有意保留的差异（均已写入 compare.py 且有规范依据）

| 差异 | 依据 |
| --- | --- |
| 不发送 nginx 版本 Banner | 安全基线；同时作为差分两侧的 identity witness |
| 无 `expires` 的静态响应默认 `Cache-Control: public, no-cache` | `ENVIRONMENT_SPEC` §13 部署新鲜度策略 |
| 默认 `X-Content-Type-Options: nosniff`（typed TOML/JSON 原生面） | `SECURITY_SPEC`；nginx 仅在显式 `add_header` 时发送（nginx 兼容面已按 nginx 语义门控） |
| 错误页正文不含版本横幅（状态/缓存头完全一致） | nginx 内置错误页内嵌版本号，复制其正文等于泄露实现指纹 |
| Basic challenge 显式声明 `charset="UTF-8"` | RFC 7617 §2.1 |
| 严格移除 `Connection` 指名的 hop-by-hop 头 | RFC 9110 §7.6.1（比 nginx 1.29.6 更严格，属安全强化） |
| 自适应 Web SPA 响应 `Vary: User-Agent, Sec-CH-UA-Mobile` | `APP_RUNTIME_TOPOLOGY_SPEC` 设备面选择 |
| typed respond/unknown-extension 的 MIME 默认 | `PRD-runtime-core` 安全 MIME 回落 |
| nginx 在 range 生成的 416 上会跑两次 header filter（可能重复 `always` 头 / 输出非 `always` 头） | nginx 实现细节；运行时按最终状态应用一次，RFC 9110 §5.3 允许合并同名字段 |

## 5. 未实现能力、商业影响与建议

| 能力 | 现状 | 商业影响 | 建议与优先级 |
| --- | --- | --- | --- |
| OCSP stapling（`ssl_stapling*`） | 未实现；声明会被诊断 | 低：Let's Encrypt 2025 已停止 OCSP，本平台使用短生命周期 ACME 证书，nginx 默认 `ssl_stapling off`；rustls 的 OCSP 字段也仅覆盖 TLS1.2 式装订且无刷新任务 | **P3**：默认不实现，作为产品决策记录；若客户 CA 仍要求，需引入 OCSP 获取/刷新/校验 owner 后再立项 |
| RFC 8441（HTTP/2 WebSocket） | 未实现 | 低：浏览器 WebSocket 仍以 HTTP/1.1 为主；H1 WebSocket 已完整支持 | **P3**：随 hyper/h2 上游能力再评估 |
| 监听拓扑热切换（新增端口/改 bind/改 TLS 策略需重启） | 仅同拓扑热加载 | 中：常规发布（新增站点、改路由、换证书）无需重启；改监听拓扑需滚动重启 | **P2**：用灰度/滚动重启兜底；如需零停机上线新端口，立项 SO_REUSEPORT 蓝绿或监听器交接 |
| 嵌套 location | 失败关闭 | 中：常见于复杂存量配置；迁移需扁平化或落到 `raw` 审核路径 | **P2**：列为下一轮兼容面候选 |
| `map` 变量执行 | 接受+诊断，不执行 | 中：真实配置常用于 `$connection_upgrade`、UA 分支等；静默配置会得到不同行为（已有诊断，不静默） | **P1/P2**：商业化前建议实现受限 map 子集（`proxy_set_header`/`add_header`/`return`/`rewrite` 可引用），否则迁移评审必须逐条处理 |
| generated-error 的 gzip/Vary | 未对齐（压缩层在 handler 之外） | 低：仅影响错误页压缩协商 | **P3**：记录为已知差异 |
| 每指令继承语义建模（PRD §3） | 目录已含上下文/语法/运行时字段/风险/证据，未逐指令建模继承链 | 低（文档完整性） | **P3**：后续随目录 schema 演进补充 |
| FastCGI/uWSGI/SCGI/memcached/mail/HTTP3/Lua/njs/第三方模块 | 档位外拒绝 | 不承诺；若业务确有 PHP/FastCGI 需求，应经独立协议与安全立项，不属 nginx 兼容档位 | 保持排除 |

## 6. 商业化落地检查单

1. **发布**：`pnpm verify` 会执行 `check:nginx-compat`；任何 nginx 兼容证据无法执行即视为发布失败。发布记录需附 `check:nginx-compat` 输出与本报告的 `evidence revision`。
2. **迁移存量 nginx 配置**：`node tools/import-nginx-conf.mjs --conf <nginx.conf>` → 阅读 `mapped` / `accepted` / `unsupported` / `blocked` 四类结果 → 对 `accepted` 按矩阵逐条决策 → `validate` 确认 `compatibility:` 诊断 → 灰度切换。禁止把 `accepted` 直接视为已生效。
3. **运维**：同拓扑变更走热加载；监听拓扑变更走滚动重启/蓝绿。健康探针 `/healthz`、`/readyz` 与 sidecar 导入面按既有 runbook。
4. **支持与合规**：对外宣称兼容性时必须同时标注 `http-core-v1` 档位与证据版本；支持矩阵与机器目录是唯一权威，不允许另写"完全兼容 nginx"的市场表述。
5. **待产品决策**：`map` 是否实现、监听拓扑热切换是否立项、OCSP 是否永久排除、native 面 nosniff 默认与 nginx 面差异如何在文档中对客户解释（建议保留当前安全基线并在矩阵中说明）。

## 7. 复现命令

```sh
cd sdkwork-webserver

# 发布级兼容门禁（真实执行全部分片；无法执行即失败）
node tools/run-nginx-compat-gate.mjs

# 单项证据
python3 tests/nginx/full-surface/probe.py                  # 76/76
node tools/run-nginx-differential.mjs                     # 50/50
node tools/run-nginx-differential-http.mjs                # 87/87
node tools/run-nginx-differential-host.mjs                # 39/39
node tools/check-nginx-directive-matrix.mjs               # 11/11
node tools/gen-nginx-directive-inventory.mjs --check      # 744 条清单可复现
node --test tools/import-nginx-conf.test.mjs              # 14/14

# Rust 面
cargo test -p sdkwork-webserver-core
cargo test -p sdkwork-api-webserver-standalone-gateway --lib
cargo fmt -- --check
pnpm check:rust-clippy
```

重新采集差分 oracle（需要真实 nginx；仅维护期使用，CI 不需要）：

```sh
# nginx.org 官方 1.29.6 包解压到 /tmp/nginx-deb/root 后
NGINX_BIN=/tmp/nginx-deb/root/usr/sbin/nginx node tools/run-nginx-differential.mjs --refresh-oracle
NGINX_BIN=/tmp/nginx-deb/root/usr/sbin/nginx node tools/run-nginx-differential-http.mjs --refresh-oracle
NGINX_BIN=/tmp/nginx-deb/root/usr/sbin/nginx node tools/run-nginx-differential-host.mjs --refresh-oracle
```
