# sdkwork-webserver 多环境容器化交付与 import 平面验证（0.1.5 → 0.1.8）

- **日期**：2026-09-11（作业跨至 2026-09-12 凌晨）
- **范围**：WSL Ubuntu 22.04 上用 `bin/` 唯一操作通道打包镜像 → 五环境原地升级启动；
  webserver 自身 **standalone** 部署、import 走 **cloud** 平面；
  每个实例 import **全部五个生命周期环境**，并真正对外服务**所有** import 环境的静态文件。
- **通道**：`bin/docker-image.sh build` / `bin/docker-deploy.sh upgrade`（未直连 docker，未手改容器）
- **结论**：**三个 P0 缺陷已定位并修复**。最终验收矩阵 **25/25**（5 实例 × 5 环境，每格都返回
  该环境**自己**的构建产物）。

---

## 1. 交付物与最终状态

| 交付物 | 值 |
| --- | --- |
| 镜像 | `registry.sdkwork.com/apps/sdkwork-webserver-standalone:0.1.8`，id `94b6ef4bab9c`，5.86 GB |
| 上一版 | `…:0.1.7` id `0e8419a4ce13`（路由已修、静态内容未切） |
| server 归档 | `dist/release/sdkwork-webserver-linux-x64-standalone-server-0.1.8.tar.gz`，`bytes=72369229 entries=634` |
| 入口脚本 sha256 | 0.1.6 `b4c9c1da…`｜0.1.7 `9ac902f9…`｜**0.1.8 `df21fb0a1a45812011a0f3270aada146c9fd9526ff0b509caee72977249659e2`**（镜像内 `/usr/local/bin/sdkwork-webserver-entrypoint` 与仓库源**逐字节一致**） |
| 版本清单 | `sdkwork.app.config.json` → `release.currentVersion=0.1.8`、`latest.BETA=0.1.8` |
| 规范 | `../sdkwork-specs/SDKWORK_WEBSERVER_SPEC.md` §17.3.2 新增/收窄 3 条 Rules |

### 最终舰队状态（五实例完全一致）

| ENV | IMAGE | HEALTH | RESTARTS | 聚合器 include | env-dispatch 残留 |
| --- | --- | --- | --- | --- | --- |
| development | 0.1.8 | healthy | 0 | 351 | 0 |
| test | 0.1.8 | healthy | 0 | 351 | 0 |
| staging | 0.1.8 | healthy | 0 | 351 | 0 |
| demo | 0.1.8 | healthy | 0 | 351 | 0 |
| production | 0.1.8 | healthy | 0 | 351 | 0 |

`DEPLOY_OVERALL_EXIT=0`（逐环境 `DEPLOY_EXIT[e]=0` + `GATE[e] health=healthy`）。

---

## 2. 验收矩阵：单实例服务**全部环境**（核心要求）

行 = 实例，列 = 该环境的对外主机，格 = `HTTP/服务到的 index hash/期望`。
产物指纹取自工作区实现（`apps/sdkwork-im-{pc,h5}/dist/cloud/<alias>/`）：

| alias | dev | test | staging | demo | prod |
| --- | --- | --- | --- | --- | --- |
| PC | `b2vHxOuO` | `D_fcujC4` | `vuGjvqsa` | `Dgr_hmt9` | `CbMQepBX` |

**0.1.7 基线（修复前）——5/25**：

```
inst\host    im-dev            im-test           im-staging        im-demo           im
development  200/b2vHxOuO/ok   200/b2vHxOuO/!!   200/b2vHxOuO/!!   200/b2vHxOuO/!!   200/b2vHxOuO/!!
test         200/D_fcujC4/!!   200/D_fcujC4/ok   200/D_fcujC4/!!   200/D_fcujC4/!!   200/D_fcujC4/!!
staging      200/vuGjvqsa/!!   200/vuGjvqsa/!!   200/vuGjvqsa/ok   200/vuGjvqsa/!!   200/vuGjvqsa/!!
demo         200/Dgr_hmt9/!!   200/Dgr_hmt9/!!   200/Dgr_hmt9/!!   200/Dgr_hmt9/ok   200/Dgr_hmt9/!!
production   200/CbMQepBX/!!   200/CbMQepBX/!!   200/CbMQepBX/!!   200/CbMQepBX/!!   200/CbMQepBX/ok
MATRIX_SCORE=5/25
```

