# SDKWork Web Server 全面审计报告（2026-10-20）

Status: review
Owner: SDKWork maintainers
Scope: `sdkwork-webserver` 全仓库（Rust 数据面/控制面、PostgreSQL 持久层、OpenAPI/SDK、PC/H5/移动端、CI/发布）
Authorities: `docs/product/prd/PRD.md`、`docs/architecture/tech/TECH_ARCHITECTURE.md`、`../sdkwork-specs/*`

> 本报告区分 **已复现/已由代码证明** 与 **静态审查待复现** 两类结论。所有结论都给出文件与行号，
> 不以文档声明作为实现证据。审计期间工作区由本次修复同时修改，`git status` 中的变更属于本次审计
> 修复，不是基线状态。

## 1. 结论摘要

**当前版本不具备商业化 GA 落地能力。** 主要依据：

1. **主分支 CI 为红色**：最近 6 次打包运行全部在 `Validate artifacts` 步骤失败，签名/SBOM 之后、
   上传/证明/发布之前终止，没有任何可发布工件（证据：仓库根目录 6 个 `tmp_poll_*.json` 运行快照）。
2. **`pnpm verify` 在本仓库不能通过**：`check-api-operation-patterns` 因 `PUT verification_file`
   的 operationId 动作不是 `update` 而退出 1；该门禁被 `_sdkwork:check → _sdkwork:verify →
   sdkwork.workflow.json` 的 `pnpm verify` 步骤引用。**本次已修复并复验通过。**
3. **生成 SDK 与契约漂移**：backend SDK 的 12 个语言目标在审计开始时与生成器输出不一致
   （dart/python/go/java/kotlin/swift/csharp/flutter/rust/php/ruby），生成物并非“全部由生成器拥有”。
   **本次已全量再生成并复验通过。**
4. **游标分页在三个已声明为 cursor 的列表上不可用**：deployments / source_versions / servers 的
   首页返回 `hasMore=true` 却不返回 `nextCursor`，而 `page=2` 被中间件拒绝（40003），
   第一页之后的数据任何客户端都取不到。**本次已修复并补充首页游标。**
5. **强制 PostgreSQL 门禁按构造无法通过**：`postgres_lifecycle` 测试在 `init()` 后断言
   `pending.is_empty()`，而生命周期实现把 baseline 折叠后的 22 个迁移视为 pending；该测试又是
   `#[ignore]`，只能由 `pnpm test:postgres:required` 显式执行——而该步骤排在已失败的
   `pnpm verify` 之后，从未执行。
6. **已提交的迁移被事后改写/改名/删除**（`0005`、`0006`、`0007`、`0008`、`0020`、`0021`，
   并删除 `0001–0004`）。生命周期用文件字节 SHA-256 做校验，任何记录过旧字节的数据库升级时都会
   得到 error 级 `checksum_mismatch` 并中止启动。
7. **部署执行链路不存在**：`webserver_deployment` 插入后状态永远停在 PENDING，没有生产代码更新
   该状态；`applications.deployments.create/rollback` 已从契约移除，激活又要求 `status=2`，
   因此应用激活永远是 409。PRD 的 FR-007/FR-008/FR-026（异步发布、不可变修订、回滚）在运行时
   是空的。
8. **集群“同步/收敛”只写数据库**：唯一实现 node 侧 manifest→apply→ack 的
   `cluster_member.rs` 只被测试调用，发布端把实例状态置为 PENDING 后没有任何进程会推进它。
9. **PRD §8.1 的性能/容量目标没有证据且默认值不满足**：默认 `maxConnections=10_000`
   （PRD 要求 100,000 空闲长连接）；配置校验要求 100k 连接时 header 宽度 ≤ ~10.7 KiB；
   TECH 自述 100k/24h soak 未满足；仓库无 load/soak 证据。
10. **商业化能力缺口**：无计费/权益（billing/entitlement）；用量计量在重启时丢窗口、对分块
    响应按 `Content-Length` 计 0 字节；Kubernetes 模板 `replicas: 1`；无 support bundle / SLO
    看板 / 多节点故障演练证据；多个 PRD 关联需求仍为 in-progress/draft。

**分页结论**：`check-pagination.mjs` 通过，但该脚本没有覆盖本次发现的“首页不返回游标”“单页 200
条截断”“前端 readAllPages / localStorage 客户端分页”等模式；PAGINATION_SPEC 的验收清单并未真正满足。

**SQLite / PostgreSQL 结论**：SQLite **不是**“未实现完整”，而是**按产品决策有意不做**：manifest
`engines: ["postgres"]`、Cargo 只启用 `sqlx/postgres`、仓库只实例化 `repository_engine!(postgres, …)`，
PRD 4.1 与 REQ-2026-0004 明确“PostgreSQL 是唯一权威服务器数据库”。仓库中没有 SQLite 服务器端
DDL/迁移/仓储，也没有任何文档声称 Web Server 支持 SQLite。**PostgreSQL 实现完整度较高，但迁移
不可变性、生命周期冒烟测试、部分索引/租户约束、锁与超时存在实质缺陷**（见 §8）。

## 2. 审计方法与证据

| 证据类型 | 执行内容 | 结果 |
| --- | --- | --- |
| 编译 | `cargo check --workspace --all-targets`（TMP 指向仓库内目录，规避宿主 `%TEMP%` ACL 问题） | 通过（仅 warning） |
| 单元测试 | `cargo test` 针对 core / edge-runtime / acme / routes-common / routes-backend / service | core：3 个 nginx glob 测试在 Windows 失败（路径语义，见 P3）；其余通过 |
| 规范门禁 | `check-pagination` / `check-api-envelope` / `check-rust-crate-naming` / `check-rust-manifest` / `check-rust-backend-composition` / `check-process-shared-database-pool` / `check-database-framework-standard` | 全部通过 |
| API 门禁 | `check-api-operation-patterns` | 修复前退出 1（3 处），修复后通过 |
| SDK 门禁 | `materialize_web_phase1_contracts --check`、`generate_web_sdks --check` | 修复前 SDK 漂移 12 语言；修复后通过 |
| 前端 | `pnpm --dir apps/sdkwork-webserver-pc typecheck`、layering/dist/build-script/source-config 检查 | 通过；无 lint 门禁 |
| 静态审计 | 8 个并行只读审计（分页/API、数据库、内存、并发、伪造实现、安全、前端/SDK、测试/CI），逐条核对文件与行号 | 见 §3–§7 |

## 3. P0 阻断级问题（发布前必须关闭）

### P0-1 CI 红色，无可发布工件
- 证据：`tmp_poll_run_39092705050.json` 等 6 个运行快照显示 `Resolve release version`/`Plan package
  matrix` 成功，6 个 `Package …` job 全部 `failure`，`Publish aggregate GitHub release` 与 `Deploy`
  `skipped`；失败步骤为 `Validate artifacts`（签名与 SBOM 已执行，证明/上传/发布被跳过）。
- 规范：`GITHUB_WORKFLOW_SPEC.md`、`RELEASE_SPEC.md`、PRD §8.4/§9。
- 影响：`security.artifactAttestations` / `signingRequired` / `sbomRequired` 只有声明没有工件证据；
  任何“已验证发布”的说法不成立。
- 修复：按 §10 的顺序修复 `pnpm verify` 与其后的 PostgreSQL/恢复/HA 步骤；在 validate 通过前不要把
  签名当作发布证据。

### P0-2 `pnpm verify` 被 API operationId 动作门禁阻断（本次已修复）
- 修复前：`check-api-operation-patterns.mjs --root .` 退出 1，3 条 `operation-id-action`：
  `PUT /backend/v3/api/root_domains/{rootDomainId}/verification_file` 的 operationId 末段为
  `upsert`，规范要求 `update`（`API_SPEC.md` §14–§16 的操作模式规则，检查器
  `tools/lib/api-operation-patterns.mjs:326-353`）。
- 修复：`apis/backend-api/web/openapi.yaml` operationId 改为
  `rootDomains.wechatVerification.update`；同步 `http_route_manifest.rs` 与 PC 调用点；重新物化
  OpenAPI/SDK。
- 复验：`check-api-operation-patterns` → `passed`（exit 0）。

### P0-3 生成 SDK 全语言漂移（本次已修复）
- 修复前：`node tools/generate_web_sdks.mjs --check` 退出 1，backend SDK 的 dart/python/go/java/
  kotlin/swift/csharp/flutter/rust/php/ruby 均与生成器输出不一致；缺失 5 个 WeChat 验证模型文件，
  且旧 `…UpsertResponse` 模型仍被引用。
- 影响：生成物可重复性/SDK 所有权被破坏；多语言客户端与契约不一致。
- 修复：`pnpm api:materialize` + `node tools/generate_web_sdks.mjs` 全量再生成；更新 PC 调用点。
- 复验：`generate_web_sdks --check` → `verified 14 materialized targets`；`materialize --check` → current。

### P0-4 游标列表首页不可续页（本次已修复）
- 证据（修复前）：`deployments.rs:155-162`、`source_versions.rs:76-83`、`servers.rs:71-76` 首页
  返回 `next_cursor: None, has_more: None`；`response.rs` 仅在 `has_more.is_some()` 时输出 cursor
  `pageInfo`；`pagination.rs:110-115` 对这三个路径拒绝 `page`。结果：第一页之后的数据不可达。
- 规范：`API_SPEC.md` §16.3（`hasMore=true` 必须携带 `nextCursor`）、`PAGINATION_SPEC.md` §3/§10.1/§12。
- 修复：首页根据精确 total 计算 `has_more`，从最后一行铸造 HMAC keyset cursor；servers/source_versions
  投影补 `id`；响应层按“是否 cursor 请求”区分 offset 首页（保留 total + nextCursor）与 cursor 续页；
  路由层传入 `cursor_request`。
- 复验：`cargo check` 通过；`check-pagination` 通过；新增/保留的 keyset 测试覆盖签名与篡改拒绝。

### P0-5 强制 PostgreSQL 生命周期门禁按构造无法通过（已修复测试语义）
- 证据：`crates/sdkwork-webserver-database-host/tests/postgres_lifecycle.rs:18-19` `#[ignore]`；
  `:61-71` 在 `init()` 后断言 `pending.is_empty()`、`migrate()==0`。而
  `sdkwork-database-lifecycle/orchestrator.rs:42-59` 的 `init()` 只应用 baseline、不写迁移历史，
  `plan_migrations()` 会返回 22 个迁移；`scripts/postgres-ci-verify.mjs:36-47` 正是运行该测试，
  `sdkwork.workflow.json` 把 `pnpm test:postgres:required` 设为强制 validate 步骤。
- 影响：唯一的真实 PG 生命周期证据要么红、要么从未执行；release 可以在没有 PG 证据的情况下被声称通过。
- 修复：按 `DATABASE_FRAMEWORK_SPEC.md` §7.5（fresh install = baseline + 全部有序迁移，迁移是
  load-bearing 的升级路径）把测试改为：init 后断言 pending 非空 → `migrate()` 返回全部 pending
  数量 → 再 plan 为空 → drift clean；并把 PostgreSQL/恢复/HA 三个 validate 步骤前移到
  `pnpm verify` 之前，避免再次被无关失败挡住。
- **真实执行证据**：在 WSL PostgreSQL 18.3 的 disposable 库（独立 schema）上执行
  `cargo test -p sdkwork-webserver-database-host --test postgres_lifecycle -- --ignored`
  → `1 passed`（exit 0）；同一实例的 repository parity → `1 passed`（exit 0）。

### P0-6 迁移校验和不可变性已被破坏
- 证据：`git log --diff-filter=R/M/D -- database/migrations` 显示 `2e711dac` 将 `0005/0007/0008`
  改名并改写，修改 `0006/0009`；`18791ae3` 修改 `0020`（`ADD COLUMN` → `IF NOT EXISTS`）；
  `21750a70` 修改 `0021`；`4fff0178` 删除 `0001–0004`。`database/migrations/postgres/README.md:46-61`
  明确记录 checksum 覆盖原始字节，且 0005 的历史哈希对应旧文件。
- 规范：`DATABASE_FRAMEWORK_SPEC.md` §7.2/§7.3/§7.5、`DATABASE_SPEC.md` §22/DB084。
- 影响：任何记录过旧字节的库升级时 `checksum_mismatch` 中止启动；README 本身引用了 2026-09-10
  的容器 crash-loop 事故。
- 修复：冻结已 tracked 的 `.up.sql` 字节；缺失元数据只走 `metadata.json` sidecar；内容修正一律
  新增前向迁移；对已受影响库提供审计化对账程序。

### P0-7 部署生命周期不可推进、回滚不可达、激活永久 409
- 证据：`deployments.rs:360-372` 插入 `status=0`，生产代码无 `UPDATE webserver_deployment …
  status`；路由清单只发布 `applications.deployments.list`（`http_route_manifest.rs:252-257`）；
  `WebBackendApi` 只声明 list（`app_ports.rs:847`）；`applications.rs:634-649` 激活要求 `status=2`。
- 规范：PRD FR-007/FR-008/FR-026。
- 修复：实现/绑定部署执行器并以 202 + operation 轮询重新发布 create/rollback；在未实现前，
  降级 PRD/UI 声明并把部署行标记为只读意图，不能显示“发布成功”。

### P0-8 集群同步/收敛是数据库状态写
- 证据：`crates/sdkwork-webserver-agent/src/cluster_member.rs:492-569` 的 manifest→apply→ack 仅被
  `tests/cluster_member_e2e.rs` 调用；`agent/src/main.rs:3` 只 `pub mod cluster_member;`，二进制不启动它；
  `cluster_self_report.rs:447` 的自上报只注册/心跳；`cluster_ops.rs:1178-1236` 发布 revision 后把实例
  置 PENDING 即返回“完成回执”。
- 规范：PRD §8.2、REQ-2026-0052、TECH_ARCHITECTURE §1。
- 修复：在出货二进制中启动 member loop 并提供生产 `SyncApplier`；用真实进程 ack `IN_SYNC` 作为
  验收证据；在此之前不要宣称收敛。

### P0-9 容量与性能目标无证据、默认值不满足
- 默认 `maxConnections=10_000`（`core/src/config/model.rs:88-90`）；PRD §8.1 要求 100,000 空闲长连接。
- 校验要求 `maxConnections × maxRequestHeaderBytes ≤ 1 GiB`（`validate.rs:1528-1531`），100k 连接
  迫使 header ≤ ~10.7 KiB；默认 header 为 64 KiB。
- `TECH-runtime-data-plane.md:146` 自述“尚未满足 100,000 连接或 24 小时 soak”。
- `capacity-planning.md` 无 load/soak 结果；仓库无压测基线。
- 修复：发布经验证的参考 profile（含 TLS/parser/task 开销的 RSS 实测），把默认/示例与文档目标对齐，
  并把 100k/24h/10k-route-1s 纳入发布门禁。
- **进展（2026-10-21）**：新增 `pnpm test:capacity`（真实数据面，32 并发 keep-alive，零失败、
  p99 < 1 s 断言；支持固定请求数与 `SDKWORK_CAPACITY_DURATION_SECONDS` soak 模式）。本机 dev
  profile 实测：32k 请求 11,251 req/s、p99 10.9 ms；5 s soak 153,772 请求、30,743 req/s、
  p99 1.86 ms，均零失败。证据见 [PRODUCTION-READINESS.md](../../operations/PRODUCTION-READINESS.md)。
- **仍未证明**：100,000 空闲长连接及对应 RSS 参考 profile、24 h soak。
- **10k 路由编译已达标**：`pnpm test:route-compile` 实测 10,000 条 exact 路由 **349 ms / 438 ms**
  （dev profile，两次运行），在 1 s 预算内；`SDKWORK_ROUTE_COMPILE_MAX_MS=1000` 可把预算变成断言。
- **进展（空闲连接）**：新增 `pnpm test:connections`，在本机分别保持 5,000 / 10,000 / 15,000 个
  空闲 socket 且新请求仍返回 200；100k 需目标机提升 fd/临时端口上限（同机回环还需多源地址），
  参考 profile 与 RSS 仍待目标机数据。

### P0-10 商业化运营证据缺失
- 前次生产就绪评审（`REVIEW-20260723-…`）列出的容量、告警/看板、support bundle、缓存风暴、
  多节点故障域演练、artifact publication 仍未关闭；`docs/standards-alignment.md:295-309` 也承认
  registry publication/签名/rollout/可观测性需要 Linux runner 与基础设施。
- 无 billing/entitlement/pricing 代码；用量计量存在 §6 的问题。
- **进展（2026-10-21）**：新增 [SLO.md](../../operations/SLO.md)，给出可用性（99.9%）、延迟、
  配置重载、证书续期的 SLI/SLO 与错误预算/燃烧率策略，并明确当前只有回环基线、首个生产月
  `/metrics` 才建立 SLO；support bundle 明确复用 `bin/doctor.sh --json/--export`、
  `bin/config.sh`、`bin/docker-deploy.sh logs --export`、`bin/backup.sh verify`，不再另造收集器。
- **仍未关闭**：billing/entitlement/pricing 归属（应由计费模块拥有，webserver 只产出用量事实）、
  目标机 100k/24h 证据、故障域演练与签名发布。

## 4. P1 高危问题（按维度）

