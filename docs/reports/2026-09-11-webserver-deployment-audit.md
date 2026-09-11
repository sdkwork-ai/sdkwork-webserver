# sdkwork-webserver 部署健康度与 sdkwork-specs 对齐审计报告

- 日期：2026-09-11
- 宿主：Windows + WSL `Ubuntu-22.04`（Docker 29.3.0 在 WSL 内）
- 源码：`E:\sdkwork-space\sdkwork-webserver` @ `b7dca2f0`（工作树含 19 处未提交改动）
- 范围：development / test / staging / demo / production 五环境的**实况**、`sdkwork-specs` 门禁对齐、运营能力完整性
- 审计脚本与原始证据：`.sdkwork/audit/`（`gates*.txt`、`ops.json`、各 `*.sh` 探针）

> **路径迁移提示（2026-09-11 之后）**：本报告按当时的工作树记录，其中
> `deployments/docker/scripts/*`、`deployments/docker/bundle/*`、
> `deployments/docker/postgres/init/*` 已迁入 `bin/container/`、
> `bin/container/postgres-init/` 与扁平的 `bin/docker-bundle-deploy.sh` /
> `bin/docker-bundle-release.sh`（`MODULE_BIN_SPEC.md` §1/§2.1/§2.2）。
> 报告正文保持原样以保留当时的取证结论；今天的位置见 `bin/README.md`。
> 取证脚本本身已按 §2.1 清理（保留 `.sdkwork/audit-evidence/` 非脚本证据）。

---

## 0. 一句话结论

**静态规范对齐是好的（部署面 9/9 门禁全绿、运营合规 12/12 PASS）；但线上实况存在 3 个 P0 级问题、5 个 P1 级问题，其中 P0-1（生产向匿名访客下发系统级访问令牌）必须立即处置。** 门禁之所以仍全绿，是因为存在四处**门禁盲点**——它审计的是文件而非运行时，且扫描范围未排除构建/agent 暂存目录。webserver 自身的应用/装配面门禁实测是干净的（22 项中所有 EXIT=1 要么全在兄弟仓，要么是暂存目录导致的假阳性）。

---

## 1. 五环境实况快照