**0.1.8 终态——25/25（全绿）**：

```
inst\host    im-dev.sdkwork.com  im-test.sdkwork.com  im-staging.sdkwork.com  im-demo.sdkwork.com  im.sdkwork.com
development  200/b2vHxOuO/ok     200/D_fcujC4/ok      200/vuGjvqsa/ok         200/Dgr_hmt9/ok      200/CbMQepBX/ok
test         200/b2vHxOuO/ok     200/D_fcujC4/ok      200/vuGjvqsa/ok         200/Dgr_hmt9/ok      200/CbMQepBX/ok
staging      200/b2vHxOuO/ok     200/D_fcujC4/ok      200/vuGjvqsa/ok         200/Dgr_hmt9/ok      200/CbMQepBX/ok
demo         200/b2vHxOuO/ok     200/D_fcujC4/ok      200/vuGjvqsa/ok         200/Dgr_hmt9/ok      200/CbMQepBX/ok
production   200/b2vHxOuO/ok     200/D_fcujC4/ok      200/vuGjvqsa/ok         200/Dgr_hmt9/ok      200/CbMQepBX/ok
MATRIX_SCORE=25/25 cells serving their own environment's build
```

即：**任一实例**（rows）访问**任一环境**的模块域名（columns）都返回**该环境自己的** PC 构建。
这正是「每个 webserver 的所有环境都能正常对外提供 import 所有环境的服务」。

### §17.3 import 平面合规

| 检查 | dev | test | staging | demo | prod |
| --- | --- | --- | --- | --- | --- |
| 活动聚合器 `import.conf` 存在 | yes | yes | yes | yes | yes |
| 双集合 `import.conf.{cloud,standalone}` 均物化 | yes | yes | yes | yes | yes |
| `product-edge-nginx.conf` 并入 | yes | yes | yes | yes | yes |
| 孤儿模块残留 | 0 | 0 | 0 | 0 | 0 |
| include 数 | 351 | 351 | 351 | 351 | 351 |

**include 精确记账**（`import.conf`，34908 B）：**351 条 `include` 指令 = 350 × `nginx.cloud.<env>.conf` + 1 × `product-edge-nginx.conf`**；
`nginx.standalone.*` = **0**；非 include 行 = 0；**351/351 全部在磁盘上解析成功**（`missing=0`）；
引用 **70** 个不同模块 ⇒ **70 模块 × 5 环境 = 350**，恰好均匀。

- **活动集合 = cloud**（用户要求）：`import.conf` 与 `import.conf.cloud` **sha256 逐字节相同**
  （`1febe36d9f27635dd2e412e959b4e89d35bdb537c05924e7b2ad028b9745372b`，34908 B）；
  0.1.8 实例日志为
  `profile=standalone path=…nginx.cloud.<env>.conf`（实例自身 standalone 部署 + cloud import，正确组合）。
- `import.conf.standalone`（`c30d1a9a…`，36673 B）已物化但**非活动**——符合 §17.3「两套集合始终物化，profile 决定激活哪套」。
- **每环境覆盖均匀**：`development/test/staging/demo/production` 各 **70** 条引用（无环境被丢）。
- `layout-imports.<profile>.toml` **不存在属合规**：仅当模块声明 layout-v3 import（`layout_count > 0`）才物化，
  本舰队全部以 nginx sidecar 交付 ⇒ 正确移除，且 `import.conf` 已满足 `module_imports_present` 判定。

---

## 3. 访问 URL（快速验证）

### 3.1 控制台 / 管理面（直连 127.0.0.1，无需 DNS）

| 环境 | 边缘 HTTP | 边缘 HTTPS | 管理 `/healthz` | 管理 `/readyz` |
| --- | --- | --- | --- | --- |
| development | http://127.0.0.1:80/ | https://127.0.0.1:443/ | http://127.0.0.1:13800/healthz | 200 |
| test | http://127.0.0.1:18898/ | https://127.0.0.1:28430/ | http://127.0.0.1:18888/healthz | 200 |
| staging | http://127.0.0.1:18099/ | https://127.0.0.1:38431/ | http://127.0.0.1:18081/healthz | 200 |
| demo | http://127.0.0.1:19098/ | https://127.0.0.1:38432/ | http://127.0.0.1:19080/healthz | 200 |
| production | http://127.0.0.1:18098/ | https://127.0.0.1:38430/ | http://127.0.0.1:18080/healthz | 200 |