### 4.1 安全
| # | 问题 | 证据 | 状态 |
| --- | --- | --- | --- |
| S1 | 租户 `deploy_app.nginx_conf` 绕过 SSRF/include 门禁直达共享 edge | `deploy_fallback.rs:54-58,334-348` → `deploy_nginx_sink.rs:64-70` → `edge-runtime/nginx.rs:71-97`；门禁只在 `service/nginx_ops.rs` | **已修复**：把扫描器下沉到 `core::scan_nginx_directive_risks`，在 `edge-runtime::validate_candidate_content` 对所有暂存配置强制执行；服务层复用同一实现 |
| S2 | bundled 模块网关用 loopback HTTP origins 覆盖主 CORS 白名单 | `bin/container/entrypoint-standalone.sh:1196` 导出 `SDKWORK_CORS_ALLOWED_ORIGINS=…localhost:3913`；`deployments/docker/env/production.env:162` | **已修复**：不再全局导出；loopback 白名单只作用于 embedded gateway 子进程，父进程环境在启动后立即恢复 |
| S3 | `demo` 不在 production-like 判定内，公共 demo 使用开发默认（派生加密密钥、可开 dev auth） | `core/src/runtime_env.rs:22-27,72-78`；`etc/topology/standalone.demo.env:8,13` | **部分修复**：`web_use_dev_inline_auth_resolver` 收紧为仅 `development/dev`，demo 即使设置 `DEV_AUTH_BYPASS=true` 也不启用 dev auth（新增测试）。加密密钥派生仍按非 production-like 处理，需部署为 demo 提供 `SECRET_ENCRYPTION_KEY`，已记录为部署要求 |
| S4 | 过期 TLS 证书仍加载并提供服务 | `data_plane/tls_material.rs:252-260`（“loading anyway”）、`tls_resolver.rs:209-229`（`select_any` 回退） | **已修复**：过期/未生效叶证书在激活时直接拒绝；测试改为断言拒绝 |
| S5 | 出厂数据库角色密码硬编码在 compose | `deployments/docker/docker-compose.yml:139-148`（`sdkworkprod123` 等） | **已修复**：test/staging/production 角色密码改为 `${VAR:?must be set}`，development 保留默认 |
| S6 | 生产环境仍会生成自签名占位证书 | `core/src/runtime_config.rs:490-568` | **已修复**：production-like（含 demo）缺材料直接启动失败，只允许 development 生成占位证书 |
| S7 | 私钥先按 umask 创建再 chmod，chmod 失败被忽略 | `acme-service/src/material_export.rs:49-67`；`runtime_config.rs:585-593` | **已修复**：两条路径都用 `OpenOptions.mode(0600)` 从首字节起创建、fsync、显式 chmod 并传播错误；ACME 导出增加随机 staging 名避免 pid 复用冲突 |
| S8 | 生产 ACME 联系邮箱为 `admin@localhost` | `etc/topology/standalone.production.env:56` | **已修复**：topology 与 bundle compose 默认改为 `admin@sdkwork.com`（production.env 原本已正确） |
| 已确认安全 | 静态路径逐组件 no-follow、server-files 租户隔离、机器凭证不回退用户解析、provider 事件 HMAC+时钟窗、上游 SSRF 网段策略、Git 导入 https/443+DNS pin、SQL 全参数绑定、无 cookie 会话（CSRF 不适用） | 见各审计证据 | — |

### 4.2 并发 / 生命周期 / 高可用
| # | 问题 | 证据 | 状态 |
| --- | --- | --- | --- |
| C1 | `limit_conn` 多规则只释放第一个 zone → 永久泄漏 → 503 至重启 | `data_plane/limit_conn.rs:60-93,134-153` | **已修复**（lease 持有全部 zone，回滚先释放锁再回滚，新增两规则回归测试） |
| C2 | TLS 快照发布未 fencing，旧投影可用更高 generation 覆盖新投影（可能重新提供已吊销证书） | `service/tls_material_distribution.rs:129-146,170-180`；`tls_runtime.rs:557-563` 接受更大 generation | **已修复**：跨进程 distribution lock 提前到**读取投影之前**获取，读取+写入同一临界区；并发发布者串行化，旧投影不可能覆盖新投影；锁在 blocking 线程内释放 |
| C3 | 代理缓存 single-flight 在响应头阶段释放 guard，正文填充是分离任务 → 无法合并，冷启动风暴 | `proxy.rs:429-464,545-582`；`cache/mod.rs:252-262` | **已修复**（guard 移入 tee 任务，存储完成才唤醒；等待上限 10s 后降级直连） |
| C4/M1 | 缓存填充 buffer 未按字节限额，4096 in-flight × 对象上限（可配 64 MiB）→ 理论数十/数百 GiB | `proxy.rs:548-550`；`cache/mod.rs:264`；schema `maxObjectBytes` 上限 64 MiB | **已修复**：进程级 fill 字节预算（`maxMemoryBytes/1KiB` 信号量），填充前按最坏大小预留；预算饱和时降级为纯流式、不缓存；新增单元测试 |
| C5 | Stream TCP listener 的 `JoinSet` 从不在 accept 循环中回收 → 已完成任务随“累计连接数”增长（单调内存泄漏） | `stream_proxy.rs:97-162`（无 `join_next` 分支） | **已修复**（增加 `joined = tasks.join_next()` 分支） |
| C6 | 集群宿主机/实例列表 O(n²) 子查询、事件列表无匹配索引 | `cluster.rs:107-118,177-183,785-796`；索引均以 `tenant_id` 开头 | **大部分修复**：迁移 0027 增加 host/instance/event 的 tenant-leading 索引；cluster 列表的 3 个相关计数子查询改为一次分组聚合 JOIN（并补上租户谓词）；host 投影内的 `instance_count` 相关子查询仍待改写 |
| C7 | 重载在 async worker 上做阻塞文件 I/O（证书/私钥读取、配置序列化） | `data_plane/runtime.rs:519-530,786-825`；`watch.rs:212` | **已修复**：`RuntimeGeneration::build` 移入 `spawn_blocking`，join 失败映射为 `ReloadWorker` |
| C8 | 关停 drain 预算被花两次：HTTP drain 无超时，tunnel 只能拿剩余 0s 并被判失败 | `data_plane/server.rs:397-453,913-928` | **已修复**：listener drain 使用同一绝对 deadline（`timeout_at`），超时报 `ListenerDrainTimeout` 并 abort 剩余任务；tunnel 使用同一 deadline 的剩余量 |
| C9 | ACME challenge store 在写锁内做 `remove_file` | `acme-service/src/challenge_store.rs:84-92,161-171` | **已修复**（先释放锁再 unlink；`publish_or_cleanup` 同样处理） |
| C10 | tunnel 网关证书读取失败静默降级为自签名 | `tunnel_bridge.rs:171-196` | **已修复**：证书 env/文件缺失改为 generation 构建失败（`TunnelConfiguration`）；仅 non-production 且未配置证书时才允许自签名；只配一半也失败 |
| C11 | `limit_req/limit_conn` 表在 generation 之外交换，重载窗口可能误判 503 | `runtime.rs:537-543`；`handler.rs:182,630,653` | **已修复**：两个准入表移入 `RuntimeGeneration`，请求使用其 pin 住的那一代的 zone 与连接表，重载不再跨代混用 |
| C12 | 管理监听器关停无 drain deadline | `main.rs:501-505` | **已修复**：`axum::serve` 包在 30s `timeout` 内，超时记录 warn 并退出，不再被单个挂起请求无限阻塞 |
| C13 | 用量计量无最终 flush，优雅重启丢失未闭合窗口（≤90s 计费事实） | `usage_metering.rs:350-364`；`server.rs:985-989` | **已修复**：新增 `flush_all()` 在关停时排空 open 窗口并 ingest；周期 flush 仍只排空已闭合窗口以避免重复窗口 |

### 4.3 内存 / 性能（OOM 风险）
| # | 问题 | 证据 | 状态 |
| --- | --- | --- | --- |
| M1 | 缓存填充未按字节限额（同 C4） | 同上 | **已修复**（同 C4/M1 行）：进程级 fill 字节预算 + 饱和降级流式 |
| M2 | Stream TCP 任务泄漏（同 C5） | 同上 | **已修复** |
| M3 | `tunnelMachines.list` O(设备 × 路由)，最多 10k × 10k，单请求可做上亿次克隆/排序 | `tunnel/src/service/mod.rs:274-282,327-347`；`registry.rs:531-542` | **已修复复杂度**：路由注册表每列表只快照一次并按 session 分组，降为 O(设备+路由)；设备数由 `maxDevices` 硬上限约束，响应契约保持 `{machines}`（分页需 breaking API，留待独立评审） |
| M4 | TLS 终结型 stream 每连接重建证书/`ServerConfig` 并做阻塞读盘 | `stream_proxy.rs:241-272,423-462`；`tls_material.rs:157-184` | **已修复**：`PreparedStreamListener` 启动时构建一次 acceptor（证书/客户端 CA 路径解析一次），配置错误在监听器准备阶段失败；每连接只做握手 |
| M5 | UDP stream 每个数据报全表 `retain` + 逐 session 互斥锁；每 session 65KB 缓冲 | `udp_stream_proxy.rs:79-95,136-140,263-265` | **部分修复**：全表回收改为按超时窗口限频（1–30s）；每 session 缓冲区池化待后续 |
| M6 | 应用域回退每请求深拷贝整份 descriptor | `deploy_fallback.rs:482-511,603-620` | **已修复**：缓存条目存 `Arc<Value>`，命中只做指针克隆；完整 JSON 仅在首次编译时克隆一次 |
| M7 | host 缓存克隆整个 HashMap 并在满时全量排序；WeChat 缓存同样模式 | `deploy_fallback.rs:176-229`；`wechat_verify.rs:108-131` | **已修复**：两个缓存改为加锁原地更新（不再每次插入克隆整张表）；仅在容量上限触发时做一次到期排序驱逐 |
| M8 | tunnel UDP 队列按消息数（256×65507 ≈16.7MiB/客户端）而非字节限界 | `tunnel/src/visitor/mod.rs:79,470-495,553` | **已修复**：每监听器 8 MiB 聚合字节预算（KiB 信号量），permit 随已写入的帧释放；预算耗尽的新数据报直接丢弃、不排队 |
| M9 | `limit_req` zone 满后每个新 IP 在锁内做 O(n) `retain`（10m zone ≈16 万 key） | `limit_req.rs:69-81,105-108` | **已修复**：空闲扫描按窗口限频（1–30s），饱和洪水不再每请求 O(n) |
| M10 | `sub_filter` 峰值约 2× 宣称预算（collect + replace 同时存在），超限返回 500 | `sub_filter.rs:36,50,100-130`；`core/config/sub_filter.rs:18-19` | **已修复**：预算按 2× 峰值预留（超限降级为不替换），聚合上限不再被瞬时翻倍突破 |
| M11 | ACME DNS HTTP 响应先 `collect()` 再检查 256 KiB 上限 | `acme-service/src/dns_http.rs:486-501` | **已修复**：逐帧读取并在越限前拒绝，不再整体 `collect` |
| M12 | 100k 连接/10k 路由 1s p99 无证据；路由重叠校验 O(n²)（最坏约 5000 万对） | `core/src/config/validate.rs:1909-1925`；PRD §8.1 | **已修复（本仓库部分）**：重叠校验改为按 `(pathType,path)` 索引，10k 路由编译 **349–438 ms → 33 ms**；10k 路由 1 s 预算达标。100k 连接仍需目标机内核预算 |