| 环境 | 管理端口 | HTTP | 实际运行镜像 | webserver | gateway 侧车 | doctor |
| --- | --- | --- | --- | --- | --- | --- |
| development | 13800 | 200 | `…standalone:0.1.2` | healthy | **unhealthy** | 10P/0W/**1F** |
| test | 18888 | 200 | `…standalone:0.1.2` | healthy | healthy | **11P/0W/0F** |
| staging | 18081 | 200 | `…standalone:0.1.2` | healthy | **unhealthy** | 8P/1W/**2F** |
| demo | 19080 | 200 | `…standalone:0.1.2` | healthy | **unhealthy** | 9P/0W/**2F** |
| production | 18080 | 200 | `…standalone:0.1.2` | healthy | **unhealthy** | 9P/0W/**2F** |

- 五环境容器创建时间均为 **2026-09-11 11:37–11:42**（网关侧车 11:41–11:42），即今日被整体重建过一次。
- 与 2026-09-10 的 0.1.5 发布结果对比：**全部五环境已从 0.1.5 回落到 0.1.2**。

---

## 2. P0 问题

### P0-1 生产环境向匿名访客下发「系统级」访问令牌（安全）

**证据**（直接 HTTP 抓取，无需认证）：

```
$ curl -s http://127.0.0.1:18080/            # production 边缘
<script>globalThis.__SDKWORK_CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN__="eyJ…";</script>
```

解码该 JWT（production）：

```json
{ "alg": "HS256", "kid": "100001:local-hs256:primary", "typ": "JWT" }
{
  "app_id": "sdkwork-webserver", "aud": "sdkwork-webserver",
  "auth_level": "system", "user_id": "system",
  "data_scope": ["tenant:100001", "user:system"],
  "permission_scope": [
    "web.applications.read","web.applications.write",
    "web.certificates.read","web.certificates.write",
    "web.nginx.write","web.servers.read","web.servers.write",
    "web.servers.files.read","web.servers.files.write","web.auditLogs.read"
  ],
  "iss": "sdkwork-iam-local", "deployment_mode": "local",
  "environment": "prod", "tenant_id": "100001",
  "iat": 1789099341, "exp": 1789102941      // 2026-09-11 11:42:21 → 13:02:21 (+08)
}
```

**五个环境全部如此**（dev/test/staging/demo/production 的同名全局变量均被注入，只是令牌内容按环境不同）。

**成因链**（已逐环节取证）：

1. `deployments/docker/scripts/entrypoint-standalone.sh:295-328` `ensure_credential_entry_bootstrap_token()` **在所有环境无条件执行**（`environment` 仅用于日志），调用 `gateway issue-credential-entry-bootstrap-token` 签发 IAM 签名令牌，落到 `/etc/sdkwork/webserver/secrets/credential-entry-bootstrap-access-token`（0600，位于每环境独立的持久化命名卷）。容器内实测：五环境该文件**均存在**（1032/1033/1069/1033/1033 字节）。
2. `crates/sdkwork-api-webserver-standalone-gateway/src/app_shell.rs:168` 读取 `SDKWORK_WEBSERVER_CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN` 并在 `:657` 拼 `<script>` 注入 `index.html`——**此处没有任何环境判断**。
3. 对照：同 crate 的 `data_plane/credential_entry_injection.rs:57-71` **明确**实现了门禁（仅 `development`/`dev`），且文件头注释写明 *"staging and production must never inject or embed the token into browser artifacts"*（IAM_CREDENTIAL_ENTRY_SPEC §3/§4/§5）。**该门禁只作用于「模块导入数据面」，未覆盖应用外壳自身。**

**规范冲突**：

- `ENVIRONMENT_SPEC.md` L163：`SDKWORK_ACCESS_TOKEN` … *"It `MUST NOT` be exposed to browser public runtime config."*
- `IAM_CREDENTIAL_ENTRY_SPEC.md` §4/§5：staging/production 不得注入或嵌入该令牌。

**为什么门禁没拦住（盲点 1）**：`check-credential-entry-bootstrap-standard.mjs` 只审计各模块 `apps/*/vite.config.ts`。本次运行它对 sdkwork-skills / sdkwork-terminal / sdkwork-rtc 报了 FAIL，**对 sdkwork-webserver 一行都没有**——webserver 的注入点在 Rust 里，完全不在审计面内。

**附带的功能缺陷**：令牌 TTL 仅 1 小时且只在容器启动时签发一次，容器运行超过 1 小时后页面里嵌的是**已过期**令牌 → 控制台登录元数据静默失效（与 `/runtime-env.js` 返回 404 的现象一致）。

---

### P0-2 版本身份「四源不一致」+ 静默降级

| 版本来源 | 值 |
| --- | --- |
| `sdkwork.app.config.json` `release.currentVersion` | **0.1.5** |
| 部署根 `bundle/manifest.json` / `bundle/image.env` | **0.1.5** |
| 部署根 `bundle/release-state/development/current.json` | **0.1.4** |
| 仓库 `deployments/docker/env/*.env`（被 gitignore 的本地操作面） | **0.1.2** |
| 部署根 `bundle/env/*.env`（今日 11:41–11:42 被改写） | **0.1.2** |
| 五环境实际运行镜像 | **0.1.2** |

**成因**：`bin/docker-deploy.sh` 在不传 `--image-tag` 时从仓库 `deployments/docker/env/<env>.env` 快照重建部署面 env，而该快照停留在 0.1.2 → **一次不带 `--image-tag` 的部署即把整个舰队静默降级**，且没有任何守卫比对「解析出的 tag」与 `release.currentVersion`。

这是 2026-09-05 报告 §2.3 同类缺陷（「版本号与产物不一致」）的**回归**。

**影响**：部署根声称 0.1.5、实际跑 0.1.2——任何信任 bundle 的审计/回滚判断都会得到错误结论。

---

### P0-3 部署绕过 bin/ 唯一操作通道（无证据、无备份、无台账）

今日 11:37–11:42 的重建：

- `target/bin-evidence/evidence.log` **没有任何 2026-09-11 记录**（最后一条是 2026-09-10T14:55:13Z 的 production 0.1.5 升级）。
- `backups/` 下**没有今日的变更前备份**（最近一份是 `sdkwork-webserver-staging-20260910T145231Z`）。
- `release-state/*/ledger.jsonl` 未新增条目。

**规范冲突**：`OPERATIONS_SPEC.md` §1（single operator channel）、§1.2/§3（变更前备份 + 追加式证据台账）。

**盲点 2**：运营门禁第 10 项 `single operator channel (§1)` 只扫描 `package.json` 与操作员文档，**无法发现运行期绕过**——今日实际绕过且门禁仍 PASS。

---

## 3. P1 问题

### P1-1 网关侧车 4/5 不健康，且 `/readyz` 失败后**不可自愈**

- 现象：dev/staging/demo/production 的 gateway 侧车 `unhealthy`；容器内 `healthz=200` 但 `readyz=503`：

  ```
  readiness probe failed readiness_error=skills Snowflake node lease is unhealthy
  ```

- 根因（`sdkwork-database/crates/sdkwork-database-id/src/node_allocator.rs`）：
  - `NodeLease::is_healthy()`（L166-175）= 心跳任务未结束 **且** 租约未过期。
  - `start_heartbeat()`（L834-897）：在 `Ok(false)`（所有权丢失）或 DB 持续失败超过 TTL 时执行 `lease_guard.fence()` 后 **`break`** → 心跳任务结束 → `is_healthy()` **永久为 false**。
  - **没有重新分配、没有重试恢复**；唯一的恢复手段是重启容器。
- 注册表证据（`sdkwork_ai_dev.sdkwork_node_registry`）：

  | node_id | service | instance | started | last_hb | expires |
  | --- | --- | --- | --- | --- | --- |
  | 2 | sdkwork-skills | `sdkwork-skills:88f2f5f8d46a` | 12:02:24 | **12:02:24（从未续约）** | 12:03:24 |
  | 6 | sdkwork-skills | `sdkwork-skills:88f2f5f8d46a` | 12:02:26 | 14:24:10（续约 2h22m 后停） | 14:25:10 |

  `88f2f5f8d46a` 实测为 **development webserver 容器**的 hostname。即：其 skills 模块的租约在 14:24 停跳，之后一直处于 fenced 状态。
- 后果：ID 生成器被 fence 且进程仍在对外服务 → 依赖它的写路径会异常；而边缘与 `/healthz` 仍显示正常。

### P1-2 边缘 `/healthz` 与 `/readyz` 契约分裂

直接实测（development 边缘）：

| 路径 | 状态 |
| --- | --- |
| `/healthz` | **200** |
| `/readyz` | **503** |
| `/` | 200（正常返回 SPA） |

即前端边缘在**后端网关明确 not_ready** 的情况下仍宣告自身就绪。以 `/healthz` 做为编排/负载均衡判据的一方，会把流量持续打进一个无法铸造 ID 的栈。

### P1-3 节点租约表无回收机制

`sdkwork_ai_dev.sdkwork_node_registry` 实测：**43 行，其中 40 行已过期**，最老一条 `2026-09-06 23:05`。表中还留着 9 条已不存在容器（`sdkwork-skills:e08771966f85` 等）的租约。没有任何 GC/reaper，行数将无界增长。

### P1-4 五环境共享同一 Redis，无键空间隔离

- 五环境 `WEBSERVER_REDIS_HOST` 同为 `192.168.31.116:6379`，`WEBSERVER_REDIS_PASSWORD` 全为空，**env 面不存在库号或 key prefix 字段**（`WEBSERVER_REDIS_DB` / 前缀键均无）。
- 实测 db0 键空间（同一实例）：`{cloudrouter-dev}`、`{cloudrouter-demo}`、`{cloudrouter-production}` …（cloudrouter 自带环境前缀），但同时也存在**与环境无关**的键：

  ```
  message:bloom:filter
  openchat:ws:server:nodes
  bull:posts:meta
  bull:agent-automation:meta
  conversation:unread:692417792548409344
  ```

  这些键被五环境共用；开发环境的写入会被生产读取/失效。
- 解析缓存的前缀是**常量** `sdkwork:resolver`（`crates/sdkwork-webserver-resolver-cache/src/config.rs:40`），未按环境派生。
- `deployments/docker/docker-compose.bundle-gateway.yml:87` 的 `SDKWORK_CLOUDROUTER_REDIS_KEY_PREFIX: cloudrouter` 是**硬编码字面量**，五环境相同（不过 cloudrouter 运行期另有环境化前缀，因此 db0 中同时存在 `{cloudrouter-dev}` 与 `{cloudrouter-development}` 两代命名，属无人清理的遗留键）。
- **规范冲突**：`ENVIRONMENT_SPEC.md` §14 明确要求 *"Test-profile isolation tests for database/schema names, **Redis key prefix**, logs, cache, runtime, and temp directories"* 与验收项 *"Test config isolates database/schema, Redis key prefix, …"*。仓库内**没有**任何针对 Redis 前缀隔离的测试。

### P1-5 staging / demo / production 的 doctor 常红，信号价值被摧毁

doctor 明细：

```
staging     FAIL config :: configuration drift: PLACEHOLDER WEBSERVER_REDIS_PASSWORD (value still '<empty>')
demo        FAIL config :: configuration drift: PLACEHOLDER WEBSERVER_REDIS_PASSWORD (value still '<empty>')
production  FAIL config :: configuration drift: PLACEHOLDER WEBSERVER_REDIS_PASSWORD (value still '<empty>')
```

但实测宿主 Redis **本身就是免密**的（`redis-cli` 报 `ERR AUTH called without any password configured`），即「空口令」是**有意姿态**，却没有被编码为豁免标记 → 三个环境的 doctor 永久 FAIL。结果是：**在一个永远红的门禁上，P1-1 这种真故障不再有任何信号**。

---

## 4. P2 / 待确认项

1. **`deploy-standard` 警告**：`profile "cloud.production" not listed in topology.profileFiles`（门禁 PASS，但配置内部不自洽）。
2. **部署根散落文件**：`bundle/env/development.env.bak.20260906T052020Z` 仍在（APPLICATION_DEPLOY_LAYOUT_SPEC §9.1 要求 deploy root 无散落文件）；上次清理只覆盖了 09-09 那批。
3. **`/runtime-env.js` 返回 404**：当前 standalone SPA 不引用它，因此无害，但与 ENVIRONMENT_SPEC §14 的浏览器公共运行时契约不一致，需确认是预期还是缺件。
4. **CORS 白名单规模差异**：dev/test/staging = 569 条、demo = 568、production = **297**。可能合理（生产不列 console 主机），建议确认不是遗漏。
5. **静态节点号与租约空间重叠**：网关容器带 `SDKWORK_IM_ID_NODE_ID=2`（静态），而注册表中 `node_id=2` 当前由 `sdkwork-skills` **租约**持有。需确认静态编号与租约分配是否共用同一 ID 空间——若共用，同 node_id 的两个生成器存在 ID 冲突风险。
6. **门禁工具在本机不可用**：`check-unified-postgres-profile.mjs` 在全工作区模式下无法完成（240s 超时后被杀，EXIT=143），不能作为本地回归手段。`check-embedded-self-loop.mjs` 全量跑约 **1h19m** 才出结果（见 §5.3），同样不宜作本地回归。
7. **构建暂存目录残留 1.3GB，并被门禁误判为「部署面」**：`.sdkwork/runtime/docker-standalone-context/`（1.3GB，`.sdkwork/.gitignore:4` 已忽略、未跟踪）是 0.1.5 镜像构建的 stage context 残留。内容经核对是**构建后的最终快照**（`sdkwork.app.config.json` = 0.1.5、entrypoint 含 3 参修复、0005 校验和 = `7c3dad54…` 与线上契约一致），因此**无正确性风险**；但两个副效应需处理：① 它让 `audit-app-iam-integration` 把该目录当成一个「应用部署面」审计并报 `missing authBoundary`（见 §5.3 假阳性）；② 后续若直接复用该目录 `docker build` 而不重新生成，会烘焙旧快照。
8. **`check-embedded-self-loop` 的 2 条违规位于 agent 暂存目录，非真实部署**：两条命中都在 `sdkwork-api-cloud-gateway/.workbuddy/{backup/cors-20260911,tmp/deployed}/webserver-compose/docker-compose.bundle-gateway.yml`——`.workbuddy/` 被 `sdkwork-api-cloud-gateway/.gitignore:88` 忽略，目录 mtime **11:17–11:26**，与 P0-3 中「11:37–11:42 绕过 bin/ 的整批重部署」属**同一会话时段**，是该会话把 webserver-compose 副本暂存到网关仓留下的产物。反过来说明 `.workbuddy/` 暂存区会污染全工作区门禁（见 §5.4 盲点 4）。

---

## 5. 规范对齐结论（门禁实测）

### 部署面（全部 PASS）

| 门禁 | 结果 | 摘要 |
| --- | --- | --- |
| `check-operations-conformance --root` | **PASS** | fails=0 warns=0 na=0，12/12 项 PASS |
| `check-module-bin --root` | **PASS** | ok=true，issues=[] |
| `check-deploy-standard --root` | **PASS** | ok（含 1 条 topology 警告） |
| `check-webserver-toml-standard --root` | **PASS** | ok |
| `check-base-url-resolution --workspace` | **PASS** | ok，debtTotal=0 |
| `check-shell-portability --root` | **PASS** | 65 files，0 findings |
| `check-package-content-standard --workspace` | **PASS** | PASS |
| `check-cors-standard --workspace` | **PASS** | 78 模块 / 777 carriers / 0 issues |
| `check-api-runtime-parity --workspace` | **PASS** | ok |

运营合规 12 项逐项：`bin/ entrypoint set` / `bin/ README` / `bootstrap layering` / `module wiring hooks` / `secret constants duplication` / `bundle deploy entrypoint` / `bundle release channel (§1.2)` / `compose log rotation (§2.3)`（14 个 compose 全覆盖）/ `environment env examples (§3.1)`（9 份）/ `runbooks (§7)`（4 中英双语）/ `shared primitive isolation (§8)` / `single operator channel (§1)` —— 全 PASS。

### 应用面 / 装配面（全工作区，22 项门禁实测）

| 门禁 | 退出码 | 是否涉及 webserver |
| --- | --- | --- |
| `check-package-content-standard` | 0 | — |
| `check-cors-standard` | 0 | —（78 模块 / 777 carriers / 0 issues，10 条 WARN 全在兄弟仓） |
| `check-rust-http-header-standard` | 0 | — |
| `check-component-api-surface-prefixes` | 0 | — |
| `check-api-runtime-parity` | 0 | — |
| `check-api-assembly-integration-closure` | 0 | — |
| `check-web-module-exports` | 0 | — |
| `check-web-module-contribution-projection` | 0 | — |
| `check-web-module-adoption --strict` | **1** | **否**——2 项全在 `sdkwork-drama` / `sdkwork-agents` |
| `audit-app-iam-integration --strict` | **1** | **是（假阳性）**——39/48，唯一 webserver 条目是 `.sdkwork/runtime/docker-standalone-context: missing authBoundary`，实为 P2-7 的未跟踪构建暂存目录被误判为部署面；其余 8 项在 appbase/community/company/deployments/memory/messaging |
| `check-auth-session-retention` | **1** | **否**——3 项全在 `sdkwork-birdcoder2` / `sdkwork-iam-h5` / `sdkwork-im-pc` |
| `check-embedded-self-loop` | **1** | **否（暂存目录）**——2 项全在 `sdkwork-api-cloud-gateway/.workbuddy/`，见 P2-8 |
| `check-credential-entry-bootstrap` | **1** | **否（盲点）**——FAIL 项全在 skills / terminal / rtc，**webserver 一行都没被审到** |
| `check-unified-postgres-profile` | 143 | 未完成（本机超时被杀） |

**webserver 自身的应用/装配面门禁实际是干净的**：所有 EXIT=1 要么全在兄弟仓，要么是暂存目录导致的假阳性。真正的缺口不在「报红」，而在「**审不到**」——即下面第 4 条盲点。

### 门禁体系的四处盲点（本次审计的核心洞察）

1. **只审文件、不审运行时**：`single operator channel` 与 `operations conformance` 都无法发现今日绕过 bin/ 的实际部署（P0-3）。
2. **只审前端 vite 配置、不审 Rust 注入路径**：`credential-entry-bootstrap` 对 webserver 完全失明（P0-1）。
3. **无「声明版本 vs 实际部署版本」一致性校验**：`check-deploy-standard` 不比对 `release.currentVersion` 与 env 快照镜像 tag（P0-2）。
4. **扫描范围未排除构建/agent 暂存目录**：`audit-app-iam-integration` 把 `.sdkwork/runtime/`（1.3GB stage context）当成部署面（P2-7），`check-embedded-self-loop` 扫进了 `sdkwork-api-cloud-gateway/.workbuddy/`（P2-8）。这两处应加入工具的 `DEFAULT_EXCLUDES`（与 `check-shell-portability` 已排除 `.workbuddy/.sdkwork/tmp` 的做法一致），否则真信号会被暂存残渣淹没。

---

## 6. 运营能力完整性评估

| 能力 | 实现 | 实况 |
| --- | --- | --- |
| `bin/` 标准入口 | **8/8 齐备** + 附加入口 | ✓ |
| 运营 runbook | 10 组中英双语（≥ 要求的 4 组） | ✓ |
| `doctor` | 11 项检查，`--json/--export` | ✓ 可用，但 4/5 环境 FAIL |
| `backup` | create/list/verify/restore | ✓ 可用；**但今日部署未产生备份** |
| `config` | list/show/get/set/diff/validate/edit | ✓；密钥脱敏正常 |
| `release.sh` | rollback + ledger + health 门禁 | ✓；**仅 development 有台账**（P1-3 之外的能力缺口） |
| 日志轮转 | json-file 50m×3，15/15 容器覆盖 | ✓ |
| 版本一致性守卫 | **缺失** | ✗（P0-2） |
| 环境资源隔离（Redis） | **缺失** | ✗（P1-4） |
| 租约自愈 / GC | **缺失** | ✗（P1-1 / P1-3） |
| 有意豁免机制 | **缺失** | ✗（P1-5） |

**回退能力缺口**：除 development 外四环境不存在 `release-state/<env>/ledger.jsonl`（upgrade 不写台账、仅 rollback 写），因此这四个环境的 `bin/docker-deploy.sh rollback` **必须显式 `--to <version>`**，无法自动回到「上一成功版本」。

---

## 7. 建议处置顺序

| 优先级 | 动作 |
| --- | --- |
| **立即** | 为 `app_shell` 的令牌注入加上与数据面一致的环境门禁（仅 development/dev）；同时让 `ensure_credential_entry_bootstrap_token` 在 staging/demo/production 不签发（或改为按需签发 + 短 TTL 自动刷新）。修好前不要在 production 暴露该控制台入口。 |
| **立即** | 补齐版本一致性守卫：`bin/docker-deploy.sh` 解析出的 tag 必须与 `sdkwork.app.config.json release.currentVersion` 一致，否则 fail-fast；把仓库 `deployments/docker/env/*.env` 的 IMAGE_TAG 复位到 0.1.5 并重发五环境。 |
| **高** | 让 upgrade 也写 release 台账（补齐四环境的 rollback 能力）；并把「绕过 bin/ 的实际部署」纳入可检测范围（例如部署面落地一次性 nonce/证据标记）。 |
| **高** | 节点租约自愈：心跳被 fence 后允许重新分配，或让 `/readyz` 不因该依赖永久 fail-closed；补注册表 GC。 |
| **高** | 统一 `/healthz` 与 `/readyz` 语义：边缘就绪必须传递后端网关的就绪状态。 |
| **中** | Redis 按环境隔离：引入 `WEBSERVER_REDIS_DB` 或环境化 key prefix（含 resolver 缓存的常量前缀），并按 ENVIRONMENT_SPEC §14 补隔离测试。 |
| **中** | 为「免密 Redis」等有意姿态引入显式豁免标记，让 staging/demo/production 的 doctor 恢复可信。 |
| **中** | 扩展 `check-credential-entry-bootstrap` 覆盖 Rust 注入路径；扩展 `check-deploy-standard` 增加版本一致性校验。 |
| **中** | 给 `audit-app-iam-integration` / `check-embedded-self-loop` 等全工作区门禁补 `DEFAULT_EXCLUDES`（至少排除 `.sdkwork/runtime`、`.workbuddy`、`.sdkwork/tmp`），消除构建/agent 暂存目录造成的假阳性与真信号淹没（P2-7 / P2-8）。 |
| **低** | 清理 `.sdkwork/runtime/docker-standalone-context/`（1.3GB 构建暂存，P2-7）；`bin/docker-image.sh` 构建结束后自动回收 stage context 更彻底。 |
| **低** | 清理 `bundle/env/*.bak.*` 散落文件；确认 `/runtime-env.js` 与 production CORS 条目数差异；确认 `SDKWORK_IM_ID_NODE_ID=2` 与租约空间是否冲突。 |

---

## 8. 附：审计脚本索引

`.sdkwork/audit/` 下可直接复用：

- `gates.sh` / `gates2.sh` / `gates3.sh` / `gates4.sh` — 门禁批量执行（含超时保护）
- `registry.sh` / `dbmap.sh` / `correlate.sh` — Snowflake 节点注册表与 DB 归属
- `doctor5.sh` — 五环境 doctor 汇总
- `token.sh` / `decode.sh` / `bake.sh` / `proctoken.sh` — P0-1 令牌泄漏取证链
- `probe.sh` / `spa.sh` — 边缘、网关、SPA 探针
- `cors.sh` / `redis.sh` / `redis2.sh` / `logs.sh` — CORS / Redis / 日志配置

（`gates2.txt` / `gates3.txt` / `gates4.txt` / `ops.json` 为原始输出。`gates2.txt` 全量跑完耗时 **1h19m**，22 项门禁退出码见 §5.3；`gates3/4` 是用 `timeout` 包裹后重跑的受限集，用于补齐先前被本机超时中断的项。）