以上 HTTP 边缘与管理面**全部 200**（HTTPS 见 §5.3 的 TLS 范围说明）。

### 3.2 模块域名（`*.sdkwork.com` 不解析，需 `--resolve` 或写入 `/etc/hosts`）

**任一实例端口都能回答全部 5 个环境域名**——可直接拿 `development` 实例（:80）验证全部环境：

```bash
# 五个环境域名，都打到同一个 development 实例的 :80
curl --resolve im-dev.sdkwork.com:80:127.0.0.1        http://im-dev.sdkwork.com:80/
curl --resolve im-test.sdkwork.com:80:127.0.0.1       http://im-test.sdkwork.com:80/
curl --resolve im-staging.sdkwork.com:80:127.0.0.1    http://im-staging.sdkwork.com:80/
curl --resolve im-demo.sdkwork.com:80:127.0.0.1       http://im-demo.sdkwork.com:80/
curl --resolve im.sdkwork.com:80:127.0.0.1            http://im.sdkwork.com:80/
```

各环境「自己的」实例端口（等价效果，端口更直观）：

```bash
curl --resolve im-dev.sdkwork.com:80:127.0.0.1        http://im-dev.sdkwork.com:80/        # dev  build b2vHxOuO
curl --resolve im-test.sdkwork.com:18898:127.0.0.1    http://im-test.sdkwork.com:18898/    # test build D_fcujC4
curl --resolve im-staging.sdkwork.com:18099:127.0.0.1 http://im-staging.sdkwork.com:18099/ # staging  vuGjvqsa
curl --resolve im-demo.sdkwork.com:19098:127.0.0.1    http://im-demo.sdkwork.com:19098/    # demo     Dgr_hmt9
curl --resolve im.sdkwork.com:18098:127.0.0.1         http://im.sdkwork.com:18098/         # prod     CbMQepBX
```

> **本机探测铁律**：WSL shell 导出了 `http_proxy/https_proxy=http://127.0.0.1:7897`（Clash）。
> curl 会把 localhost 探测也走代理，HTTPS 会得到 `tlsv1 alert access denied` 的**假 000**。
> 所有本地探测请加 **`--noproxy '*'`**（`no_proxy` 含 `127.*`，但 `*` 更保险）。

---

## 4. 三个 P0 缺陷

### 4.1 P0-A：import id 碰撞（0.1.6 → 0.1.7 修）

**症状**：多环境 webserver 只命中一个虚拟主机（日志恒为
`virtual_host_id=server-dev-sdkwork-com-80 route_id=route-6-3`），各环境模块域名返回**实例自己的**控制台 shell。

**根因**：`runtime_config.rs#upsert_import_entry` 按 `id` **覆盖**（后写替换先写），而聚合器为每模块发
**5 条 include**（每环境一条）且全部解析为**裸模块 id** ⇒ 运行时把每模块 5 个 sidecar **静默压成 1 个**
（最后列出的 production），其余环境域名全部落到 listener default host。

**修法**：import id 限定为 `<module>-<profile>-<environment>`
（`module_import_id_from_nginx_sidecar`，helper 迁至 `module_imports.rs`）。
改前先实证 355 个 cloud sidecar 的 `server_name` **跨模块 0 重复**（7703 token）。
10 个 `deployments/docker/env/*.env(.example)` 显式声明
`SDKWORK_WEBSERVER_IMPORT_ENVIRONMENTS=development,test,staging,demo,production`。

**验证**：`import_aggregator_keeps_every_commissioned_environment` 断言 5 个存活 id；
0.1.7 部署后五实例 × 五环境域名**全部 200**（§2 基线矩阵每格 200）。

### 4.2 P0-B：孤儿模块 `sdkwork-env-dispatch` 污染 import（0.1.8 修）

**症状**：`test` 容器 crash-loop：`RestartCount` 递增、`Health=starting`、**`ExitCode=0`**；
日志被 `seeding placeholder shell …` 淹没。