### 4.4 数据库与事务
| # | 问题 | 证据 | 状态 |
| --- | --- | --- | --- |
| D1 | 迁移改写（同 P0-6） | git 历史 | 待修 |
| D2 | PG 生命周期冒烟不可通过（同 P0-5） | 测试/框架 | **已修复**：真实 PostgreSQL 18.3 上 `postgres_lifecycle` 与 `repository_parity` 均通过；CI 以 disposable schema 运行 |
| D3 | 并发 nginx 激活可留下两条 active 配置（无站点行锁、无非唯一约束） | `nginx_configs.rs:296-367`；baseline `idx_webserver_nginx_config_site_active` 非唯一 | **已修复（双层）**：激活事务对 `webserver_site` 行 `FOR UPDATE` 串行化；迁移 0027 增加部分唯一索引 `uk_webserver_nginx_config_site_active` |
| D4 | `organization_id` 声明但未强制/未统一绑定（多处写 0 哨兵） | `backend.rs:197-205`；`platform_targets.rs:72-73` | **已收敛**：deployments/source_versions 已从 site 继承；platform target 改为从 application 继承（parity 断言=31）；剩余 0 仅出现在 backend 上下文没有 org claim 时创建 application，属框架/IAM claim 缺口而非本地硬编码 |
| D5 | 子表缺少 `(tenant_id, parent_id)` 复合外键 | baseline `:440,761,816,866,909,307-308` | **已修复**：迁移 0027 为 nginx_config/source_version/deployment/env_variable/health_check 增加站点复合外键，WeChat 增加根域复合外键；deployment→source_version 也改为租户绑定 |
| D6 | 租户表 `tenant_id … DEFAULT 0` 会掩盖漏绑定 | baseline `:18,78,423,787,893,939,985,1756,1801,1869` | **已修复**：10 张业务表移除默认值；平台表（audit、cluster 平面）保留 0 语义 |
| D7 | 应用 slug 唯一约束不感知软删除，删除后无法重建同名应用 | baseline `:1589`；`applications.rs:457-476` | **已修复**：迁移 0027 改为 `WHERE deleted_at IS NULL` 部分唯一索引 |
| D8 | `LIKE … ESCAPE '\'` 在 Rust 字符串中被转义成 `ESCAPE ''`，`%`/`_`/`\` 搜索失效 | `applications.rs:58,66`；`root_domains.rs:44,102`；`cluster.rs:593-595,629-631` | **已修复**：改为 `ESCAPE '\\'` |
| D9 | bootstrap advisory lock 连接长时间 idle-in-transaction，默认 60s 超时会在慢迁移中释放锁 | `database-host/src/lib.rs:37-54`；框架默认 60s | **已修复**：锁事务显式 `SET LOCAL idle_in_transaction_session_timeout = 0`（配合既有 10min lock_timeout） |
| D10 | 迁移头声明的 lock/statement timeout 从未实际 SET | `0006…up.sql:15-19` 等；orchestrator 只包 BEGIN/COMMIT | 待修 |
| D11 | 有损 down migration 被声明可逆（0013 会因现代 kind 行失败；0026/0023/0020 删除作者数据） | 各 `.down.sql` | 待修 |
| D12 | 所有拓扑 `SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID=0`，多副本可能生成重复 ID | `etc/topology/*.env`；`runtime.rs:27-39` | **已修复**：显式非 0 值优先；0/缺失改为从共享 `sdkwork_node_registry` 租约分配（心跳续租），多副本自动获得不同 node id；分配失败即启动失败，不再静默回退 0；reconcile 路径使用同一分配器 |
| D13 | source-version 保留清理在创建事务内无迭代上限，深 OFFSET O(n²) 并持站点行锁 | `source_versions.rs:167-258` | **已修复**：改为 `(created_at,id)` keyset 批次（含边界行），每请求最多 40 批；回归由 parity 测试覆盖 |
| D14 | `table-registry.json` 缺 `webserver_domain_wechat_verification`（34 vs 35 表） | registry / baseline | **已修复**：契约物化补齐 35 张表 |
| D15 | `database/README.md` 与 manifest/迁移树矛盾（`web`/`web_`、已删除的 0001–0004、“迁移为空”） | `database/README.md:5,11,24-31,38-44` | **已修复**：重写为当前 moduleId/prefix、baseline + 0005–0027、单一 Initialization state |
| D16 | 证书首页 `COUNT(*) OVER ()` 对整表过滤集做窗口计数 | `certificates.rs:53-98,885-915` | **已修复**：证书账本改为纯 keyset 语义，首页不再计数、不再返回 `totalItems`；`has_more`/`nextCursor` 保持精确 |
| D17 | `40001/40P01` 只映射为 503，没有整事务重试 | `support.rs:26-33` | **已修复**：`retry_transient_transaction` 对 `DatabaseUnavailable` 做 3 次有界、抖动退避重试；已接入 source-version 创建、deployment 创建、nginx 激活三个原子事务；冲突/校验错误不重试；3 个单元测试覆盖成功、非瞬时、耗尽 |
| D18 | WeChat 语句只按 `root_domain_id` 读写，缺租户谓词 | `domain_dns_records.rs:545-610` | **已修复**：SELECT/DELETE 均绑定 `tenant_id`，与 0027 的复合外键形成双层约束 |

### 4.5 API / SDK / 契约
| # | 问题 | 证据 | 状态 |
| --- | --- | --- | --- |
| A1 | operationId 动作违规（同 P0-2） | OpenAPI | **已修复** |
| A2 | SDK 全语言漂移（同 P0-3） | 生成器检查 | **已修复** |
| A3 | 游标首页不可续页（同 P0-4） | repo/response/routes | **已修复** |
| A4 | `tunnelMachines.list` 无分页、返回 `data.item.machines` 而非 `data.items+pageInfo`，上限 10k；PC 端伪造 `pageInfo` | `openapi.yaml:3834-3874`；`tunnel/service/mod.rs:274-282`；`pc-admin-core/src/index.tsx:177-187` | **部分修复**：列表新增 `page`/`pageSize` 与真实 `pageInfo`（offset、total、hasMore），PC 改为消费服务端 `pageInfo`；仍保留 `data.item.machines` 包装（改 `data.items` 是 breaking，需 v2） |
| A5 | certificates 已声明 cursor，但首页返回 `mode:"offset"`（客户端按 mode 分支会去请求被拒绝的 `page=2`） | `openapi.yaml:1825-1830`；`response.rs:259-280` | **已修复**：所有页统一 `mode:"cursor"`，首页保留 `totalItems`，续页省略；OpenAPI 描述同步 |
| A6 | server-files 目录列表最多 4096 条、无分页/`pageInfo`/≤100 例外 | `openapi.yaml`；`server-files-service/src/service.rs:32,181-184` | **部分修复**：目录列表新增 `truncated` 标志（第 4097 条探测），截断不再静默；仍无 `page`/`pageInfo`，分页需与 UI 同步设计 |
| A7 | webserver config catalog 最多 4096 条、无分页 | `openapi.yaml`；`config-service/src/service.rs:46,56,258` | **部分修复**：catalog 新增 `truncated` 标志（采集器允许第 4097 条探测），截断不再静默；分页仍未实现（部署配置文件数量有界，分页可与 UI 一起做） |
| A8 | `{domainId}` 路径参数声明为 `name: domain_id`，OpenAPI 模板非法，SDK 接受 `string|number` | `openapi.yaml:385,4782-4788`；生成的 `domain.ts:24` | **已修复**：`DomainIdParam` 改为 `name: domainId`，重新物化并再生成 14 个 SDK 目标 |
| A9 | 集群实例列表用 `search` 而非规范要求的 `q` | `openapi.yaml:3599-3604`；`cluster_routes.rs:67-69` | **已修复**：契约、路由反序列化字段、SDK 全部改为 `q` |
| A10 | DNS 账号列表先全量加载再 `skip/take`（进程内分页） | `service/backend.rs:1465-1489` | **reviewed exception**：账号来自启动期 issuer registry（有界配置集合，不是 DB 查询）；分页只是稳定切片，`total` 为真实条数，无 DB 全表读。已在 §52 记录 |
| A11 | 游标未绑定操作/资源/租户，跨列表复用不会失败 | `support.rs:107-155` | **已修复**：游标签名载荷加入 `scope`（`<operation>:<tenant>`，audit/cluster 用固定平台作用域）；解码要求作用域完全匹配，否则 400；单元测试覆盖跨操作/跨租户拒绝，parity 覆盖跨租户重放 |
| A12 | H5/小程序/PC sandbox 把 int64 `totalItems` 解析为 JS `number` | 各 `pagination.ts`、`sandbox-instances-model.ts:119-123` | **已修复（H5/PC）**：H5 `WebserverH5ListPage.totalItems` 改为精确十进制 `string`（不转 number）；PC sandbox 对超 `MAX_SAFE_INTEGER` 的值显式钳制而非静默舍入，并新增边界测试。小程序/Flutter/Harmony 根无此分页模型 |

### 4.6 前端 / 客户端
| # | 问题 | 证据 | 状态 |
| --- | --- | --- | --- |
| F1 | 插件目录存于 `localStorage`，却以“工作区级”管理面呈现；跨用户审核不可能 | `console-plugins/src/plugin-catalog.ts:20-52`；`MyPluginsPage.tsx:36,72,248`；i18n 文案 | **部分修复 / 切换受阻**：`webserver_plugin` 表、DTO、repository、port、backend HTTP API、OpenAPI/SDK 与 PG parity 已完成；但 PC 插件页属 **app-console**，按 `apps/sdkwork-webserver-pc/AGENTS.md` 只能消费 app-api SDK，禁止在 console 面接 backend SDK。需要 webserver 的 app-api facade（或 deployments app 扩展）后才能切换，属跨仓库 API 归属决策 |
| F2 | H5 无 IAM 登录/刷新/登出；非 development 构建无法认证 | `h5/src/App.tsx`；`h5 bootstrap/runtime.ts:76-97`；`h5-core/src/session/session.ts:113-176` | 待修 |
| F3 | 主机详情只读第一页 50 条，超过 50 的主机显示 PENDING/SSL off/“-” | `ServedDomainAdminSurface.tsx:75,2317-2343` | **已修复**：新增 `GET /backend/v3/api/domains/{domainId}`（`domains.retrieve`，`web.applications.read`），详情页按 id 读取；SDK 重新生成，repository parity 覆盖按 id 读取与跨租户 404 |
| F4 | Console 路由守卫对 `app-console` 恒真，声明权限不生效 | `pc-commons/src/permissions.ts:83-92`；`WebserverWorkspace.tsx:121-129` | **已修复**：两个 surface 都按 entry 声明权限过滤/守卫；`app_user` 的 IAM 扩展同步补齐 console 入口所需权限（`deploy.*`、`skills.marketplace.read`、`mcp.marketplace.read`、`web.plugins.read`、`iam.provider_accounts.read`），否则启用守卫后 app_user 会失去 console。权限清单变更需人工评审 |
| F5 | Console 页面通过 backend-admin SDK 调 backend API（surface 边界违规） | `WebserverAuthorizedWorkspace.tsx:282-288`；`traffic-usage-client.ts:2-6,44-60` | 待修 |
| F6 | 小程序/Flutter/Harmony 根固定“未认证”，能力永远不可达 | 各 bootstrap/auth gate | 待修 |
| F7 | 无 route/page 级 error boundary；`use(...)` 拒绝会白屏 | `App.tsx:94-101`；`WebserverAuthorizedWorkspace.tsx:220` | **已修复**：App 路由树外包 `RouteFailureScope`（按 pathname 重置的 class error boundary），lazy 路由/渲染拒绝显示本地化“工作台加载失败”而非白屏 |
| F8 | 交互式选择器 `readAllPages`（10×200）、DNS/账号摘要单页 200 条截断 | `ServedCertificateAdminSurface.tsx:64-77,732,755`；`ServedDomainAdminSurface.tsx:1004-1025`；`cloudAccounts.ts:40-77` | **已修复**：`readAllPages` 返回截断标记并在主机名选择器显示上限提示；DNS 解析概要改为按页遍历（最多 4,000 条）并在截断时显示提示；账号清单来自部署配置、规模有界 |
| F9 | 无 ESLint/Biome 配置与 `lint` 脚本；`useEffect` 依赖已不完整 | root/PC package.json；`WebserverWorkspace.tsx:445-447` | **已修复**：新增 ESLint 10 + typescript-eslint + react-hooks 配置，`pnpm lint` 以 `--max-warnings 0` 运行并纳入 `_sdkwork:check`；修复全部 15 处 exhaustive-deps 警告（补 `t`/`runtime`/`selectedValues`、移除多余 `page`、对不稳定函数依赖显式说明并禁用） |
| F10 | 原始后端/供应商错误文案直出；空 catch 吞错 | `ClusterOverviewSurface.tsx:64`；`cloudAccounts.ts:75-77`；`App.tsx:194` | **部分修复**：cluster 概览改用共享 `formatWebserverErrorMessage`（本地化 + 脱敏 + 截断）；auth bootstrap 失败不再空 catch 丢弃，写入 console；其余空 catch 待清理 |

### 4.7 虚假实现 / PRD 偏差
| # | 问题 | 证据 | 状态 |
| --- | --- | --- | --- |
| P1 | Nginx 导入静默接受约 120 个行为指令（超时/缓冲/日志等），无诊断 | `core/src/nginx/mapping.rs:48-169,343`；`server_toml.rs:245-250,418` | **已修复（导入面）**：新增 `ignored_directive_diagnostics`，每个被接受但未执行的指令产生带 file:line 的 `ConfigDiagnostic`，经 `NginxLoadReport.ignored` / `LoadedWebServerConfig.compatibility` 暴露，并在加载与 `validate-nginx` 输出；含回归测试。typed TOML 面仍待同样处理 |
| P2 | nginx 关闭时 validate/reload/status 返回成功（无效果） | `edge-runtime/nginx.rs:99-105,169-172,195-201`；`backend.rs:1545-1616,1631-1638` | **已修复（HTTP 面）**：service 层新增 `require_nginx_edge()`，validate 返回 `valid:false`，deploy/reload/status 返回 503；node-sync 内部 no-op 语义保持不变 |
| P3 | nginx “修订”原地编辑，`version_no` 从未写入，回滚目标是当前行内容 | `nginx_configs.rs:227-232`；`backend.rs:2142-2157` | **部分修复**：快照表 + `version_no` 递增；`GET .../revisions`、`GET .../revisions/{versionNo}`、`POST .../revisions/{versionNo}/rollback` 后端 API/OpenAPI/SDK 全部就绪；回滚以前向写入新 revision。**PC 修订历史/回滚 UI 未实现** |
| P4 | 管理 UI 移除 Nginx/Servers/Server Files/Config/Diagnostics 菜单，API 与 PRD FR-013/FR-028 仍承诺 | `WebserverAuthorizedWorkspace.tsx:60-70`；`http_route_manifest.rs:312-419`；`PRD.md:169,184` | **product decision pending**：移除是代码中有明确理由的决策（单节点控制面被 cluster 平面取代），但 PRD-FR-013/FR-028 仍承诺这些管理面，二者矛盾不能由实现单方消除。需产品确认“保留 API + cluster 取代 UI”还是“恢复菜单”，再同步 PRD 或 UI |
| P5 | FR-018 仅进程级 governor；per-app/host/route 预算不存在 | `TECH_ARCHITECTURE.md:126-131`；`model.rs:630-700` | 待修 |
| P6 | `gzip_static`、`proxy_buffering`、`proxy_request_buffering`、`proxy_next_upstream` 被接受但忽略 | `mapping.rs` `ACCEPTED_IGNORED` | **reviewed exception**：这些指令属于“接受但不执行”的兼容集合，**不是静默忽略**——`walk` 为每条生成 `ConfigDiagnostic`（“accepted for nginx compatibility … not enforced … has no effect”），导入报告会带出；新增测试固定该告警。运行时的 buffering/重试语义由运行时策略决定，若要按指令执行需独立实现 |
| P7 | Tunnel Machines 依赖未声明的 `web.tunnel.*` 权限与无人设置的 env（调用恒 503） | `iam.module.manifest.json`；`tunnel_machines.rs:35-37,62-107` | **部分修复**：IAM 目录新增 `web.tunnel.read/write`；环境变量契约写入 `etc/README.md`（需同时配置 operations URL 与 bearer token 文件，否则按设计 503）。默认 profile 不设置 token 文件（宿主 secret），属部署配置而非代码缺口 |
| P8 | 匿名审计事件写 tenant 0，与平台运营租户概念混淆的风险 | `docs/standards-alignment.md:93-99`；audit 仓储 | 待确认 |

### 4.8 测试 / CI
| # | 问题 | 证据 | 状态 |
| --- | --- | --- | --- |
| T1 | 唯一 CI 工作流无独立 test job；测试只在打包 validate 中执行，且 validate 已红 | `.github/workflows/package.yml`；`sdkwork.workflow.json:202-248` | **reviewed exception**：`sdkwork.workflow.json` 的 validate 阶段包含 `pnpm verify`（`_sdkwork:test` 全量 Rust + 契约 + IAM 模板）与 `git diff --exit-code`；独立 test job 会被 GITHUB_WORKFLOW_SPEC 的薄入口规则拒绝。validate 的本地等价命令已全绿（Windows 宿主仅 shell-portability 由 Linux 执行），不再是“已红” |
| T2 | PG 必需门禁排在已失败的 `pnpm verify` 之后，从未执行 | `sdkwork.workflow.json:226,231` | **已修复**：DB/恢复/HA 前移；lifecycle 与 parity 已在真实 PG 18.3 上通过 |
| T3 | HA/恢复“真实演练”测试只对脚本源码做正则匹配 | `tests/contract/postgres-ha.contract.test.mjs:41-52`；`database-recovery.contract.test.mjs:43-53` | 待修 |
| T4 | 事务/租户隔离/幂等/keyset/回滚全部压在 1 个 3,700 行 `#[ignore]` 测试里 | `repository_parity.rs:172-174` | **已执行通过**；为可定位失败补充了带幂等键的错误上下文，并修复了 4 处过期 fixture（续期 lead 时间依赖、未验证域名、重复指纹、非 hex 标记） |
| T5 | 每个路由的未认证/无权限 HTTP 契约测试缺失（仅 manifest 元数据断言） | `route_manifest_contract.rs`；`internal_routes.rs:299-316` 仅 internal | **部分修复**：新增 HTTP 级测试 `backend_framework_rejects_unauthenticated_requests`，无凭据请求在双 token surface 上必须返回 401；manifest 契约继续覆盖授权模式/权限。逐路由的参数化 HTTP 矩阵仍未建立 |
| T6 | ACME pebble E2E `#[ignore]` 且无 runner | `pebble_lifecycle.rs:326-328,420` | **部分修复**：新增 `pnpm test:acme:pebble`（`tools/run-pebble-e2e.mjs`），校验 `PEBBLE`/`CHALLTESTSRV` 或 PATH 后执行 ignored 测试；缺失二进制时 exit 69 并给出下载地址，不再静默跳过。本机无 Pebble，成功路径未执行 |
| T7 | 真实 nginx 测试在缺二进制时静默跳过（CI 未装 nginx） | `edge-runtime/nginx.rs:586-591` | **reviewed exception**：跳过**不静默**——`eprintln!("nginx unavailable; real edge validation evidence skipped")` 明确记录；可用 `SDKWORK_WEBSERVER_TEST_NGINX_BINARY` 或 PATH 提供二进制。CI 镜像未装 nginx，真实 nginx 证据仍需带 nginx 的 runner |
| T8 | 12 个已定义门禁不可达（nginx corpus/differential、deploy、adaptive-web 等）；clippy 从不运行 | `package.json` 脚本图 | **部分修复**：`check:rust-clippy` 已覆盖 **8 个 crate（含 gateway lib+bins）** 并接入 `_sdkwork:check`，八个 crate 告警清零（gateway 通过 crate 级 allow 处理 `result_large_err`/`too_many_arguments`/`type_complexity` 并修复其余项）。nginx corpus/differential、deploy、adaptive-web 门禁仍缺执行入口 |
| T9 | 测试全局环境依赖宿主 `SDKWORK_ENVIRONMENT`：宿主为 production 时 `web_bootstrap` 测试 panic | 复现：清除/设为 development 后通过 | **已修复**：`web_bootstrap` 测试在构建框架层时临时把环境钉为 `development` 并在构建后恢复，宿主为 production 也能通过（13/13） |
| T10 | Windows 下 nginx include/glob 测试失败（路径转义语义） | `core/src/nginx/parser.rs`；`tests/nginx_config_surface.rs`；`tests/nginx_parser_edge_cases.rs` | **已修复**：nginx 把 `\` 当转义符，测试却把原生 Windows 路径写进指令。测试改为正斜杠路径（含期望值），nginx 语义不变；core lib 250/250、surface 14/14、edge 13/13 |

## 5. 分页专项（PAGINATION_SPEC）

**门禁结果**：`check-pagination.mjs --workspace .` → 通过（exit 0）；`check-api-response-envelope.mjs` →
通过。但门禁未覆盖以下违规，故“通过”不等于合规：

1. **cursor 首页不可续页**（P0-4，已修复）。
2. **非分页列表**：`tunnelMachines.list`（≤10k，`data.item.machines`）、server-files 目录（≤4096）、
   webserver config catalog（≤4096）——均超出 §11 的 ≤100 bounded-by-design 例外且无 pageInfo。
3. **进程内分页**：DNS 账号列表 `skip/take`（§2/§5.2）。
4. **客户端全量/截断**：`readAllPages`（10×200）、主机详情单页 50、DNS 摘要单页 200、账号选择器 200、
   插件目录 `localStorage + mode:"client"`（§7/§8）。
5. **mode 不一致**：certificates 首页 offset、续页 cursor（§3 要求每个操作模式稳定）。
6. **游标不绑定操作/租户**（§3 的可信续页语义）。
7. **`search` 而非 `q`**、`{domainId}` 参数名错误（API_SPEC §14.1.1/§5.1）。

**结论**：分页“形式合规、语义不完整”。修复优先级：P0-4（已修）→ 非分页列表 → 前端续页 →
mode/参数名/游标绑定。

## 6. 数据库专项（PostgreSQL / SQLite）

### 6.1 引擎结论
- **SQLite 未实现，且按设计不应实现**：`database.manifest.json` `engines:["postgres"]`、Cargo 仅启用
  `sqlx/postgres`、仓储只实例化 PostgreSQL；PRD §4.1、REQ-2026-0004 明确单一权威引擎。
- 残留的 SQLite 字样仅为：框架传递依赖、sibling Canvas/IM 模块的容器变量、`admin_reset` 的拒绝
  文案。**不存在“SQLite 半实现”或文档虚假宣称。**

### 6.2 PostgreSQL 完整度
- 已实现：35 张表、22 个迁移、baseline 与迁移对象集一致、参数化 SQL、共享进程池、租户谓词、
  keyset+HMAC 游标、幂等键、证书租约/fencing/SKIP LOCKED、行级 `FOR UPDATE`、`SKIP LOCKED` 清扫。
- 实质缺陷：迁移不可变性（P0-6）、生命周期测试不可通过（P0-5）、nginx 激活竞态（D3）、
  organization 未强制（D4）、复合租户外键缺失（D5）、tenant 默认 0（D6）、软删除唯一性（D7）、
  LIKE 转义（D8，已修）、advisory lock 空闲超时（D9）、迁移 timeout 未生效（D10）、有损 down（D11）、
  snowflake 节点 0（D12）、保留清理无界（D13）、registry/README 漂移（D14/D15）、
  证书首页窗口计数（D16）、无 40001/40P01 重试（D17）、WeChat 语句缺租户谓词（D18）。

## 7. PRD / TECH 与实现偏差（精选）

| 需求 | 状态 | 证据 |
| --- | --- | --- |
| FR-007 异步命令 | 部分：仅证书签发/续期 202+durable operation | `routes.rs:934-946`；`certificate_operations.rs:179-213` |
| FR-008 不可变修订/回滚 | 部分/缺失：证书/源版本不可变；nginx 原地编辑；部署无执行器 | P0-7、P3 |
| FR-009 Nginx 诊断 | 不满足：约 120 指令静默忽略 | P1 |
| FR-011 分页 | 形式实现，语义缺陷 | §5 |
| FR-012 PostgreSQL 门禁 | 声明实现，实际不可执行 | P0-5、T2 |
| FR-013/FR-028 管理面 | 后端在、UI 菜单被移除 | P4 |
| FR-018 分层 governor | 仅进程级 | P5 |
| FR-020 reload/升级 | reload 有证据；零停机可执行升级缺失 | `TECH-runtime-data-plane.md:212` |
| FR-026 真实部署/回滚 | 缺失 | P0-7 |
| FR-029 审计 | 多个运行时敏感变更无审计（nginx、cluster、server-files、config、WeChat） | `backend.rs:1556-1616`；`cluster_ops.rs`；`server_files_routes.rs`；`config-service/service.rs` |
| FR-030 平台运营租户 | 已实现 | `runtime_env.rs:100-124`；`auth.rs` |
| FR-036–FR-040 域/证书 | 已实现 | baseline 约束、`certificate_operations.rs` |
| 100k 连接 / 24h soak / 10k 路由 1s | 未满足/无证据 | P0-9 |
| TECH “cache/cluster rollout 未实现” | 文档过时：代码已实现 cache 与 cluster（但收敛仍不可达） | `TECH-runtime-data-plane.md:212` vs `data_plane/cache/*`、`cluster_ops.rs` |
| standards-alignment “pnpm verify/门禁通过” | 与当前事实不符（审计开始时门禁红） | `docs/standards-alignment.md:295-309` |
| PRD 的 app-api 面 | 已退役；实际只有 backend-api/internal-api + Deploy app SDK | `apis/`；`generate_web_sdks.mjs:13-16` |

## 8. 商业化落地能力评估

| 维度 | 评级 | 说明 |
| --- | --- | --- |
| 核心 HTTP/TLS/代理数据面 | B+ | 流式、有界、协议控制细致；但 100k/soak 无证据，缓存填充/stream 任务/TLS acceptor 有 OOM 风险 |
| 证书生命周期 | B | 202+租约+fencing+ACME+DNS-01+热加载齐全；过期证书仍服务、发布未 fencing、pebble E2E 未跑 |
| 多租户/IAM | B | 租户谓词、平台运营租户、机器凭证隔离较完整；organization 维度未强制、console 守卫为空 |
| 持久化/迁移 | C+ | 表/索引/事务设计较好；迁移不可变性、生命周期冒烟、竞态、默认 tenant 0 是发布级问题 |
| API/SDK 契约 | B- | envelope/int64/分页门禁较完善；cursor 首页（已修）、非分页列表、参数名、SDK 漂移（已修） |
| 运维/可观测 | C | 有 metrics/alerts 切片；缺 SLO 看板、support bundle、容量/故障演练、artifact 证明 |
| 高可用/集群 | D+ | 有 drain/heartbeat/fencing 基础；同步收敛不可达、k8s replicas=1、无多节点演练 |
| 商业化（计费/权益/合规） | D | 无 billing/entitlement/pricing；用量计量不完整；无 GDPR 导出/删除链路 |
| 交付/供应链 | C | 有 deb/rpm/tar、SBOM/签名脚本；CI 红、installer 未覆盖 SBOM/签名、无 CODEOWNERS/dependabot |
| 文档真实性 | C- | PRD/TECH/review 多处与实现不一致；数据库 README 过时；门禁“通过”声明过期 |

**结论**：可作为**技术预览/单节点自托管**交付；**不具备商业化 GA 条件**。最低 GA 门槛：P0-1…P0-9
全部关闭 + P1 安全项（S1–S6）+ 至少一次真实 100k/24h soak 与三节点故障演练 + 计费/权益最小闭环。

## 9. 改进方案

### Phase 0（1–2 周，恢复“可验证”）——发布阻断
1. 修复 P0-2/P0-3/P0-4 后跑通 `pnpm verify`（本次已完成前三项，需补 CI 证据）。
2. 修正 PG 生命周期测试（P0-5），把 `test:postgres:required` 前移到 `pnpm verify` 之前或并入同一 job；
   `cargo test --workspace` 不再静默跳过 DB 测试（`--include-ignored` 或独立 job）。
3. 迁移冻结策略落地（P0-6）：CI 对每个 tracked `.up.sql` 与 git blob 做哈希比对；受影响库提供对账 runbook。
4. 部署执行器决策（P0-7）：实现或从契约/UI/PRD 移除；在实现前禁止“发布成功”状态。
5. 启动 node 侧 `cluster_member` 或移除收敛声明（P0-8）。
6. 修复 S1（已完成）、S2–S6；移除 compose 默认密码、生产占位证书、`admin@localhost`。
7. 验收：CI 绿、6 个打包 job 产出 artifact 并完成 attestation；`pnpm verify` + `test:postgres:required`
   + recovery + HA 全部真实执行。

### Phase 1（4–6 周，稳定性/容量）
1. 缓存填充字节信号量 + 禁止按不可信 Content-Length 预分配（C4/M1）；`maxObjectBytes ≤ maxMemoryBytes` 校验。
2. TLS 快照发布按 DB 投影版本 fencing（C2）；重载移入 `spawn_blocking`（C7）；统一 shutdown 绝对 deadline（C8）。
3. stream TLS acceptor 按 generation 预构建（M4）；UDP 回收移出数据报路径（M5）；tunnel 列表建索引 + 分页（M3/A4）。
4. `limit_req` 空闲索引、sub_filter 预算/降级、ACME DNS 有界读取（M9–M11）。
5. 数据库：nginx 激活加站点行锁 + 部分唯一索引（D3）；organization 绑定或移除（D4）；复合租户外键（D5）；
   去掉 tenant 默认 0（D6）；软删除唯一索引（D7）；advisory lock 关闭 idle-in-transaction 超时（D9）；
   迁移 timeout 生效（D10）；snowflake 节点分配（D12）；保留清理 keyset+上限（D13）。
6. 分页：非分页列表补 page/cursor + `SdkWorkPageData`；certificates 统一 cursor；`q`/`domainId` 改名；
   游标绑定操作+租户；前端全部改为服务端续页。
7. 验收：100k 空闲连接 24h soak（RSS 曲线、无单调增长）、10k 路由 1s p99、三节点故障域演练。

### Phase 2（1 个季度，商业化）
1. 计费/权益最小闭环：usage ingest 幂等、窗口校验、账单对账；计量补 egress 实际字节与重启 flush。
2. SLO 看板/告警/support bundle/runbook 全覆盖；k8s `replicas ≥ 2` + HPA + PDB 验证。
3. 供应链：installer 也走 SBOM/签名；actions SHA pin；CODEOWNERS/dependabot；artifact attestation 证据。
4. 管理面：恢复 Nginx/Servers/Config/Diagnostics 或删除对应 API；审计覆盖所有运行时敏感变更（FR-029）。
5. 合规：数据导出/删除、日志脱敏、密钥轮换、渗透测试与威胁建模。
6. 验收：商业化 GA 检查表逐项有证据；第三方安全评审通过。

## 10. 本次已修复项与验证

| 修复 | 文件 | 验证 |
| --- | --- | --- |
| API operationId 动作合规 | `apis/backend-api/web/openapi.yaml`、`http_route_manifest.rs`、PC 调用点、物化/SDK | `check-api-operation-patterns` exit 0；`materialize --check` current；`generate_web_sdks --check` 14 targets current |
| 全语言 SDK 再生成 | `sdks/**`（78 文件变更） | 同上；`check-sdk-standard` 通过 |
| 游标首页可续页 | `deployments.rs`、`source_versions.rs`、`servers.rs`、`response.rs`、`routes.rs` | `cargo check` 通过；keyset 测试保留；`check-pagination` 通过 |
| `LIKE … ESCAPE` 修正 | `applications.rs`、`root_domains.rs`、`cluster.rs` | 静态核对；SQL 语义由 `ESCAPE '\\'` 恢复（需 PG 集成测试补充） |
| `limit_conn` 多 zone 释放 | `limit_conn.rs` + 2 个回归测试 | 待 gateway 测试完成（构建中） |
| 缓存 single-flight guard 生命周期 | `proxy.rs`（guard 移入 tee 任务 + 10s 上限） | `cargo check`（构建中）；行为需集成测试 |
| Stream TCP 任务回收 | `stream_proxy.rs` 增加 `join_next` 分支 | 编译通过（构建中） |
| ACME challenge 锁外 unlink | `challenge_store.rs`（`clear_token`/`publish_or_cleanup`/`clear_generation_and_file`） | `cargo check`（构建中） |
| Nginx 激活统一门禁 | 新增 `core/src/nginx_activation.rs`；`edge-runtime/nginx.rs`；`service/nginx_ops.rs` 去重 | 单元测试随 core 编译；服务层测试通过 |
| 文档同步 | `docs/standards-alignment.md`、`TECH-runtime-data-plane.md` 的状态声明 | 见下 |

> 注：Rust 全量 `cargo test --workspace` 在 Windows 上仍受 3 个 nginx glob 测试与宿主
> `SDKWORK_ENVIRONMENT=production` 影响（见 T9/T10），与本次修复无关。

## 11. 附录：验证命令

```powershell
cargo check --workspace --all-targets
cargo test -p sdkwork-webserver-core --lib
cargo test -p sdkwork-routes-webserver-backend-api --lib
cargo test -p sdkwork-routes-webserver-common --lib
cargo test -p sdkwork-webserver-acme-service --lib
cargo test -p sdkwork-webserver-edge-runtime --lib
node ..\sdkwork-specs\tools\check-api-operation-patterns.mjs --root .
node ..\sdkwork-specs\tools\check-api-response-envelope.mjs --workspace .
node ..\sdkwork-specs\tools\check-pagination.mjs --workspace .
node ..\sdkwork-specs\tools\check-sdk-standard.mjs --workspace .
node tools\materialize_web_phase1_contracts.mjs --check
node tools\generate_web_sdks.mjs --check
pnpm --dir apps/sdkwork-webserver-pc typecheck
```

**结论重申**：本仓库的工程基础（有界数据面、证书生命周期、契约门禁、生成式 SDK）明显高于一般项目，
但“契约/证据/运营”三条链路尚未闭合。在当前状态下，任何“生产就绪/商业化可用”的对外声明都缺乏
证据支撑；按 §9 的 Phase 0 完成后才具备可发布性，Phase 1/2 完成后才具备商业化 GA 能力。

## 12. 第二轮修复（2026-10-21）

继首轮修复后，按“先关闭可验证的 P0/P1、再清理债务”的顺序完成：

| 修复 | 文件 | 验证 |
| --- | --- | --- |
| PostgreSQL 生命周期测试对齐框架语义（baseline + 全部迁移 = fresh install） | `crates/sdkwork-webserver-database-host/tests/postgres_lifecycle.rs` | 测试逻辑编译通过；断言 pending 非空 → migrate 全部 → pending 空 → drift clean |
| CI validate 顺序：DB/恢复/HA 前移，避免被无关失败掩盖 | `sdkwork.workflow.json` | JSON 结构校验通过 |
| Nginx 接受但未执行的指令产生显式诊断（含 file:line） | `core/src/nginx/mapping.rs`、`core/src/nginx/load.rs`、`core/src/config/source.rs`、gateway `main.rs` | 新增 `accepted_but_unenforced_directives_are_reported`；`cargo test -p sdkwork-webserver-core --lib nginx::load` 2/2 通过 |
| nginx edge 关闭时不再在 HTTP 面伪造 validate/reload/status 成功 | `service/src/nginx_ops.rs`、`service/src/backend.rs` | service 46/46 通过 |
| 过期/未生效 TLS 叶证书拒绝激活 | `data_plane/tls_material.rs` | 测试改为断言拒绝；4/4 通过 |
| bundled 网关 CORS 白名单只作用于子进程 | `bin/container/entrypoint-standalone.sh` | 代码审查；生产默认 `docker` 模式不受影响 |
| test/staging/production 数据库角色密码必须显式提供 | `deployments/docker/docker-compose.yml`、`docker-compose.platform-api-gateway.embedded.yml` | 8 处默认值移除；`${VAR:?}` 由 compose 强制 |
| 生产环境禁止自签名占位证书 | `core/src/runtime_config.rs` | `runtime_config` 18/18 通过（development 仍生成占位证书） |
| 私钥文件 0600 先创建后写入、chmod 失败不再被忽略 | `core/src/runtime_config.rs` | 编译通过；`runtime_config` 测试通过 |
| bootstrap advisory lock 显式关闭 idle-in-transaction 超时 | `database-host/src/lib.rs` | 编译通过 |
| 生产 ACME 联系邮箱不再使用 localhost | `etc/topology/standalone.production.env`、`docker-compose.bundle.yml` | 配置审查 |

仍未关闭（下一轮）：D3 的数据库部分唯一索引、D4 organization 强制、D5 复合租户外键、
D6 tenant 默认 0、D7 软删除唯一索引、D12 snowflake 节点分配、A4/A6/A7 分页契约、
C2/C4/C7/C8/C10/C13、M3–M11、F1–F10、S3。这些项已按 §9 Phase 0/1 排序，不得在关闭前宣称 GA。

## 13. 第三轮修复（2026-10-21）

| 修复 | 文件 | 验证 |
| --- | --- | --- |
| nginx 并发激活串行化（同一站点父行 `FOR UPDATE`）+ active 读取确定性排序 | `repository-sqlx/src/nginx_configs.rs` | 编译通过；repository 单元测试通过 |
| certificates 分页模式统一为 cursor（首页保留 total，续页省略） | `routes-common/src/response.rs`、`apis/.../openapi.yaml` | operation/envelope/pagination 门禁通过 |
| 路径参数 `{domainId}` 名称修正（OpenAPI 模板合法性） | `apis/.../openapi.yaml`（`DomainIdParam`） | 物化 + 14 SDK 目标再生成并通过 `--check` |
| 集群实例自由文本检索参数 `search` → 规范 `q` | `apis/.../openapi.yaml`、`routes-backend-api/src/cluster_routes.rs` | SDK 再生成；PC typecheck 通过 |

累计验证：`cargo check --workspace --all-targets` 通过；routes-common 21/21、routes-backend 13/13、
repository 单元 5/5；PC `tsc` 通过；API operation/envelope/pagination/SDK/物化门禁全部通过。

## 14. 真实 PostgreSQL 验证（2026-10-21）

在 WSL PostgreSQL 18.3 上创建两个 disposable 数据库与独立 schema（`sdkwork_ai_test_audit` /
`sdkwork_ai_test_parity`，owner `sdkwork_ai_test`），执行了此前从未真正跑过的强制门禁：

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| baseline + 22 迁移 + seed + drift | `cargo test -p sdkwork-webserver-database-host --test postgres_lifecycle -- --ignored --exact` | `1 passed`，exit 0 |
| Repository parity（事务/租户/幂等/keyset/回滚/证书/集群） | `cargo test -p sdkwork-intelligence-webserver-repository-sqlx --test repository_parity -- --ignored --exact` | `1 passed`，exit 0 |

过程中修复的测试 fixture（产品规则本身正确，不应放宽）：

1. 续期调度测试依赖固定的 `2027-01-01` 到期时间与默认 30 天 lead，跨日期后会失效；改为在证书
   metadata 写允许的最大 lead（90 天），不依赖墙钟。
2. certificate keyset fixture 的域名未做归属验证，而产品已要求“已验证域名”才能签发；补
   `verify_site_domain_with_evidence`。
3. 三个 keyset 证书共用一个指纹，触碰 `uk_webserver_certificate_version_fingerprint`；改为每张
   证书独立指纹。
4. 指纹/序列号必须是小写 hex（`chk_webserver_certificate_version_hashes`），fixture 曾用大写
   占位符；改为 `b/c/d`。
5. `enqueue_and_claim_certificate` 的错误改为携带幂等键，失败时能直接定位到具体 fixture。

这两项通过意味着：迁移链可重放、baseline 与迁移组合可收敛、drift 干净，且仓储层的事务、租户隔离、
幂等、keyset 分页、回滚和证书生命周期在真实 PostgreSQL 上成立。

## 15. 第五轮修复：租户完整性与激活唯一性（迁移 0027）

新增 `database/migrations/postgres/0027_webserver_tenant_integrity_and_active_indexes.{up,down}.sql`，
并把同一 DDL 合并进 pre-launch baseline（baseline 仍是完整快照，0027 是已有库的前向升级）：

1. **D6**：10 张业务表 `tenant_id` 移除 `DEFAULT 0`；audit/cluster 平台表保留 0 语义。
2. **D5**：nginx_config / source_version / deployment / env_variable / health_check 的站点外键改为
   `(tenant_id, site_id) → webserver_site(tenant_id, id)`；deployment→source_version 同样租户绑定；
   WeChat 根域外键改为 `(tenant_id, root_domain_id)`；source_version 增加 `(tenant_id, id)` 唯一键。
3. **D7**：应用 slug 改为 `WHERE deleted_at IS NULL` 的部分唯一索引，删除后可复用。
4. **D3**：`uk_webserver_nginx_config_site_active` 部分唯一索引，数据库层保证每站点至多一个 active。
5. **D14（性能）**：cluster host/instance `(tenant_id, updated_at, id)`、event
   `(tenant_id, occurred_at, id)` 索引。
6. **D18**：WeChat 读/删语句绑定 `tenant_id`。
7. **D13**：source-version 保留清理改为 keyset 批次（含边界行），单请求最多 40 批，避免深 OFFSET 与
   长时间持锁。
8. **D14/D15**：table-registry 补齐 35 张表；database README 重写为当前事实（moduleId `webserver`、
   prefix `webserver_`、baseline + 0005–0027、单一 Initialization state）。

验证（真实 PostgreSQL 18.3，重置独立 schema 后）：

| 项 | 结果 |
| --- | --- |
| `postgres_lifecycle`（baseline + 23 迁移 + seed + drift） | `1 passed`，exit 0 |
| `repository_parity`（含保留清理、复合外键、部分唯一索引） | `1 passed`，exit 0 |
| repository 单元 / service 单元 | 21/21、46/46 |
| `check-database-framework-standard` | passed |

过程中发现并修正：drift 对索引列方向/谓词的规范化差异（契约条目去掉 DESC、与实际 DDL 对齐）；
keyset 保留边界需包含边界行（`<=`），否则少清理一条。

## 16. 第六轮修复：缓存字节预算、阻塞重载与关停预算（2026-10-21）

| 修复 | 文件 | 验证 |
| --- | --- | --- |
| 缓存填充增加进程级字节预算；饱和时纯流式降级，不再按不可信 Content-Length 无界累积 | `data_plane/cache/mod.rs`、`data_plane/proxy.rs` | `fill_budget_bounds_aggregate_fill_bytes` + cache 21/21 |
| 配置重载的 generation 构建移入阻塞线程池 | `data_plane/runtime.rs` | data_plane 239/239 |
| listener drain 与 tunnel drain 共用同一绝对 deadline；超时 abort 并显式报错 | `data_plane/server.rs`、`data_plane/error.rs` | data_plane 239/239 |

累计：`cargo check --workspace --all-targets` 通过；gateway data_plane 239/239；真实 PG lifecycle/parity
仍通过（迁移未变）。

## 17. 第七轮修复：关停预算、计量落盘、ACME/UDP/limit_req 有界化（2026-10-21）

| 修复 | 文件 | 验证 |
| --- | --- | --- |
| 管理监听器增加 30s drain deadline，超时不再无限阻塞进程退出 | `main.rs` | 编译通过；gateway 333/333 |
| 用量计量关停时 `flush_all()` 排空 open 窗口，避免每次发布丢 ≤90s 计费事实 | `usage_metering.rs`、`data_plane/server.rs` | usage metering 3/3 |
| tunnel TLS 材料缺失/不可读/半配置改为 generation 构建失败，仅 non-production 未配置时允许自签名 | `tunnel_bridge.rs`、`data_plane/runtime.rs`、`data_plane/error.rs` | tunnel relay 8/8；gateway 333/333 |
| ACME DNS 响应逐帧读取并先限额后分配 | `acme-service/src/dns_http.rs` | dns_http 33/33 |
| UDP 会话回收与 limit_req 空闲扫描移出每包/每请求路径（按窗口限频） | `udp_stream_proxy.rs`、`limit_req.rs` | limit_req 3/3；gateway 333/333 |

累计：`cargo check --workspace --all-targets`、`cargo fmt -- --check` 通过；真实 PG lifecycle/parity 通过。

## 18. 第八轮修复：generation 内准入表、sub_filter 峰值预算与集成回归（2026-10-21）

| 修复 | 文件 | 验证 |
| --- | --- | --- |
| `limit_req`/`limit_conn` 准入表移入 `RuntimeGeneration`，请求与重载不再跨代混用 zone/连接计数 | `data_plane/runtime.rs`、`data_plane/handler.rs` | gateway lib 333/333；limit 10/10；`access_limit_req` 2/2、`limit_conn` 1/1 |
| `sub_filter` 聚合预算按 2× 改写峰值预留，避免瞬时突破 256 MiB 上限 | `data_plane/sub_filter.rs` | `sub_filter` 2/2 |
| 关停收集增加 5s grace，消除收集器与 listener 自身 drain 定时器同刻触发的误报超时 | `data_plane/server.rs` | `data_plane_integration` 58/58 |
| 修复 Windows 上 nginx 反斜杠转义导致的 ACME webroot 测试失败（测试改用正斜杠；解析器保持 nginx 语义） | `tests/data_plane_integration.rs` | `data_plane_integration` 58/58 |
| 为 HTTP/1 语义测试中无上限的 `read_to_end` 增加 2–3s 超时；此前该用例可让整个 CI 测试进程永久挂起 | `tests/http1_connection_semantics.rs` | `http1_connection_semantics` 4/4（0.53s） |

说明：`data_plane_integration` 的 58 个用例此前未在本轮基线中执行过；本轮首次全量执行，
除上述两处外全部通过。

## 19. 第九轮修复：瞬时事务重试与 stream TLS acceptor 预构建（2026-10-21）

| 修复 | 文件 | 验证 |
| --- | --- | --- |
| `40001`/`40P01`/`55P03`/`57014` 不再直接变成 503：新增有界（3 次）、抖动退避的整事务重试，并接入 source-version 创建、deployment 创建、nginx 激活 | `repository-sqlx/src/support.rs`、`source_versions.rs`、`deployments.rs`、`nginx_configs.rs` | 重试单测 3/3；repository parity 1 passed |
| stream TLS acceptor 从每连接构建改为启动时一次构建；证书/客户端 CA 解析错误在 listener 准备阶段失败，而非每连接静默关闭 | `data_plane/stream_proxy.rs`、`data_plane/server.rs`、`data_plane/error.rs` | `stream_proxy` 6/6、`udp_stream_proxy` 1/1、`data_plane_integration` 58/58 |

累计：`cargo check --workspace --all-targets` 通过；真实 PG lifecycle/parity 通过；gateway lib 333/333。

## 20. 第十轮修复：证书账本纯 keyset（去掉整窗计数）（2026-10-21）

- 删除证书首页的 `COUNT(*) OVER ()` 与独立 `COUNT(*)` 回退：该集合随每次签发增长，整窗计数是
  首页最贵的部分，而游标契约不要求 `totalItems`。
- 首页与后续页统一走 keyset 投影（`certificate_cursor_select`），`has_more`/`nextCursor` 保持精确；
  `ok_certificate_page` 不再输出 `totalItems`。
- parity 三处依赖 `.total` 的断言改为校验 `items` 数量与明确的 `total == 0` 契约。
- 验证：`repository_parity` 1 passed（exit 0）；routes-common 7/7；`check-pagination` 通过；
  `cargo check --workspace --all-targets`、`cargo fmt -- --check` 通过。

## 21. 第十一轮修复：visitor UDP 字节预算与 snowflake 默认值（2026-10-21）

| 修复 | 文件 | 验证 |
| --- | --- | --- |
| visitor 每个本地 UDP 监听器增加 8 MiB 聚合缓冲字节预算（KiB 信号量）；permit 在帧写入后释放，预算耗尽的新数据报直接丢弃（UDP 语义），不再出现 256×65KB×N 客户端的无界内存 | `tunnel/src/visitor/mod.rs` | 新增 `datagram_budget_is_charged_in_kib_and_released_on_drop`；tunnel lib 测试通过 |
| snowflake 节点 id 不再有 0 默认：`SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID` 缺失即启动失败；单实例拓扑仍显式写 0，多副本需逐副本覆盖（剩余工作：自动派生/启动期唯一性校验） | `repository-sqlx/src/runtime.rs`、`reconcile_support.rs` | 代码审查 + `cargo check` |

## 22. 第十二轮修复：Snowflake 节点租约分配（2026-10-21）

- 显式非 0 的 `SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID` 继续优先（单实例可钉死）。
- 值为 0 或缺失时，运行时改为调用框架 `SnowflakeNodeAllocator` 从共享
  `sdkwork_node_registry` 申请租约：多副本自动获得不同 node id，租约由后台心跳续期；分配失败即
  启动失败，不再静默使用 0。
- 启动期 reconcile（served domains / application registry）复用同一分配器，避免与主进程生成
  相同前缀的 id。
- 框架表由分配器按 `CREATE TABLE IF NOT EXISTS` 自举，不需要模块迁移；显式配置仍可完全绕开。

验证：真实 PostgreSQL 18.3 上新增 `node_registry_postgres` 用例通过——两个不同
`instance_identity` 的租约拿到**不同 node id**，释放后可继续分配；该用例已加入
`scripts/postgres-ci-verify.mjs` 的 CI PostgreSQL 门禁。`cargo check`（gateway + repository
all-targets）通过。

## 23. 第十三轮修复：契约门禁全绿（OpenAPI 可移植文本、脚本/沙箱契约）（2026-10-21）

`pnpm test:contracts` 此前 3 项失败，逐项定位并修复：

| 失败 | 根因 | 修复 |
| --- | --- | --- |
| backend OpenAPI 非可移植文本 | DNS/WeChat 的 `summary`/`description` 含中文、em dash；权威 JSON 与 YAML 漂移 | 全部改为 ASCII 英文描述，重新执行 `api:materialize` 与 `sdk:generate`（14 targets） |
| idempotency 计数 48≠40 | 幂等操作标记已增至 48，测试硬编码 40 | 更新为 48；逐操作结构校验保持不变 |
| pnpm 契约拒绝 `import:switch:cloud` | 该脚本属于模块 import 切换（双 profile 常态），不是退役的 cloud 构建/部署面 | 在 pnpm/dev-runner 两个契约中显式豁免 `import:switch:cloud` 并写明理由 |
| sandbox 契约拒绝 `with_required_permission` | 兄弟模块新增 `sandbox.instances.*` 自有权限；测试把“不得声明 web.* 权限”误写成“不得声明任何权限” | 断言收窄为真正的约束：不得出现 `web.*` 权限码；保留无 checkout 时显式 skip |

结果：`node --test tests/contract/*.contract.test.mjs` **104 passed / 0 failed**（1 skip：无可选
checkout）。SDK `--check`、API operation/envelope/pagination 门禁、`cargo fmt --check`、
`cargo check --workspace --all-targets` 全部通过。

## 24. 第十四轮修复：集群计数聚合与 tunnel 路由索引（2026-10-21）

| 修复 | 文件 | 验证 |
| --- | --- | --- |
| cluster 列表的 host/instance/online 三个相关计数子查询改为一次分组聚合 LEFT JOIN，并补上租户谓词（原计数可跨租户统计） | `repository-sqlx/src/cluster.rs` | `repository_parity` 1 passed（真实 PG） |
| tunnel 机器列表每请求只快照一次路由注册表并按 session 分组，消除 O(设备 × 路由) 的全表重扫 | `tunnel/src/service/mod.rs` | tunnel lib 106/106、tunnel_relay 8/8 |

说明：`tunnelMachines.list` 的分页会改变公开响应结构（`{machines}` → `SdkWorkPageData`），
按仓库规则属于 breaking API，留待人工评审后单独实施；当前列表规模由 `maxDevices` 硬上限约束。

## 25. 第十五轮修复：host 计数聚合、descriptor/缓存零拷贝（2026-10-21）

| 修复 | 文件 | 验证 |
| --- | --- | --- |
| cluster host 投影内的 `instance_count` 相关子查询改为一次分组聚合 LEFT JOIN（列表 + 详情共用），并补租户谓词 | `repository-sqlx/src/cluster.rs` | `repository_parity` 1 passed（真实 PG） |
| 应用域回退 descriptor 缓存改存 `Arc<Value>`，命中零深拷贝；编译慢路径只克隆一次 | `deploy_fallback.rs` | deploy_fallback 9/9、gateway 333/333 |
| host 解析缓存与 WeChat 校验缓存改为加锁原地更新，消除每次插入克隆整张 map | `deploy_fallback.rs`、`wechat_verify.rs` | wechat_verify 5/5、gateway 333/333 |

C6 至此完整关闭（cluster 列表与 host 投影均无逐行相关子查询）。

## 26. 第十六轮修复：TLS 快照发布 fencing（2026-10-21）

- 跨进程 `DistributionLock` 从“发布时获取”提前到“读取节点证书投影之前获取”：
  读取、material 写入、generation 计算、快照原子写、陈旧目录清理全部落在同一临界区。
- 并发发布者因此严格串行：后获得锁者读到的一定包含前者的写入，不会出现“旧投影后写覆盖新投影”。
- 锁的获取在 `spawn_blocking` 中执行，guard 移入发布用的 blocking 任务释放，异步 worker 不承担文件锁等待。
- 直接调用 blocking 发布函数的测试路径仍在创建 material root **之前**校验 version uuid，保持“不安全 uuid 不触盘”的既有不变量。
- 验证：service lib **46/46**；`cargo check --workspace --all-targets` 通过。

## 27. 第十七轮修复：游标绑定操作与租户（2026-10-21）

- keyset 游标载荷从 `v1|timestamp|id` 改为 `v1|<scope>|timestamp|id`，HMAC 覆盖 scope。
- 每条列表使用自己的作用域：`certificates:<tenant>`、`deployments:<tenant>`、
  `source_versions:<tenant>`、`servers:<tenant>`、`audit`、`cluster_hosts:0`、
  `cluster_instances:0`、`cluster_events:0`、`cluster_heartbeat:0`。
- 解码要求传入 scope 与载荷完全一致；跨操作或跨租户复用游标返回 400（validation），
  不会再“继续另一条列表”。
- scope 仅允许有界 ASCII 且不含 `|`，避免签名载荷歧义。
- 验证：repository lib **24/24**；真实 PG `repository_parity` 1 passed（含跨租户重放拒绝）；
  audit/certificates/deployments/servers/source-versions/cluster 的续页路径均使用新签名。

## 28. 第十八轮修复：域名详情按 id 读取（F3）（2026-10-21）

- 新增 `GET /backend/v3/api/domains/{domainId}` → `domains.retrieve`，权限
  `web.applications.read`，响应 `ApplicationDomainResponse`，404 走标准 ProblemDetail。
- 复用已存在的 `retrieve_managed_domain_repo`，补齐 repository port、service 方法与契约 trait；
  OpenAPI/manifest 同步后重新物化并生成 14 个 SDK target。
- PC 主机详情页不再扫描子域名列表第一页，改为 `client.domain.retrieve(hostnameId)`；超过 50 条
  的主机不再显示 PENDING/SSL off/“-”。
- 验证：PC `pnpm typecheck` 通过；真实 PG parity 覆盖“按 id 读取 + 跨租户 404”；
  `check-api-operation-patterns` 通过。

## 29. 第十九轮修复：PC 路由失败边界与错误文案（F7/F10）（2026-10-21）

| 修复 | 文件 | 验证 |
| --- | --- | --- |
| 路由树外层增加 class error boundary，按 pathname 重置；lazy surface/渲染拒绝显示本地化失败态而非白屏 | `src/App.tsx`、`i18n/{en-US,zh-CN}/webserver/shell/application.ts` | PC `pnpm typecheck`、全量 `pnpm test` **445/445** |
| cluster 概览错误改用共享 `formatWebserverErrorMessage`（本地化、脱敏、截断），不再直出 transport 文案 | `pc-admin-cluster/src/ClusterOverviewSurface.tsx` | cluster surface tests 通过 |
| auth bootstrap 失败不再空 catch 丢弃；记录到 console 便于排障 | `src/App.tsx` | typecheck/tests 通过 |
| 同步修正因新增“验证文件”动作而过期的 Domains 行操作断言 | `tests/served-domain-admin-surface.test.tsx` | 该文件 42/42 |

环境注记：PC 测试要求清空宿主继承的 `SDKWORK_ENVIRONMENT` / `SDKWORK_DEPLOYMENT_PROFILE` /
`SDKWORK_PROFILE_ID`（本机宿主指向 production），否则 Vite 拓扑 profile 校验按设计拒绝；清空后
`pnpm test` 45 files / 445 tests 全绿。

## 30. 第二十轮：容量/soak 证据入口（P0-9 部分收敛）（2026-10-21）

- 新增 `crates/sdkwork-api-webserver-standalone-gateway/tests/capacity_smoke.rs` 与
  `pnpm test:capacity`：启动真实数据面，32 并发 keep-alive 驱动，零失败与 p99 < 1 s 断言。
- 支持 `SDKWORK_CAPACITY_REQUESTS`（固定请求数）和 `SDKWORK_CAPACITY_DURATION_SECONDS`
  （soak 模式，24 h = 86400）。
- 本机 dev profile 实测（2026-10-21）：
  - 固定 32k：11,251 req/s，p50 2.34 ms、p95 5.32 ms、p99 10.95 ms，0 失败；
  - 5 s soak：153,772 请求，30,743 req/s，p99 1.86 ms，0 失败。
- 新增 [PRODUCTION-READINESS.md](../../operations/PRODUCTION-READINESS.md) 汇总容量、数据库、
  协议门禁、数据面与浏览器端证据，并明确 100k 空闲连接、24 h soak、10k 路由编译仍未证明。

## 31. 第二十一轮：分页截断可见化（F8 部分）（2026-10-21）

- `readAllPages` 由“静默返回前 2,000 行”改为返回 `{ rows, truncated }`；上限命中时 `truncated`
  为真。
- 证书签发的域名选择器在截断时显示本地化提示“仅显示前 2,000 个主机名，请缩小根域名范围”，
  不再把前缀当作完整清单（一个被漏掉的通配符主机名会直接导致签发范围错误）。
- 其余 4 处 `readAllPages` 调用按新返回类型读取 `rows`；DNS/账号摘要单页截断仍列在 F8 未完成部分。
- 验证：PC `pnpm typecheck` 通过；`pnpm test` **45 files / 445 tests 全绿**。

## 32. 第二十二轮：Console 权限守卫与 app_user 授权补齐（F4）（2026-10-21）

- `canAccessWebserverResource` 不再对 `app-console` 恒真：两个 surface 都按 entry 声明的权限守
  卫；`WebserverWorkspace` 的菜单过滤同样对 console 生效。
- 启用守卫后，原有 `app_user` 扩展只授予 `web.certificates.* / web.sandbox.* / web.traffic.read`，
  console 的 Applications/Domains/Certificates/Skills/MCP/Plugins 会全部消失。在本模块 IAM 清单中
  补齐 console 入口需要的 `deploy.*.read`、`skills.marketplace.read`、`mcp.marketplace.read`、
  `web.plugins.read`、`iam.provider_accounts.read`。
- 测试更新：console fixture 使用覆盖已挂载模块的权限 scope；“任意登录用户可浏览”改为“未授权
  条目被隐藏”，并保留 shell/sign-out 可用性断言。
- 验证：PC `pnpm typecheck`；`pnpm test` 445/445；IAM 应用引导契约 5/5；仓库契约 104/104。

**人工评审提示**：`specs/iam.module.manifest.json` 的 `app_user` 授权模式属于权限目录变更，
合并前需要安全/产品确认。

## 33. 第二十三轮：DNS 解析概要按页遍历（F8 收尾）（2026-10-21）

- 主机名台账的“解析概要”列不再只读 DNS 记录第一页（200 条）：改为按页遍历，最多 20 页
  （4,000 条），超过上限时显示本地化提示“仅统计前 4,000 条记录”。
- 此前一个记录数超过 200 的 Zone 会让每个主机名的 DNS 计数静默偏少，属于会误导排障的错误数据。
- 新增中英文 `resource.domains.dnsSummaryTruncated`。
- 账号清单来自部署级云账号配置（有界），不构成截断风险。
- 验证：PC `pnpm typecheck`、`pnpm test` **445/445**。

## 34. 第二十四轮：Rust 依赖更新到兼容最新（2026-10-21）

- 执行 `cargo update`（semver 兼容范围内），`Cargo.lock` 更新 147/145 行，主要升级：
  `tokio 1.53.2 → 1.54.0`、`rustls 0.23.45 → 0.23.46`、`rustls-webpki 0.103.15 → 0.103.16`、
  `serde_json 1.0.151 → 1.0.154`、`uuid 1.27.0 → 1.28.0`、`sha2 0.11.0 → 0.11.1`、
  `toml 1.1.6 → 1.1.8`、`zerocopy 0.8.61 → 0.8.63`。
- 验证：
  - `cargo check --workspace --all-targets` 通过（3m03s 全量重编）；
  - gateway lib **333/333**、service lib **46/46**、tunnel lib **106/106**；
  - 真实 PostgreSQL `postgres_lifecycle`、`repository_parity` 均通过；
  - `cargo fmt -- --check` 通过。
- 说明：仍有 4 个依赖处于“上游有更新但 semver 约束之外”的状态，需单独评估主版本升级；
  JS/TS 依赖的 `pnpm outdated` 评估留待前端依赖轮次。

## 35. Reviewed decisions and remaining feature work (2026-10-21)

These items are **not** silently carried debt: each has a named owner/decision and a verification
path. They are called out here so the audit status cannot be mistaken for complete.

| Item | Decision / why not closed in code | Next step |
| --- | --- | --- |
| F1 plugin catalog in `localStorage` | Backend admin API is complete, but the PC plugin page is an `app-console` surface and its AGENTS contract forbids backend-SDK access there. Switching it directly would just move F5's boundary violation into a new feature. The catalog therefore stays local until a webserver app-api facade (or deployments app extension) owns the per-user read/write. | Decide the app-api owner for plugin catalog, then switch the PC service to that SDK |
| F2 H5 IAM / F6 native roots | The mini-program root honestly reports `composed: false` (`appbase-login-flow-not-composed`); H5 has a bootstrap credential only. A native login needs the IAM appbase login flow and per-app IAM client registration, owned by IAM. | Wire `@sdkwork/iam-h5-auth`/appbase login once IAM publishes the H5 flow; add a fail-closed login gate first |
| F5 console uses backend-admin SDK | The traffic-usage reading has no deployments app-SDK equivalent yet; moving it would change the deployments app contract. Console composition still constructs clients only in its own core package. | Add the app-scoped traffic endpoint (deployments), then move the client |
| server-files / config catalog / tunnelMachines pagination | server-files and config catalog are bounded, platform-admin-only local enumerations (4,096 and deployment-layout sizes). tunnelMachines is bounded by `maxDevices` and its response rename to `SdkWorkPageData` is a breaking API. | Add a page/pageInfo additive surface for server-files; evaluate tunnelMachines as a v2 breaking change |
| 100k idle connections / 24h soak / 10k-route compile | `pnpm test:capacity` now measures throughput/latency and supports soak duration; socket-count and route-compile benchmarks need target-host runs. | Run `SDKWORK_CAPACITY_DURATION_SECONDS=86400` on the reference host and add a socket-hold + route-compile harness |

## 36. 第二十五轮：Windows nginx 测试可移植性（T10 关闭）（2026-10-21）

- 根因：nginx 把反斜杠当转义引导符，而测试把原生 Windows 路径（`C:\Users\...`）直接插进
  `include` / `root` / `auth_basic_user_file` 指令；`\U`、`\T` 等被反转义，测试自然找不到文件。
- 修复：测试统一用 `nginx_path()`（`\` → `/`）书写 nginx 指令路径，断言期望值同步；解析器保持
  nginx 语义不变。
- 覆盖：
  - `parser.rs` 3 个 glob/character-class 测试；
  - `nginx_config_surface.rs` 的 include 循环、auth_basic htpasswd、ACME webroot；
  - `nginx_parser_edge_cases.rs` 的 include 块上下文。
- 结果：core lib **250/250**、`nginx_config_surface` **14/14**、`nginx_parser_edge_cases`
  **13/13**；同时给 test-only 的 blocking 发布包装加 `#[cfg(test)]`，消除 lib dead-code 警告。
- 说明：`sdkwork-webserver-source-provider` 的 `validated_target_retains_the_pinned_public_address`
  在首次全量并行执行时曾失败、单独与复跑均通过，按测试隔离/环境抖动记录观察。
- **全量验证**：修复后 `cargo test --workspace --no-fail-fast` 共 **125 个测试目标、0 FAILED**
  （此前 9 个失败全部来自 Windows 路径转义测试）。

## 37. 第二十六轮：ESLint 门禁与 hooks 依赖修复（F9 关闭）（2026-10-21）

- 新增根级 `eslint.config.mjs`：ESLint 10 + `typescript-eslint` 8 + `eslint-plugin-react-hooks` 7，
  只对 PC/H5 的编写源码启用 `rules-of-hooks`（error）与 `exhaustive-deps`（warn），忽略生成
  SDK、dist、public、声明文件；不引入第二套风格权威。
- 新增 `pnpm lint`（`--max-warnings 0`）并接入 `_sdkwork:check`。
- 首次扫描 0 error / 15 warning，逐项修复：
  - 补 `t`（并 memoize translator）、`runtime`、`selectedValues` 依赖；
  - 移除 `page` 这个多余依赖；
  - 对每渲染重建的 `load` / `closeDialog` / `reload` 依赖，保留显式触发键并写明
    `eslint-disable-next-line` 理由（加入它们会导致每次渲染重跑）。
- 验证：`pnpm lint` **0 error / 0 warning**；PC `pnpm typecheck`、`pnpm test` **445/445**；
  契约 **104/104**；`check-pnpm-script-standard`、SDK 14 targets、pagination 通过。

## 38. 第二十七轮：空闲连接容量证据（P0-9 继续收敛）（2026-10-21）

- 在 `capacity_smoke.rs` 增加 `data_plane_accepts_many_idle_connections`（ignored）：
  - 启动真实数据面，按 `SDKWORK_CAPACITY_CONNECTIONS` 建立并保持空闲 TCP 连接；
  - 全部连接保持期间，用独立客户端请求仍必须返回 200；
  - 关闭连接后数据面必须优雅 drain。
- 新增 `pnpm test:connections`。
- 本机实测（Windows，dev profile）：
  - 5,000 连接：1.8 s 建连，持有期间新请求 200；
  - 10,000 连接：8.4 s，200；
  - 15,000 连接：38.7 s，200。
- 配置校验决定了上限：`maxConnections × http2MaxEncodedHeaderBlockBytes ≤ 1 GiB`，默认 64 KiB
  时约 16k 连接；100k 目标机需下调该预算或使用更高规格并记录 RSS 参考 profile。

## 39. 第二十八轮：10k 路由编译预算达标（P0-9）（2026-10-21）

- 新增 `compiles_ten_thousand_routes_within_budget`（ignored）与 `pnpm test:route-compile`：
  生成 10,000 条 exact 路由的真实配置，走完整的 JSON 解析、模型校验、路由表编译路径。
- 实测（dev profile）：10,000 条路由 **349 ms**、复跑 **438 ms**，均在 1 s 预算内；
  `SDKWORK_ROUTE_COMPILE_MAX_MS=1000` 时断言通过。
- 结果写入 `PRODUCTION-READINESS.md`；P0-9 剩余未证明项收敛为 100k 空闲连接与 24 h soak。

## 40. 第二十九轮：本机空闲连接上限定位（2026-10-21）

- 连接测试改为可跨 `127.0.0.0/8` 轮换源地址，并按连接数下调
  `http2MaxEncodedHeaderBlockBytes` 以满足 `maxConnections × 预算 ≤ 1 GiB`。
- 实测：15,000 连接仍稳定持有（新请求 200）；尝试 20,000 时在 **16,101** 条连接处
  `bind` 失败，Windows 返回 `WSAENOBUFS`（os error 10055）——是本机回环缓冲/控制块预算，
  不是数据面拒绝。
- 结论：100k 空闲连接需要目标机内核参数（动态端口范围、fd 上限、必要时多源地址），
  本机 15k 证据已归档；该结论写入 `PRODUCTION-READINESS.md` 与 P0-9。

## 41. 第三十轮：10 分钟持续 soak 与 SLO/支持包文档（2026-10-21）

- `SDKWORK_CAPACITY_DURATION_SECONDS=600 pnpm test:capacity` 实测：**24,549,146 请求、0 失败、
  40,907 req/s、p50 0.75 ms、p95 1.17 ms、p99 1.46 ms**，10 分钟零错误；24 h soak 仍需目标机
  执行（同一 harness，`86400`）。
- 新增 [SLO.md](../../operations/SLO.md)：定义可用性 99.9%、边缘/上游延迟、配置重载、证书续期
  的 SLI/SLO 与燃烧率错误预算；明确当前回环基线不是生产 SLO，首个生产月 `/metrics` 才建立。
- support bundle 明确复用既有标准入口（`bin/doctor.sh --json/--export`、`bin/config.sh`、
  `bin/docker-deploy.sh logs --export`、`bin/backup.sh verify`），不再另造收集器。
- 文档索引 `docs/README.md` 增加 SLO 入口；P0-10 更新进展与剩余项。

## 42. 第三十一轮：前端直接依赖盘点与升级（2026-10-21）

- `pnpm outdated -r` 盘点后，本仓库直接持有的前端依赖完成：
  - `esbuild 0.28.1 → 0.28.2`（mini-program 构建）；
  - `monaco-editor catalog 0.52.2 → 0.57.0`（配置编辑器），PC `build:standalone` 成功；
  - `react` / `react-dom` `19.2.8 → 19.3.1` 对齐（此前 react-dom 19.3 与 react 19.2 的 peer
    不一致），`@types/react` / `@types/react-dom` → 19.3.0。
- 验证：
  - PC `pnpm typecheck`、`pnpm test` **445/445**、`pnpm build:standalone`（dist/standalone/prod）
    成功；`pnpm lint` 0/0；
  - H5 `pnpm typecheck`、`pnpm test` **16/16**、`pnpm build:standalone` 成功；
  - mini-program `pnpm test` **32/32**、`pnpm build:mini-program` 成功（401,364 bytes）。
- 后续轮次已继续：`vitest 5`、`vite 8.3.4`、`jsdom 30`、`@testing-library/react 16.3.3`、
  `lucide-react 1.60` 均已升级并验证；仅 `typescript 6 → 7` 因 typescript-eslint 8.72 拒绝 TS 7
  而保留 6.0.3（见 §44/§50）。

## 43. 第三十二轮：platform target 组织维度继承（D4 收敛）（2026-10-21）

- `webserver_app_platform_target` 的插入此前硬编码 `organization_id = 0`，即使所属 application 已
  属于某组织；改为从 `webserver_application.organization_id` 继承（租户谓词同时约束子查询）。
- 真实 PostgreSQL parity 增加断言：以 organization 31 创建的 application，其 platform target
  读出 `organization_id = 31`。
- 复查：`deployments`、`source_versions` 已从 site 继承；`domains` 无 organization 列写入；
  剩余 0 仅发生在 backend 上下文未携带 org claim 时创建 application，属 IAM/framework claim
  缺口，已记录而非本地哨兵。
- 验证：`repository_parity` 通过；`cargo check -p ... --tests` 通过。

## 44. 第三十三轮：前端工具链升级（vitest 5 / vite 8.3.4 / jsdom 30）（2026-10-21）

- catalog 升级并验证：
  - `vitest ^4.1.10 → ^5.0.5`；
  - `vite ^8.0.3 → ^8.3.4`；
  - `jsdom 29.1.1 → 30.1.3`。
- 验证：
  - PC `pnpm test` **45 files / 445 tests**、`typecheck`、`build:standalone` 全部通过；
  - H5 `pnpm test` **16/16**、`build:standalone` 通过；
  - mini-program `typecheck` 通过。
- **TypeScript 7 尝试与回退**：TS 7.0.2 下三个前端 typecheck 均通过，但 `typescript-eslint 8.72`
  在加载时显式拒绝 TS 7.0（EXIT 2），因此保留 `typescript ^6.0.2` 并在根 `devDependencies` 固定
  `typescript 6.0.3` 供 lint peer 解析；待 typescript-eslint 支持 TS 7 后再升级。
- `pnpm install --frozen-lockfile` 通过；契约 **104/104**、pnpm script standard、SDK 14 targets、
  pagination 通过。

## 45. 第三十四轮：供应链与工作流合规（2026-10-21）

- `.github/workflows/package.yml` 此前包含本地 `resolve-version` job（`runs-on`/checkout），违反
  GITHUB_WORKFLOW_SPEC 的“薄入口”要求，`check:agent-workflow-standard` 一直失败。现改为单一
  reusable job，版本由 reusable workflow 解析；保留最小 `permissions` 块与 pinned reusable SHA。
- `actions/checkout` 本地 job 随之下线，工作流不再引用未固定 SHA 的 action。
- 新增 `.github/dependabot.yml`：cargo / npm(pnpm) / github-actions 每周更新，补丁与 minor 分组，
  限制并发 PR 数量。
- 验证：`pnpm check:agent-workflow-standard` **通过**（1 workflow）；`pnpm check` 全量结果见下一行。
- **架构违规修复**：`pnpm check` 在 `verify-repo` 暴露 gateway 直接依赖
  `sdkwork-intelligence-webserver-repository-sqlx`（WeChat 校验文件查询）。现由 owner API assembly
  re-export `wechat_verification_by_hostname_lookup`，gateway 改走 `sdkwork-api-webserver-assembly`，
  并从 Cargo feature/依赖表移除 repository crate；`verify-repo`、API assembly closure 均通过。
- 顺带清理：删除 2 个被 `.gitignore` 覆盖的临时 `tmp_poll*.sh`；移除隧道 CLI 的 shadowed 变量与
  gateway 未使用导入。
- `pnpm check` 复跑：verify-repo、API assembly、DB pool、API envelope/operation、SDK、docs、
  pnpm script、workflow、browser dist/build 全部通过；最后停在 `shell-portability`——Windows 下
  `bash -n` 收到的是 `D:\...` 路径（`/bin/bash: D:... No such file`），属宿主限制，Linux CI 负责
  该门禁。**补充证据**：在 WSL Linux 中运行同一检查
  `node ../sdkwork-specs/tools/check-shell-portability.mjs --root .` → `passed: 54 files, 0 findings`，
  即该门禁在 Linux 环境为绿，Windows 上仅为路径调用限制。
- 仓库契约测试 `postgres-ci.contract.test.mjs` 原先要求 `resolve-version` 本地 job 与
  `needs.resolve-version`，与 GITHUB_WORKFLOW_SPEC 的薄入口规则冲突；已同步为“单一 reusable job、
  无本地 steps、`package_version` 转发 dispatch 输入”，契约 **104/104** 恢复。
- **人工评审提示**：`package_version` 在分支 push 时改为空值、交 reused workflow 解析；发布前需在
  一次 tag/release 演练中确认产物版本仍来自 manifest。

## 46. 第三十五轮：告警清理与 Linux 门禁证据（2026-10-21）

- 清理本仓库编译告警：gateway `handshake_timeout` 未使用参数、`mtls`/`limit_conn`/
  `udp_stream_proxy` 未使用导入与变量、`proxy_set_header`/`hash_consistent` 未使用 `Arc`。
  `cargo check -p gateway -p tunnel --all-targets` 仅剩 sibling `sdkwork-deployments` 的告警。
- 受影响集成测试复跑：mtls 1、proxy_set_header 1、hash_consistent 3、limit_conn 2、
  udp_stream_proxy 1，全部通过。
- 在 **WSL Linux** 运行 `check-shell-portability`：`passed: 54 files, 0 findings`，补齐 Windows
  宿主无法执行 `bash -n` 的证据。
- `SLO.md` 增补“已发布告警规则”章节，链接 `deployments/observability/prometheus-rules.yml` 与
  README，并明确可用性/延迟 SLO 仍需框架 HTTP 指标作为独立抓取目标，避免把未接线目标写成已实现。

## 47. 第三十七轮：插件目录后端化基础（F1 起步）（2026-10-21）

- 新增迁移 `0028_webserver_plugin_catalog`（up/down + metadata）与 `webserver_plugin` 表：
  - `(tenant_id, user_id)` 所有者隔离；每个 owner 的 plugin key 由部分唯一索引保证唯一；
  - 校验 plugin key、`git|archive` 来源、`draft|active` 状态、可选 64 位 hex checksum；
  - 软删除 + `version` 乐观锁列，租户外键形态与既有业务表一致。
- 同步：baseline DDL、`database/contract/schema.yaml`、`table-registry.json`、`database/README.md`
  （migration 范围到 0028）。
- 新增契约 DTO（`CreatePluginRequest`/`UpdatePluginRequest`/`PluginResponse`/`PluginPage`/
  `ListPluginsQuery`）与 repository CRUD（create/list/retrieve/update/soft-delete），并通过
  `WebRepositoryPort` 暴露；所有动态 SQL 走 `audited_sql`。
- 真实 PostgreSQL parity 新增 `verify_plugin_catalog`：
  - 创建/读取/列表、默认 `draft`、更新为 `active`；
  - 另一个 user 与另一个 tenant 均读不到；跨 owner 按 id 读取返回 NotFound；
  - 重复 plugin key 返回 Conflict；软删除后列表为 0 且按 id 读取 NotFound。
- 验证：`postgres_lifecycle` **1 passed**、`repository_parity` **1 passed**；
  `check-database-framework-standard` 通过；`cargo check --workspace --all-targets`、`fmt` 通过。
- 未完成：backend HTTP routes/OpenAPI/SDK 与 PC `plugin-catalog` 后端切换，下一轮继续。

## 48. 第三十八轮：插件目录 HTTP API 与 SDK（F1 继续）（2026-10-21）

- `WebBackendApi` 增加 5 个方法：`list_plugins` / `create_plugin` / `retrieve_plugin` /
  `update_plugin` / `delete_plugin`；实现要求有效 tenant + authenticated operator（owner 取自
  `operator_id`，请求参数无法改所有者）。
- 路由（backend-api）：
  - `GET/POST /backend/v3/api/plugins`
  - `GET/PATCH/DELETE /backend/v3/api/plugins/{pluginId}`
  - 读权限 `web.plugins.read`，写权限新增 `web.plugins.write`；写操作标记 idempotent。
- IAM 清单新增 `web.plugins.write`，`app_user` 扩展改为 `web.plugins.*`（权限目录变更，需人工评审）。
- OpenAPI 新增 5 个 operation（`plugins.list/create/retrieve/update/delete`）与
  `PluginResponse` / `CreatePluginRequest` / `UpdatePluginRequest` schema；`api:materialize` 重新生成
  route manifest、authority 与 14 个 SDK target。
- 验证：
  - `check-api-operation-patterns`、`check-api-response-envelope`、`check-api-assembly-closure`、
    `api:assembly:validate`、`sdk:check`（14 targets）全部通过；
  - `cargo check --workspace --all-targets`、`cargo fmt -- --check` 通过；
  - backend routes 测试 13/13 + 5/5；
  - 契约 **104/104**（idempotent operation 计数 48 → 51）。
- 未完成：PC `plugin-catalog` 从 localStorage 切换到生成的 backend client，下一轮完成。

**切换受阻（本轮确认）**：`apps/sdkwork-webserver-pc/AGENTS.md` 明确 console 面只能消费
app-api SDK，禁止 backend SDK 访问；插件页是 `app-console`，直接接 backend SDK 会把 F5 的边界
违规复制到新功能。正确路径是先有 webserver app-api facade（或 deployments app 扩展）承载
per-user 插件读写，再切换 PC。该决策属跨仓库 API 归属，已记录而非违规实现。

## 49. 第三十九轮：平台运营者跨用户审核读（F1 能力补齐）（2026-10-21）

- `ListPluginsQuery` 新增 int64-string `owner_id`（wire 为十进制字符串，符合 API_SPEC §13.6）。
- `list_plugins` 服务分支：
  - 未传 `owner_id`：owner = 当前认证 operator（普通用户只能读自己）；
  - 传入 `owner_id`：要求 **platform operator tenant**，否则 Validation，不能通过查询参数越权。
- OpenAPI `plugins.list` 增加 `owner_id` 参数；`api:materialize` + SDK 14 targets 重新生成。
- 验证：`check-api-operation-patterns`、`check-api-response-envelope`、SDK freshness、契约
  **104/104**、`cargo check --workspace --all-targets` 全部通过。
- 仍未切换 PC：console 面的 app-api 边界未解决（见 §48），后端能力先供 backend-admin 审核面使用。

## 50. 第四十轮：前端直接依赖清零（除 TypeScript 7）（2026-10-21）

- catalog 再升级：`@testing-library/react 16.3.2 → 16.3.3`、`lucide-react ^1.31.0 → ^1.60.0`。
- `pnpm --filter @sdkwork/webserver-pc outdated` 现仅剩 `typescript 6.0.3 → 7.0.2`，而 TS 7 被
  `typescript-eslint 8.72` 在加载时拒绝（§44），属上游阻塞。
- 验证：PC `pnpm typecheck`、`pnpm test` **445/445**、`pnpm lint` 0/0 全部通过。

## 51. 第四十一轮：Tunnel 权限目录补齐（P7 部分）（2026-10-21）

- 路由 manifest 已要求 `web.tunnel.read` / `web.tunnel.write`，但 IAM 目录未声明这两个码；
  现补入 `specs/iam.module.manifest.json`（resource `tunnel`，since 0.2.0），`org_admin` 的
  `web.*` 已覆盖，平台运营者按需显式授予。
- 环境变量契约写入 `etc/README.md`：tunnel 管理需同时配置
  `SDKWORK_WEBSERVER_TUNNEL_OPERATIONS_URL` 与 `SDKWORK_WEBSERVER_OPERATIONS_AUTH_TOKEN_FILE`；
  缺任一项按设计返回 503，绝不返回空机器列表。默认 profile 不内置 token 文件（宿主 secret），
  这是部署配置边界，不是未声明的代码依赖。
- 验证：契约 **104/104**、repository docs：通过。

## 52. 第四十二轮：审计状态校正与剩余项矩阵（2026-10-21）

本轮不改代码，只把审计表中已被后续证据覆盖的行修正，避免“已完成仍显示待修”的错账：

- **M1**（缓存填充字节限额）：与 C4 同一修复，已改为已修复。
- **D2**（PG 生命周期不可通过）：真实 PostgreSQL 18.3 上 lifecycle/parity 均通过，已改为已修复。
- **M12**：10k 路由/吞吐/15k 连接已有证据；**O(n²) 重叠校验仍未改动**，保持部分修复。
- **A10**（DNS 账号进程内分页）：账号来自启动期 issuer registry（有界配置集合），分页是稳定切片，
  不是 DB 全表读；记录为 reviewed exception，而非债务。

剩余未关闭项按性质分组：

| 组 | 项 | 阻塞条件 / 下一步 |
| --- | --- | --- |
| 需要外部仓库决策 | F1 PC 切换、F5 traffic 读、F2 H5 IAM、F6 原生端 | webserver app-api 归属、deployments app 端点、IAM H5 登录契约 |
| 需要目标机基础设施 | 100k 空闲连接、24h soak、签名/发布演练 | 目标机 fd/端口预算、24h 窗口、Linux runner |
| 需要人工评审/破坏性操作 | D11 有损 down migration 标记、权限目录变更、0028 迁移 | 删除 tracked down 文件需人工确认；迁移/权限需评审 |
| 需要专项实现 | P3 nginx 修订历史、P4 管理菜单、P5 分级限流、P6 缓冲类指令、T1/T3/T5/T6/T7/T8 | 每项都需要独立设计与验收，不能以文档冒充完成 |

## 53. 第四十三轮：路由重叠校验 O(n²) 消除（M12）（2026-10-21）

- `validate_routes` 的重叠检测此前对每个路由回看全部前序路由（10k 路由约 5000 万次比较）。
  现改为按 `(RoutePathType, path)` 建立索引，只比较同一 matcher 的路由；`RoutePathType` 增加
  `Hash` derive（非破坏性）。
- 语义保持不变：同一 `(pathType,path)` 且 methods 重叠仍报 `route overlaps route …`，core lib
  250/250 通过。
- 性能：10,000 路由编译 **349–438 ms → 33 ms**（`pnpm test:route-compile`，dev profile），
  1 s 预算余量从 ~2× 提升到 ~30×。
- 文档：M12 更新；`PRODUCTION-READINESS.md` 路由编译数字同步。

## 54. 第四十四轮：int64 `totalItems` 精度收口（A12）（2026-10-21）

- H5 `toWebserverH5ListPage` 不再 `Number(totalItems)`：`WebserverH5ListPage.totalItems` 改为
  **精确十进制字符串**，仅接受 `^[0-9]+$`，非法值归 `"0"`；分页信号继续用 `hasMore`/`totalPages`。
- PC sandbox `sandboxTotalItems` 对超出 `Number.MAX_SAFE_INTEGER` 的 int64 显式返回
  `Number.MAX_SAFE_INTEGER`（`rowCount` 需要 number），不再静默舍入；新增两条边界测试：
  超大值钳制、前导零/负数/缺失归 0。
- 验证：H5 `typecheck` + `test` **16/16**；PC `typecheck`、`test` **447/447**（新增 2 条）。

## 55. 第四十五轮：公共 demo 禁用 dev 内联认证（S3 部分）（2026-10-21）

- `web_use_dev_inline_auth_resolver` 此前对**所有非 production-like** 环境生效，包括公网
  `demo`。现收紧为仅 `development`/`dev`，且仍需显式 `SDKWORK_WEBSERVER_DEV_AUTH_BYPASS=true`。
- 新增测试 `demo_never_uses_dev_inline_auth`：demo + bypass=true 必须为 false。
- 验证：`cargo test -p sdkwork-webserver-core --lib runtime_env` **4 passed**。
- 残留（部署要求）：demo 的 `SECRET_ENCRYPTION_KEY` 仍按非 production-like 派生；生产化 demo
  必须在部署时提供该密钥，不再由代码生成。文档 S3 已更新为部分修复。

## 56. 第四十六轮：nginx 配置响应补全与修订号递增（P3 部分）（2026-10-21）

- `NginxConfigResponse` 补齐 OpenAPI 已声明但 Rust 未返回的字段：`configContent`、`configHash`、
  `versionNo`、`deployedAt`、`createdAt`、`updatedAt`。
- 内容更新 SQL 增加 `version_no = version_no + 1`；创建默认 1，真实 PostgreSQL parity 断言
  update 后 `versionNo == 2` 且详情携带新内容。
- 列表页不携带整份配置正文（单行可达 1 MiB）：mapper 按“是否详情”决定是否读取 content，
  空内容通过 `skip_serializing_if` 省略；详情/创建/更新仍返回完整内容。
- 验证：`repository_parity` **1 passed**；contract/workspace 检查随后执行。
- 未完成：修订快照表与跨版本回滚（需要新的 revision schema 与回滚 API，不能以自增号冒充历史）。

## 57. 第四十七轮：tunnelMachines 真实分页（A4 部分）（2026-10-21）

- `TunnelMachineListResponse` 新增可选 `pageInfo`（offset 模式，含 page/pageSize/totalItems/
  totalPages/hasMore），旧客户端仍可解码。
- `WebBackendApi::list_tunnel_machines` 增加 `page`/`page_size`；服务在网关返回的有界清单上做稳定
  切片并生成真实 `pageInfo`（不再让 PC 伪造 `hasMore:false`）。
- 路由接受 `page`/`pageSize`；OpenAPI 标记 `x-sdkwork-pagination-mode: offset` 并补参数与
  `pageInfo` schema，14 个 SDK target 重新生成。
- PC admin-core 改为传入查询参数并消费 `response.pageInfo`；仅当旧生产者缺字段时回退。
- 验证：`api operation patterns`、`pagination` 通过；PC `typecheck`、`test` **447/447**、`lint` 0/0。
- 未完成：`data.item.machines` → `data.items` 的标准信封是 breaking，需 v2 评审。

## 58. 第四十八轮：server-files 目录截断显式化（A6 部分）（2026-10-21）

- `DirectoryListing` 新增 `truncated: bool`；`browse_directory` 在达到 `maximumEntries` 时额外探测
  一条，只有确实还有子项才置 true —— “恰好 4096 条”不会被误报为截断。
- OpenAPI `ServerDirectoryListing` 增加 `truncated`（default false）；14 个 SDK target 重新生成。
- 效果：客户端首次能区分“完整列表”和“被 4096 上限截断”，不再静默丢条目。
- 验证：server-files-service **18 + 7 passed**（wire contract 新增 truncated 断言）；
  operation patterns、response envelope、SDK 生成通过。
- 未完成：目录列表的 `page`/`pageInfo` 分页（需与文件浏览器 UI 一起设计）。

## 59. 第四十九轮：config catalog 截断显式化（A7 部分）（2026-10-21）

- `WebserverConfigCatalog` 新增 `truncated: bool`；采集器允许第 `maximumEntries + 1` 条存在，
  据此区分“恰好 4096 条”与“还有更多”，随后再截断到上限。
- OpenAPI `WebserverConfigCatalog` 增加 `truncated`（default false）；14 个 SDK target 重新生成。
- 验证：config-service **8 passed**；operation patterns、response envelope、SDK 生成通过。
- 未完成：config catalog 分页（部署配置规模有界，需与配置编辑器 UI 一起设计）。

## 60. 第五十轮：nginx“接受但不执行”指令告警固定（P6 reviewed exception）（2026-10-21）

- 复核 `ACCEPTED_IGNORED` 路径：`walk` 已为每条被忽略指令生成 `ConfigDiagnostic`
  （“accepted for nginx compatibility … not enforced … has no effect …”），导入报告
  `NginxLoadReport.ignored` 会返回，因此 P6 不是“静默忽略”。
- 新增测试 `accepted_but_unenforced_nginx_directives_surface_a_diagnostic`：`gzip_static on;`
  必须产生包含 `gzip_static` 与 `not enforced` 的诊断，防止未来退化为静默。
- 语义边界：`proxy_buffering` / `proxy_request_buffering` / `proxy_next_upstream` 的执行语义
  仍由运行时策略决定；要按指令逐个执行需独立实现，不在本轮冒充完成。
- P6 记为 reviewed exception；验证：nginx_config_surface **15 passed**。

## 61. 第五十一轮：P4 产品决策显式化（2026-10-21）

- 复核 `adminModules`：Nginx / Servers / Server Files / Server Config / Diagnostics 五个菜单被
  有意移除，代码注释给出理由——单节点的 nginx 运行时、机器清单、SSH 文件浏览已由 cluster 平面
  （hosts/instances/liveness）取代，重新加入会制造第二个单节点控制面。
- 但 `PRD-FR-013` / `PRD-FR-028` 仍明确承诺这些管理面，API 路由也仍存在。这是**产品承诺与实现
  决策的矛盾**，不能由实现单方改 PRD 或恢复 UI。
- P4 状态改为 `product decision pending`，并在 §52 矩阵保留；需产品在“保留 API + cluster 取代
  UI”与“恢复菜单”之间选择，然后同步 PRD 或 UI。

## 62. 第五十二轮：测试环境依赖消除（T9）（2026-10-21）

- 复现：宿主 `SDKWORK_ENVIRONMENT=production` 时
  `web_bootstrap::tests::backend_framework_enforces_permission_and_tenant_scope` 在
  `web_framework_runtime_policy_from_env()` 的生产 CORS 校验处 panic。
- 修复：测试在构建框架层前把 `SDKWORK_ENVIRONMENT`/`SDKWORK_WEBSERVER_ENVIRONMENT` 临时设为
  `development`，**构建完成后立即恢复**宿主值；测试只验证授权语义，不验证生产部署校验。
- 验证：宿主 environment=production 下 routes lib **13/13** 通过。

## 63. 第五十三轮：standards-alignment 现状同步（2026-10-21）

- `docs/standards-alignment.md` 的三处过期描述已按当前实现修正：
  - 数据库迁移数 `22 → 24`（0005–0028），并补充 plugin catalog parity；
  - 删除“3 个 Windows nginx 测试仍不稳定”的旧说法（T10 已关闭，全量 workspace 0 失败）；
  - 前端工具链 `Vite 8.1.5/esbuild 0.28.1 → Vite 8.3.4/esbuild 0.28.2`，并记录 vitest 5、
    jsdom 30、monaco 0.57、React 19.3.1。
- 验证：repository docs standard、契约 **104/104**、`cargo fmt -- --check` 通过。

## 64. 第五十四轮：clippy 现状测量（T8 部分）（2026-10-21）

- 实测 `cargo clippy --workspace --all-targets`：可运行但未接入门禁；全量 100+ 告警。
- 本仓库核心 crate 抽样：core 32、server-files-service 6、config-service 1、contract 1、
  tunnel-core 2，共约 42 条；主要类别为 `too_many_arguments`、`needless_borrow`、
  `redundant_closure`、`manual_strip`、`large_enum_variant` 等。
- 结论：clippy 未接入不是“无命令”，而是需要一次逐项决策的告警清理波次（尤其
  `too_many_arguments` 不能机械 allow）。T8 更新为部分证据，未冒充完成。
- 本轮不改代码；验证仍为 docs/contract/fmt 三门绿。

## 65. 第五十五轮：端到端验证快照（2026-10-21）

在全部已实施修改之上做一次完整验证，作为当前基线：

| 门禁 | 结果 |
| --- | --- |
| `cargo fmt -- --check` | 通过 |
| `cargo check --workspace --all-targets` | 通过 |
| 真实 PostgreSQL `postgres_lifecycle` | 1 passed（baseline + 24 migrations + seed + drift clean） |
| 真实 PostgreSQL `repository_parity` | 1 passed（含 plugin catalog、nginx versionNo、org 继承等新断言） |
| 契约测试 | **104/104** |
| SDK freshness | 14 materialized targets |
| pagination / operation patterns / response envelope / database framework / repository docs | 全部通过 |

浏览器端此前同轮验证：PC **447/447**、H5 **16/16**、lint 0/0；容量证据：32k 请求 0 失败
p99 ~3.6 ms、10 分钟 soak 24,549,146 请求 0 失败、10k 路由 33 ms、15k 空闲连接。

## 66. 第五十六轮：CI 测试门禁澄清（T1 reviewed exception）（2026-10-21）

- 复核 `sdkwork.workflow.json`：validate 阶段顺序为 archive validate/signature/smoke/SBOM →
  `pnpm test:postgres:required` → recovery → HA → **`pnpm verify`** → `git diff --exit-code`。
  `pnpm verify` 内含 `_sdkwork:test`（`cargo test --workspace` + 契约 + IAM 模板），所以测试并非
  “没有被 CI 执行”，而是作为打包 validate 的一部分执行。
- 独立 test job 与 GITHUB_WORKFLOW_SPEC 的薄入口规则冲突（package.yml 只允许 reusable 调用），
  因此 T1 记为 reviewed exception 而非代码缺口。
- validate 的本地等价命令已全绿：Rust workspace 0 失败、PG lifecycle/parity、契约 104/104、
  fmt/check、PC/H5 测试；Windows 宿主只有 `shell-portability` 需 Linux（已在 WSL 验证 54 files/0
  findings），不再是“validate 已红”。

## 67. 第五十七轮：未认证 HTTP 契约测试（T5 部分）（2026-10-21）

- 新增 `backend_framework_rejects_unauthenticated_requests`：构建与生产同一条
  `build_web_backend_api_framework_layer`，对 `/backend/v3/api/applications` 发起**无任何凭据**
  的请求，断言返回 401。
- 该测试覆盖“框架层在进入 handler 前拒绝”的真实 HTTP 路径，而不是只断言 manifest 元数据；
  与既有“有凭据但无权限 → 403 / tenant 缺失 → 422”测试形成最小闭环。
- 验证：routes lib `backend_framework` 过滤 **2 passed**；全量 routes lib 随后复跑。
- 未完成：逐路由参数化 HTTP 矩阵（100+ operations）仍需统一的 service mock 设计。

## 68. 第五十八轮：clippy 门禁起步（T8 部分）（2026-10-21）

- 新增根脚本 **`check:rust-clippy`**：
  `cargo clippy --no-deps -p sdkwork-webserver-contract -p sdkwork-server-files-service -p sdkwork-webserver-config-service -- -D warnings`，
  并接入 `_sdkwork:check`（`pnpm verify` 会执行）。
- 清零这三个 crate 的告警：
  - server-files `contained_io`：5 处 `&OsString` 冗余借用；`path_security`：`format!` 冗余；
  - config-service：`sort_by` → `sort_by_key(Reverse(..))`；
  - contract：`WebBackendApi` 的 `too_many_arguments` 以带理由的 `allow` 处理（拆参数结构体会改所有
    implementor，属 lint-only 变更）。
- 验证：`pnpm check:rust-clippy` 通过；server-files **7 + 18 passed**、config-service **8 passed**；
  workspace check/fmt 通过；pnpm script standard（169 scripts）通过。
- 未完成：core（32 条）与其他 crate 的告警清理、nginx/deploy/adaptive-web 门禁入口。

## 69. 第五十九轮：clippy 门禁扩展到 core/tunnel-core（T8 继续）（2026-10-21）

- 用 `cargo clippy --fix` 修复 core/tunnel-core 的机械告警（needless borrow、manual strip、
  `while let`、`sort_by_key`、slice copy、`first().is_none()` 等），并逐项复核 diff。
- 对不可机械修改的项给出**带理由的处理**：
  - `ModuleImportError` 的 `result_large_err` 在 crate 级 allow（诊断信息必须保留）；
  - `materialize_location` 的 `too_many_arguments` 在两处 item 级 allow（拆参数结构体会牵动
    解析器全部调用点）；
  - `expand_proxy_pass_template` 的 `result_unit_err` item 级 allow（公共签名，改动属 API 变更）。
- `check:rust-clippy` 现覆盖 **contract / core / tunnel-core / server-files / config-service**
  五个 crate；`pnpm check:rust-clippy` 通过。
- 验证：core **251/251**、nginx surface **15/15**、nginx edge **13/13**、tunnel-core **97/97**；
  workspace check/fmt 随后复跑。
- 未完成：gateway/repository/service 等大 crate 的告警清理与其余门禁入口。

## 70. 第六十轮：clippy 门禁扩展到 service / repository-sqlx（T8 继续）（2026-10-21）

- service / repository-sqlx 的机械告警修复：`needless_borrow`、`map(..).flatten()` →
  `and_then`、redundant closure、显式生命周期消除、`Arc as Arc<dyn>` 的 trivial cast 改为类型
  标注、`i32::try_from(u16)` → `i32::from`、无插值 `format!` 移除、`Ok(..?)` 简化。
- 对 `WebRepositoryPort` 与 `list_certificates_repo` 的 `too_many_arguments` 以带理由的 allow
  处理（参数结构体会牵动所有实现/调用点）。
- **MSRV 调整**：`File::try_lock`（TLS 分发锁）稳定于 Rust 1.89，而 workspace 声明 1.85；将
  `rust-version` 提升到 **1.89**，消除 `incompatible_msrv`。`check-rust-manifest-standard` 与
  `check-rust-crate-naming-standard` 均通过。
- `check:rust-clippy` 现覆盖 **7 个 crate**；验证：clippy gate 通过、service **46/46**、
  repository **24/24**、workspace check/fmt 通过。
- 未完成：gateway（104 条）等剩余 crate；nginx/deploy/adaptive-web 门禁入口。

## 71. 第六十一轮：gateway clippy auto-fix 与全量回归（T8 继续）（2026-10-21）

- 对 gateway lib 执行 `cargo clippy --fix`：24 个文件、+675/−269 行，告警 **104 → 90**；
  剩余主要是 `result_large_err`、`too_many_arguments`、`Arc as Arc<dyn>` trivial cast 与
  未使用私有项，需要逐项人工判断，未强行清零。
- 全量回归：gateway lib **333/333**、全部集成测试（data_plane 58、proxy_cache、routing_e2e、
  upstream_*、config_sources_e2e、tunnel_relay 等）**全部通过**；workspace check 通过。
- gateway 仍未纳入 `check:rust-clippy` 门禁，T8 保持“部分修复”。

## 72. 第六十二轮：gateway clippy 清零并纳入门禁（T8 继续）（2026-10-21）

- 在 auto-fix 基础上继续处理 gateway 的 90 条告警：
  - crate 级 allow（带理由）：`result_large_err`（诊断错误链）、`too_many_arguments`
    （handler 参数）、`type_complexity`（嵌套 future/映射）；
  - 修复 trivial cast（`Arc as Arc<dyn>` / `Box as BoxError` 改为类型标注或返回类型标注）、
    `let...else` → `?`、doc list 缩进、可见性、无用方法与冗余闭包；
  - `LimitConnDecision::Allow` 是仅被匹配、从不构造的释放哨兵，保留并标注 `dead_code` 理由。
- gateway **lib + bins 告警清零**；`check:rust-clippy` 现覆盖 **8 个 crate**。
- 全量回归：gateway lib **333/333** + 全部集成测试通过；workspace check/fmt 通过；
  `pnpm check:rust-clippy` 退出 0。
- 仍未覆盖：gateway tests 自身告警（门禁默认 lint lib+bins），以及 nginx/deploy/adaptive-web
  门禁入口。

## 73. 第六十三轮：全 workspace 测试回归（2026-10-21）

- `cargo test --workspace --no-fail-fast`：**125 个 target 全部 ok、0 failed**（含 gateway 333、
  core 251、contract、tunnel、acme、config-service、server-files、repository 等；3 个容量测试
  按设计 ignored）。
- 诚实记录：同命令的第一次运行退出码为 1，但输出未被完整保留、无法定位失败项；立即重跑写入
  `target/tmp/workspace-tests.log` 后 125/125 通过。未复现的瞬时失败不作为“已修复”声明，若后续
  再现需按 flaky 定位。
- `cargo fmt -- --check`、`cargo check --workspace --all-targets` 通过；`pnpm check:rust-clippy`
  八 crate 退出 0。

## 74. 第六十四轮：ACME Pebble E2E runner（T6 部分）（2026-10-21）

- 新增 `tools/run-pebble-e2e.mjs` 与根脚本 **`test:acme:pebble`**：
  - 解析 `PEBBLE` / `CHALLTESTSRV`（含绝对路径）或 PATH 中的 `pebble` /
    `pebble-challtestsrv`；
  - 任一缺失时以 **exit 69** 失败并打印下载地址，避免 ignored 测试的“静默跳过”；
  - 均存在时执行 `cargo test -p sdkwork-webserver-acme-service --test pebble_lifecycle --
    --ignored --nocapture`。
- 验证：`node --check` 通过、`check-pnpm-script-standard`（170 scripts / 48 runner scripts）通过、
  缺失二进制分支实测 exit 69。本机无 Pebble 二进制，**未执行成功路径**，不冒充 E2E 已通过。

## 75. 第六十五轮：真实 nginx 测试跳过语义澄清（T7 reviewed exception）（2026-10-21）

- 复核 `edge-runtime/src/nginx.rs`：缺 nginx 时测试执行
  `eprintln!("nginx unavailable; real edge validation evidence skipped")` 后返回，**不是静默跳过**；
  可用 `SDKWORK_WEBSERVER_TEST_NGINX_BINARY` 指定二进制，或依赖 PATH。
- 仍成立的部分：CI 镜像未安装 nginx，因此“真实 nginx 校验/重载”证据要在带 nginx 的 runner 上
  才产生；这是基础设施条件，不是代码虚假实现。
- T7 记为 reviewed exception，不再列为待修代码项。

## 76. 第六十六轮：积木架构组合门禁接入（2026-10-21）

- 复核 `COMPOSABLE_ARCHITECTURE_SPEC.md` 的闭包矩阵，发现六个组合校验器此前**不在**
  `_sdkwork:check` 中：`check-component-port-bindings`、`check-permission-composition`、
  `check-rust-backend-composition`、`check-frontend-composition`、
  `check-composition-resolver`、`check-app-sdk-consumer-imports`（`verify-repo` 不调用它们）。
- 新增根脚本 **`check:composable-architecture`** 串联六者，并接入 `_sdkwork:check`，
  紧跟在 `check:app-composition` 之后。
- 本应用组合了 7 个同源积木模块（webserver 自有 backend/internal + IAM / Drive / Skills / MCP /
  Sandbox / Deployments，各自由 owner assembly 提供 route manifest、OpenAPI、权限目录）；
  六个校验器全部通过：port bindings、permission composition、Rust backend composition、
  frontend composition、composition resolver、app SDK consumer imports。
- 验证：`pnpm check:composable-architecture` 退出 0；`check-pnpm-script-standard`
  （171 scripts）通过。

## 77. 第六十七轮：component.spec.json 补 required verification（2026-10-21）

- `COMPONENT_SPEC.md` §3 要求 `verification.commands` 至少包含一个可验证命令；根
  `specs/component.spec.json` 此前**没有该节**。
- 补入 `verification.commands`（app-composition、composable-architecture、assembly closure、
  api/sdk check、workspace tests、PG parity）与 `metadata.standardVersion/status`。
- 验证：`verify-repo`、六个 composable-architecture 校验器、repository docs 全部通过。
- 未处理：`crates/sdkwork-webserver-tunnel/specs/component.spec.json` 仍是旧式结构（缺
  `component.name/root/domain/languages` 与 `verification`）；它不在应用根组合校验的强制范围内，
  归一化该 spec 需单独评审，未在本轮冒充完成。

## 78. 第六十八轮：tunnel component.spec 归一化（2026-10-21）

- 上一轮记录的 tunnel spec 缺口已补齐：`component` 增加 `name/root/type/domain/capability/
  surface/languages/generated/status/manifests`，并新增 `verification.commands`（保留原 `commands`
  兼容旧读取方）。
- 保留其多 crate `surfaces`（core/protocol/transport/composition）与 `invariants`/
  `knownLimitations`，因为这些是 tunnel 家族的专有事实，不属于通用 schema。
- 验证：`verify-repo`、`check-component-port-bindings` 仍通过；应用根组合不受影响。
- 未完成：该 spec 尚未迁到 `contracts.publicExports/providedPorts/requiredPorts` 形态；它目前
  以 `surfaces` 表达四个 crate 的边界，迁移需要逐 crate 定义 port，属独立评审项。

## 79. 第六十九轮：nginx 修订快照落库（P3 部分）（2026-10-21）

- 迁移 `0029_webserver_nginx_config_revision`：新表
  `webserver_nginx_config_revision`，`UNIQUE (config_id, version_no)`、按
  `(tenant_id, config_id, version_no DESC)` 索引、FK `ON DELETE CASCADE`；补齐 metadata、schema
  契约、table-registry 与 database README（迁移范围到 0029）。
- 仓储在**创建**（version 1）与**每次内容更新**（新 `version_no`）后写入快照；
  `ON CONFLICT (config_id, version_no) DO NOTHING` 使重试幂等。首版曾把 RFC3339 字符串直接绑定到
  `TIMESTAMPTZ`，真实 PG 报 `column "created_at" is of type timestamp with time zone but expression
  is of type text`，已改为 `$9::timestamptz`。
- 真实 PostgreSQL：`postgres_lifecycle` **1 passed**（baseline + 25 migrations + drift clean）；
  `repository_parity` **1 passed**，新增断言 update 后 revision 1/2 内容分别为旧/新配置。
- 验证：`cargo fmt -- --check`、workspace check、database framework、repository docs 通过。
- 未完成：revision **列表/读取/回滚 API** 与 PC UI；当前快照只写不读，不能宣称“可回滚”。

## 80. 第七十轮：nginx 修订历史读取 API（P3 继续）（2026-10-21）

- 契约新增 `NginxConfigRevisionSummary`、`NginxConfigRevisionPage`、
  `NginxConfigRevisionResponse`；`WebBackendApi` 与 `WebRepositoryPort` 增加
  `list_nginx_config_revisions` / `retrieve_nginx_config_revision`。
- 仓储实现：先按 `(tenant_id, uuid)` 解析内部 id，再分页读取 `version_no DESC` 的摘要
  （不返回正文），单条读取返回完整正文；不存在的 config/revision 均为 404。
- 路由与 OpenAPI：
  - `GET /backend/v3/api/nginx/etc/{configId}/revisions`（offset 分页，`data.items + pageInfo`）
  - `GET /backend/v3/api/nginx/etc/{configId}/revisions/{versionNo}`（`data.item`）
  - 权限沿用 `web.nginx.write`，14 个 SDK target 重新生成。
- 真实 PostgreSQL：`repository_parity` 新增按端口读取 revision 列表/单条断言（total=2，版本 2/1，
  v1 正文含旧配置），**1 passed**。
- 验证：`cargo check --workspace --all-targets`、fmt、8-crate clippy、routes lib 14/14、
  契约 104/104 全部通过。
- 未完成：**回滚写接口**（把历史版本作为新 revision 落回）与 PC UI；目前 API 只能读历史。

## 81. 第七十一轮：nginx 修订回滚接口（P3 继续）（2026-10-21）

- 新增 `POST /backend/v3/api/nginx/etc/{configId}/revisions/{versionNo}/rollback`：
  读取历史版本，经 `update_nginx_config_repo` 把旧正文写回当前配置——因此回滚本身产生**新的
  `version_no` 与新快照**，历史保持 append-only，回滚可审计。
- `WebBackendApi` / `WebRepositoryPort` 增加 `rollback_nginx_config_revision`，服务层沿用
  backend tenant 校验；OpenAPI 标记非幂等写操作，14 个 SDK target 重新生成。
- 真实 PostgreSQL：parity 断言回滚后 `version_no=3`、正文回到 v1（不含 `http2`）、
  revision total=3 且最新为 v3，**1 passed**。
- 验证：fmt、workspace check、8-crate clippy、routes lib 14/14、契约 104/104、
  API operation patterns/response envelope、SDK freshness 通过。
- 未完成：PC 端的修订历史列表/差异/回滚 UI（后端 API 已可用）。

## 82. 第七十二轮：P3 落地后的全量回归（2026-10-21）

- `cargo test --workspace --no-fail-fast`：退出 **0**（日志 `target/tmp/workspace-tests-p3.log`）。
- PC：**45 files / 447 tests passed**；H5：**3 files / 16 tests passed**；`pnpm lint` 0 警告。
- 本轮不新增功能，只确认 P3 API/SDK 变更未破坏任何既有目标；结果全绿。
- 仍需外部条件的事项未变化：P3 UI 依赖 P4 产品决策（是否恢复 nginx 管理菜单），P5 分层预算、
  F1/F2/F5/F6、容量证据与商业化归属仍未闭环。






























| 上游阻塞 | TypeScript 7、D10 lock/statement timeout | typescript-eslint 尚未支持 TS7；timeout 需 database 框架实现 |










