**定位链**：
1. 过滤噪声后才挖到真错：
   `merged module-imports configuration failed: Web Server config failed validation (112 diagnostic(s))`，
   紧跟 112 条 `server name X on listener listener-0-0-0-0-80 is already owned by Y`。
2. 诊断显示 offender vhost 的 `serverNames[0..15]` 是 `im-test.*`（无人冲撞），`[16..]` 才是
   `api-test.* / server-test.* / server-app-test.* / server-admin-test.*` ⇒ offender 是一个把
   **test 整块矩阵**一起声明的 vhost。112 = 64(test 四前缀) + 16(api-staging) + 16(api-demo) + 16(api. on :443)。
3. `grep -rl` 找到两个来源：`sdkwork-env-dispatch/nginx.cloud.development.conf`（**10.6 KB 全矩阵**）
   与各模块自己的 sidecar。

**根因**：`sdkwork-env-dispatch` 是**前代设计的残留**，其 `server.common.toml` 自述：

> `description = "Cross-environment Host dispatch edge for single-host multi-env deployments (dev instance only)"`

upstream 指向 `host.docker.internal:18898 / 18099 / 18098 / 19080`——即「单 :80 实例按 Host 转发到
其它环境容器」，正是「每实例自服务全环境」要取代的旧方案。它**不在** Windows 工作区、
**无 `AGENTS.md` / 无 app manifest / 无 `.git`**、全仓 `grep -rn env-dispatch` = **0 命中**，
却因带 `deployments/webserver/server.common.toml` 而被 `discover_importable_modules` 当成模块 import。

**为何 0.1.6 没炸**：id 覆盖让它 5 个 sidecar 只剩 production（1.1 KB，无害）；
0.1.7 保住全部 5 个 ⇔ development（10.6 KB 矩阵）复活 ⇒ 暴露冲突。
**教训：「修好一个 bug 会暴露下一个」在这条链上是常态。**

**修法**：
1. **运维（立即解阻）**：安全门校验（无 AGENTS.md / 无 manifest / 无 .git）通过后**备份 + 移出**（非删除）：
   - 备份：`/opt/deploy/backups/pruned-env-dispatch-20260912-001323`（18 文件）
   - 移出：`/opt/deploy/sdkwork-space/.pruned-env-dispatch-20260912-001323`
   - 术后：`sdkwork-*` 99 个、含 `server.common.toml` 76 个、**孤儿 0 个**
2. **代码（防复发）**：`discover_importable_modules` 增加 `module_is_governed_repo`（要求 `AGENTS.md`），
   非受治理目录 `skipping non-governed path … (no AGENTS.md; not a fleet module)`。
   `AGENTS.md` = 舰队枚举标记（`SDKWORK_WORKSPACE_SPEC`；实测 100 个 `sdkwork-*` 中 99 个有）。

**验证**：重启后 `RestartCount=0`、`health=healthy`，配置校验全绿（每 sidecar `unreachable=0`）；
聚合器 include **357 → 352 = 357 − 5**（裁剪当时读数），终态 0.1.8 镜像下精确为 **351**
（350 cloud sidecar + 1 product-edge，见 §1 记账），五实例一致，`grep -c env-dispatch import.conf = 0`。

> 关键结构性事实：`/opt/deploy/sdkwork-space` **不是 git 树**，是长期复用的模块 checkout 根
> ⇒ 会累积历史残留；`discover_importable_modules` 是唯一枚举点（`SDKWORK_SPACE_IMPORT_MODULES`
> 由它覆写）⇒ 守卫必须放在这里。

### 4.3 P0-C：环境级静态根改写不生效（0.1.8 修）

**症状**：0.1.7 路由正确（各环境 `virtual_host_id` 各自命中），但**每个环境域名返回实例自身环境的产物**
（§2 基线矩阵「行恒定」即指纹）。

**根因**：`module_imports.rs::environment_scoped_adaptive_root` 用**仓库目录 id** 拼前缀
（`/usr/share/sdkwork/sdkwork-im/web/`），而 sidecar 声明的是 **Adaptive Web runtime code**：
`root /usr/share/sdkwork/im/web/pc`。`strip_prefix` 因此**必然失败**，函数静默返回 `None`
（**零诊断**），于是继续使用声明的根——而 `/usr/share/sdkwork/im/web/pc` 是指向**实例自身环境**
产物的符号链接。

反证：entrypoint 侧 `module_env_web_static_root` 是「在最后一段前插 alias」的**段无关**实现，
两侧**并非**注释所称 byte-for-byte 同步——**我自己的注释掩盖了实现偏差**。

**修法**：从声明根**自身**推导计划（`rsplit_once('/')` 后插 `<alias>/`），只校验形状 `…/web/{pc,h5}`；
`<module>` 段**永不解释**。其余静态根保持不变；兄弟根不存在时回落声明根（向后兼容）。

**验证**：新增 `environment_scoped_adaptive_root_follows_the_declared_root_segment`
（正常改写 / 未物化回落 / 尾斜杠容忍 / 非 Adaptive Web 根与过深路径拒绝）**通过**；
`cargo test -p sdkwork-webserver-core --lib module_imports::` → **7 passed / 0 failed**；
端到端 §2 矩阵 **5/25 → 25/25**。

---

## 5. 舰队事实与观测（易踩）

### 5.1 主机命名铁律
环境后缀 = **dist alias**，**production 裸名无后缀**：
`im-dev` / `im-test` / `im-staging` / `im-demo` / **`im`**；`server-` / `api-` / `server-app-` /
`server-admin-` 同族。用 `im-development.` / `im-production.` 探测会因**无任何 sidecar 声明**而 502
——**不是缺陷**（先 `grep -rl '<host>' $SPACE --include='nginx.*.conf'` 确认有没有人声明）。

### 5.2 端口
dev 80/443/13800；test 18898/28430/18888；staging 18099/38431/18081；
demo 19098/38432/19080；production 18098/38430/18080。五实例**共享**同一 checkout 根。

### 5.3 TLS 范围（旁证，非本次改动引入）
**仅 `production` 系列的 sidecar 声明 `listen 443 ssl`**；`development/test/staging/demo` 的 sidecar
**只声明 `listen 80`**。实测：

```
https://im.sdkwork.com:38430        -> 200  index-CbMQepBX.js     # 生产/apex 有 TLS
https://im-test.sdkwork.com:28430   -> tlsv1 alert access denied   # 无 TLS 监听，rustls 拒绝该 SNI
```

即**非生产环境主机当前仅 HTTP**；这是模块 `deployments/webserver/` 侧的唯一真源决定的范围
（§17.3 高内聚：webserver 不复制不改写模块配置）。如需为非生产主机开 TLS，属**舰队级 sidecar 决策**，
不在本次范围——列为**建议**而非缺陷。

### 5.4 部署门禁的时序缺口（本次实测）
`docker-deploy.sh` 的健康门禁只探**管理面** `127.0.0.1:3800/healthz`，而**边缘数据平面**绑定时更晚：
production 实测 start 16:33:12 → 校验 16:33:38 → `serving merged module-imports data plane` 16:33:59
（**滞后约 47 s**）。故 `GATE=healthy` 通过的一瞬，`https?://<edge>` 仍可能 502。
→ **边缘验收必须等数据平面就绪**（以 `serving merged module-imports data plane` 行为准），不要只信容器 health。

### 5.5 重启安全性：镜像 tag 的四个版本源不一致，但**不会静默降级**（已实测收敛）

交付后发现 bundle 内的镜像 tag 元数据与运行态不一致，逐条实测三条可能路径：

| 版本源 | 值 |
| --- | --- |
| `sdkwork.app.config.json` → `release.currentVersion` | **0.1.8** ← `bin/docker-deploy.sh` 的默认 tag 取此处 |
| 运行容器实际镜像 | **0.1.8** |
| bundle `image.env` | 0.1.7 |
| bundle `manifest.json` (`imageTag`) | 0.1.7 |
| bundle `env/<env>.env` × 5 | 0.1.2 |
| bundle `release-state/<env>/current.json` | 仅 development 有（0.1.4），其余 4 个**为空** |

**结论：三条路径都不会「静默降级」到旧镜像**，实测证据：

```
1. docker compose restart
   -> 从不重新解析 image:（沿用同一 image ID）                              : SAFE
2. 裸 docker compose up -d（compose 目录内无 .env，故按 ${VAR:-0.1.0} 兜底）
   -> compose 在插值阶段即报错退出，0.1.0 兜底根本不可达：
      error while interpolating x-webserver-common.environment.SDKWORK_WEBSERVER_ENVIRONMENT:
      required variable SDKWORK_WEBSERVER_ENVIRONMENT is missing a value: required
      （单独设 ENVIRONMENT 亦报 POSTGRES_PASSWORD / MGMT_HOST_PORT 缺失）        : FAIL-CLOSED
3. bundle deploy.sh --environment <env>（不带 --image-tag）
   -> 版本守卫 die，退出码 1：
      ERROR: version mismatch for environment test:
      env/test.env pins SDKWORK_WEBSERVER_IMAGE_TAG=0.1.2 but this bundle ships 0.1.7.
      Refusing to silently change the deployed image version.
      （--image-tag 0.1.8 显式路径 exit=0，正常）                                : FAIL-CLOSED
```

`env/*.env` 之所以滞后是**设计使然**：`bin/docker-deploy.sh` 的默认 tag 来自 app manifest（现 0.1.8），
而 bundle 的 `persist_runtime_overrides()` **只回写** `*_HOST_PORT` / `*_IMPORT_HTTP_HOST_PORT` /
`*_HTTPS_HOST_PORT` / `PRIMARY_DOMAIN`，**不回写 `SDKWORK_WEBSERVER_IMAGE_TAG`**
⇒ `--image-tag 0.1.8` 是一次性覆盖，env 文件保持历史值 0.1.2。

守卫本身即本仓 2026-09-11 部署审计的 P0-② 修复（`bundle/deploy.sh:215-235` 注释原文记录了事故：
「a single untagged upgrade moved all five environments from 0.1.5 back to 0.1.2 exactly this way」）。

> **残留项（元数据层面，非行为缺陷）**：四个版本源仍互不相同。若要让 `env/*.env` 成为可信单一源，
> 需在升级时同步回写 tag——但**不建议**改 `bin/` 行为，因为当前 fail-closed 语义比「自动跟随」更安全。
> 列为**观察项**，未做任何变更。

### 5.6 诊断技法

- **fail-closed 校验失败会让进程以 0 退出**，`restart: unless-stopped` 于是无限重启：
  `RestartCount` 增长 + `Health=starting` + **`ExitCode=0`** 即此类，**不是 OOM**（`OOMKilled=false`）。
- **`docker logs` 里 ERROR 后紧跟多行 diagnostics**（`format_webserver_config_error` 逐条换行追加），
  `tail` 只会看到噪声：
  ```bash
  docker logs <c> 2>&1 | grep -vE "seeding placeholder shell|no apps/|module nginx include" | tail -60
  docker logs <c> 2>&1 | grep -nE "error|fatal|panic|failed" | grep -v "warning: " | tail -40
  docker logs <c> 2>&1 | sed -n "<ERROR行号>,+115p"     # 取完整 diagnostics
  ```
- **占位 shell 噪声是预期的**：多数兄弟模块没有 cloud dist 产物 ⇒ entrypoint 会
  `seed warning: <mod> <surface>: no apps/*-… build … seeding placeholder shell at …`。
- 路由黄金证据行（每成功请求一条）：
  `request served … virtual_host_id=<host>-80 route_id=route-… status=200`；
  **只对成功服务写**，所以「没有 log 行」本身说明**没匹配到任何 vhost**。

---

## 6. 既存问题（**不在本次范围，未静默修复**）

1. **浏览器 typecheck 阻塞打包**：生成 SDK 漂移——`apis/backend-api/web/openapi.yaml` 用 `q`
   （API_SPEC §16.4），而 checked-in `ApplicationListParams` 仍是 `keyword` ⇒
   `src/data-source.ts(39,83): TS2353`。本次以 `--skip-pc-build --skip-h5-build` 复用既有 dist 绕开。
2. **2 个既存单测失败**（夹具漂移）：`config::server_toml::tests::{merges_im_example_production_hosts_for_all_base_domains,
   merges_the_product_webserver_layout_for_both_profiles}` 断言 14 个 `[[http.server]]`，
   而 `deployments/webserver/server.production.toml` 有 16 个。另 `loads_standalone_profile_with_gateway_target`
   **单独跑 ok**、全量跑 FAILED ⇒ **既存测试串扰**（判失败前**先单跑**）。
3. **工作区 CRLF 大面积漂移**：`git status` 324 文件、`153958 insertions = 153958 deletions`，
   与本次改动无关（本次仅改 `module_imports.rs`、入口脚本、env 文件、manifest、spec）。
4. **bundle 版本元数据四源不一致**（行为安全，见 §5.5）：app manifest / 运行镜像 = 0.1.8；
   `bundle/image.env`、`bundle/manifest.json` = 0.1.7；`bundle/env/<env>.env` × 5 = 0.1.2；
   `bundle/release-state/<env>/current.json` 仅 development 有值（0.1.4），其余 4 个为空。
   实测三条重启/升级路径均 **fail-closed**，不会静默降级；属**观察项**，未改 `bin/` 行为。
   与 `2026-09-11-webserver-deployment-audit.md` 中"组件版本源四处易矛盾"为同一既存问题。

---

## 7. 复现命令

```bash
# ---- 打包（WSL，复用 PC/H5 静态产物以绕开既存 typecheck 漂移）----
cd /mnt/e/sdkwork-space/sdkwork-webserver
export SDKWORK_IMAGE_NO_PULL=1 SDKWORK_BROWSER_SKIP_TYPECHECK=1
export SDKWORK_RELEASE_STAGE_PARENT="$HOME/sdkwork-release-stage"   # 必须 ext4
node scripts/webserver-release.mjs package --deployment-profile standalone \
  --architecture x64 --environment production --version 0.1.8 --skip-pc-build --skip-h5-build
SDKWORK_IMAGE_SKIP_RELEASE_BUILD=1 bash bin/docker-image.sh build --image-tag 0.1.8

# ---- 五环境逐环境原地升级（风险递增，失败即停）----
for e in test development staging demo production; do
  bash bin/docker-deploy.sh upgrade --environment "$e" --image-tag 0.1.8 --yes || break
done

# ---- 验收矩阵：任一实例服务任一环境（务必 --noproxy '*'）----
for pair in "development:80:b2vHxOuO" "test:18898:D_fcujC4" "staging:18099:vuGjvqsa" \
            "demo:19098:Dgr_hmt9" "production:18098:CbMQepBX"; do
  env=${pair%%:*}; rest=${pair#*:}; port=${rest%%:*}; want=${rest##*:}
  case $env in development) h=im-dev.sdkwork.com;; production) h=im.sdkwork.com;;
               *) h=im-$env.sdkwork.com;; esac
  got=$(curl -sS --noproxy '*' --resolve "$h:$port:127.0.0.1" "http://$h:$port/" \
        | grep -oE 'index-[A-Za-z0-9_-]+\.js' | head -1)
  printf '%-12s %-26s want=index-%s.js got=%s\n' "$env" "$h" "$want" "${got:-<none>}"
done

# ---- 孤儿审计：受治理仓必须有 AGENTS.md ----
for d in /opt/deploy/sdkwork-space/sdkwork-*; do
  [ -f "$d/deployments/webserver/server.common.toml" ] && [ ! -f "$d/AGENTS.md" ] && echo "ORPHAN: $d"
done

# ---- import 平面精确记账（350 cloud sidecar + 1 product-edge = 351）----
ID=$(docker ps -q --filter name=sdkwork-webserver-development-i1-webserver | head -1)
docker exec "$ID" cat /etc/sdkwork/webserver/imports.d/import.conf > /tmp/import.conf
grep -cE '^\s*include\s' /tmp/import.conf                      # 351
grep -cE '^\s*include\s.*nginx\.cloud\.' /tmp/import.conf       # 350
grep -cE '^\s*include\s.*nginx\.standalone\.' /tmp/import.conf  # 0
sort -u <(grep -oE 'sdkwork-space/[a-z0-9-]+/' /tmp/import.conf) | wc -l   # 70 模块

# ---- 重启安全性：三条路径都不应静默降级（§5.5）----
cd /opt/deploy/sdkwork-webserver/bundle
bash deploy.sh --environment test --dry-run; echo "guard_exit=$?"          # 1 = fail-closed
bash deploy.sh --environment test --image-tag 0.1.8 --dry-run; echo "ok_exit=$?"  # 0
grep -n 'image:' compose/docker-compose.bundle.yml   # ${SDKWORK_WEBSERVER_IMAGE_TAG:-0.1.0}
```
