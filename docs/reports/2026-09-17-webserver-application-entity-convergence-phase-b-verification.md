# 「应用」实体收敛 Phase B 验证报告（挂 deployments 发布页）

- 日期：2026-09-17
- 宿主：Windows（Git Bash）+ 已跑起的 dev 栈（ingress `3800` / 浏览器入口 `5182` / PC vite `5184` / H5 vite `5185`）
- 源码：`<workspace-root>/sdkwork-webserver`（工作树，含未提交改动）
- 范围：Phase B 收口 —— 控制台「我的应用」由 webserver 自建实现改为**桥接 sdkwork-deployments 的规范发布页**；连带测试对齐、全量门禁复验、两条既存技术债修复
- 操作通道：`pnpm <script>`（`MODULE_BIN_SPEC.md` 单操作员通道；未绕过任何 bundle 私有接口）

---

## 1. 结论

| 项 | 结果 |
| --- | --- |
| 静态类型 | `tsc -p tsconfig.json --noEmit` → **0 错误**（改前 34 条，全部在测试文件） |
| 单元/集成测试 | **24/24 文件、126/126 用例** |
| 生产构建 | `build:prod` → `PASS apps/sdkwork-webserver-pc pc standalone.production` |
| 契约物化一致性 | `api:materialize:check` → **web phase-1 contracts are current** |
| 契约门禁 | `api:check`（envelope + operation-patterns + api-assembly）→ **三项 passed**（3 route crates） |
| 应用组合 | `check:app-composition`（`verify-repo`）→ **passed** |
| 数据库布局 | `db:validate` → **Database framework standard passed** |
| 拓扑 / 部署 | `topology:validate` **v5 valid**；`deploy:validate` **ok** |
| Shell 可移植性 | `check-shell-portability` → **65 files, 0 findings** |
| 脚本规范（PC / H5） | `check:pnpm-script-standard` → **ok**（改前 PC 侧红，见 §4a） |
| 前缀归属（跨仓） | **exit 1 / 45 findings**（未变；本轮未涉表前缀） |
| 真实浏览器 | `/console/apps` **正确路由**、**零 console 错误**（会话缺失时按预期跳登录页） |

> **未证到的一步**：带会话的受保护页端到端（登录后控制台真正调 deployments API）。开发态管理员
> 密码只存在于开发者本地环境，本轮**未擅自改动用户 dev 库**，改用桥接输入的静态取证替代（§5）。

---

## 2. 本轮落地的形态（高内聚低耦合）

```
apps/sdkwork-webserver-pc/src/surfaces/WebserverAuthorizedWorkspace.tsx
  ├─ /console/*  resourceRenderers.apps → <DeployAppsManagementSurface …/>
  └─ /admin/*    adminResourceRenderers.apps → <DeployAppsAdminSurface …/>
        ↓ createDeploymentsConsoleClients({ deployBaseUrl, driveBaseUrl, tokenManager })
        ↓ <PublishingAppsPage deployClient driveClient locale/>   ← sdkwork-deployments 的规范实现
```

- **菜单项留在宿主**（`resource: "apps"`），**页面是 deployments 的实现** ⇒ 同一实体只有一个实现。
- 桥接面共用宿主既有 IAM dual-token 会话与 `deployAppApiBaseUrl` / `driveAppApiBaseUrl`；
  webserver 侧**不再有任何应用生命周期实现**（`WebserverWorkspace.tsx` 3350 → 约 1750 行，
  commons 应用专属模块与 i18n 键族已退役）。
- 两个面（console / admin）各自一个薄桥接组件，命名与既有 `DeployDomainManagementSurface` 同族。

---

## 3. 测试对齐（4 个文件，均为「被测代码已退役」的连带失效）

| 文件 | 处置 | 判据 |
| --- | --- | --- |
| `tests/admin-capabilities.test.ts` | 删 1–271 行（`admin application capability` 三例 + `testSourceStorage` / `defaultApplicationSubmission` / `testMediaStorage` 三个夹具） | 保留的 `admin control-plane capability` 两例不依赖应用生命周期；`@sdkwork/webserver-pc-admin-applications` 已退役 |
| `tests/application-source-package.test.ts` | `git rm`（409 行） | 被测模块 `application-source-package.ts` 已删除 |
| `tests/error-message.test.ts` | 删 `WebserverActionError` 导入 + 其 1 条用例 | `tsc` 报 `TS2724`：该类已随应用生命周期退役 |
| `tests/visual/{console-workspace.tsx,.html}` | `git rm`（248 + 12 行） | 引用 3 个已删包（`console-sites` / `console-site-configuration` / `console-deployments`）；**全仓无任何配置引用它**（`grep -rn "tests/visual\|console-workspace.html"` 零命中） |

`tests/console-workspace.test.tsx` 已重写为 13 例，含三条关键断言：
「renderer 优先于 registry」「apps 挂到 deployments 发布页（等 `heading: Applications`）」
「无 renderer 时回落 registry 表格」。

---

## 4. 修掉的两条既存技术债

### (a) `check:pnpm-script-standard` 在 PC 应用上红（HEAD 上同样红 ⇒ 既存，非本轮引入）

```
pnpm script standard failed [root-scripts]: package.json invokes "pnpm exec sdkwork-app"
but does not declare "@sdkwork/app-topology"
```

PC 应用有 4 条脚本调 `pnpm exec sdkwork-app`（`dev:standalone` / `dev:cloud` /
`dev:browser:postgres:standalone` / `stop`），却未声明该 CLI 的提供包 —— 而 root、H5、
mini-program 都声明了；`@sdkwork/app-topology@0.2.0` 就在工作区里。

- 修法：`devDependencies` 增 `"@sdkwork/app-topology": "workspace:*"`（对齐 H5 写法）→ `pnpm install`
  建立链接（`node_modules/@sdkwork/app-topology -> /d/sdkwork-space/sdkwork-app-topology/`）。
- 复验：`pnpm script standard ok: package.json (23 root scripts, 24 package manifests scanned, …)`。
- 顺带：`fflate` 唯一使用者是被删的源码打包模块与其测试 ⇒ 移除死依赖。

### (b) 🔴 生成器模板落后于生成物（真实债，已修 + 已变异证明）

**症状**：`sdks/sdkwork-webserver-{app,backend,internal}-sdk/bin/generate-sdk.sh` 三个副本被 SDK
生成**回退成 `mapfile`**，而 HEAD 是 bash 3.2 兼容的 `while IFS= read -r` 循环，且
`sdkwork-specs/PORTABILITY_SPEC.md` 明文禁 `mapfile/readarray`。

**定位（三跳）**：

| 假设 | 事实 |
| --- | --- |
| SDK 生成器仓 `sdkwork-sdk-generator` | `git grep -ln mapfile` **零命中** |
| 规范仓 `sdkwork-specs` | `git grep -ln "namespace_args\|generate-sdk.sh\|SDK_NAME="` **零命中** |
| **本仓** `tools/materialize_web_phase1_contracts.mjs:944` | ✅ 就是它（JS 模板字符串） |

⇒ 有人修了**生成物**却漏了**模板**，每次 `api:materialize` 都把修复冲掉。
**可复用定位手法**：拿生成物里的**特征符号**（函数名，不是文件名/命令名）`git grep -ln`。

**字节级变异测试**（`tmp/mutate-sdk-shell-portability.py`；还原放 `try/finally` 并核对 sha256）：

| 阶段 | 生成脚本含 `mapfile` 命令 | `check-shell-portability` |
| --- | --- | --- |
| 变异（模板回退 `mapfile`） | **3** | **FAIL** — `generate-sdk.sh:64: BASH4: mapfile/readarray (bash 4+)` |
| 还原（模板修复） | **0** | **PASS** — `65 files, 0 findings (bash -n: on)` |

模板 sha256 前后一致（`455af9c48f0f32add70334b305007faac8dec2247cfc5ec53517bc76ae85ceab`）；
三个生成脚本 `git diff` 归零（与 HEAD 逐字节相同）。

**顺手关掉的盲区**：`PORTABILITY_SPEC.md` 称该门禁为**强制**，但
`grep -rn "check-shell-portability" --include=package.json` 全仓**零命中** ⇒ 回归才会静默存活。
已加 `check:shell-portability` 并挂进 `_sdkwork:check`（紧跟 `check:browser-build-scripts`），
`check:pnpm-script-standard` 复验通过（158 root scripts）。

---

## 5. 真实浏览器验证与替代取证

### 5.1 已取证：路由与启动链路

用已装 Chrome + CDP 直连（零安装）驱动 `http://127.0.0.1:5184/console/apps`：

```json
{
  "href": "http://127.0.0.1:5184/auth/login?redirect=%2Fconsole%2Fapps",
  "lang": "zh-CN",
  "readyState": "complete",
  "headings": ["h2: 扫码登录", "h1: 账号登录"],
  "consoleErrors": [],
  "failedRequests": [{ "status": 404, "url": "http://127.0.0.1:5184/favicon.ico" }]
}
```

⇒ 应用真实启动、路由正确、**零 console 错误**（唯一网络噪声是 favicon 404）；未登录时按设计跳转。

### 5.2 未取证：带会话的受保护页 ⇒ 改为对**桥接输入**静态取证

开发态管理员密码不在仓库内（`apps/sdkwork-webserver-pc/.env.development.example` 只有空的
`SDKWORK_ACCESS_TOKEN`，真实凭据由 credential-entry Vite 插件在 dev 进程内解析）。**不擅自重置
用户 dev 库密码**，改为排除"桥接输入是空 ⇒ 页面静默失败"这类最危险缺陷：

| 检查 | 结果 |
| --- | --- |
| 运行时是否一等字段 | `packages/sdkwork-webserver-pc-core/src/index.ts:12/150/158/163-165`：类型声明 + `readDeployBaseUrl(value, origin, environment)` 兜底 |
| 是否进入物化白名单 | `scripts/materialize-runtime-env.mjs:25` ✅ |
| 构建产物实际值 | `dist/standalone/{dev,prod}/runtime-env.json` → `deployAppApiBaseUrl = "/"`（同源，共享 IAM dual-token） |

**待用户提供**：开发态管理员凭据（或授权重置 dev 库密码）后，可补一条"登录 → 控制台我的应用
→ 列表来自 `deploy_app`"的端到端用例。

---

## 6. 一个自伤假阳性（值得记录）

修复后 `grep -l mapfile sdks/*/bin/generate-sdk.sh` **仍列出三个文件** —— 命中的是修复时添加的
注释 `# bash 3.2 has no mapfile (macOS /usr/bin/bash)`。

⇒ 判"有没有用某命令"**必须锚行首**：`grep -nE "^[[:space:]]*mapfile\b"`。
这是所有 token 扫描类门禁的共同假阳性来源（同源：`audit-darkmode-pairing.mjs` 必须先剥注释、
`check-shell-portability` 用 `PORTABILITY:allow` 标记机制）。

---

## 7. 未决（需拍板）

| 项 | 说明 |
| --- | --- |
| 带会话端到端验收 | 需开发态管理员凭据或授权重置 dev 库密码（§5.2） |
| 后端 `webserver_application` 残留 | `crates/sdkwork-intelligence-webserver-repository-sqlx/src/applications.rs` 仍有 **26 处** `webserver_application` SQL。控制台入口已不走它，但该表在活库仍在、模块仍在读它 |
| Phase 3 剩余 25 张表退役 | 被 **53 条活路由 + 24 个 repository 模块**绑住 ⇒ 硬前置是 Phase 2 |
| `webserver_source_version` | 16 文件 / 6 个 legacy 面，是否一并退役 |
| `web.applications.*` 旧权限码 | 仍守卫 **32 条活 app-api 路由**（`.read` 2 / `.write` 30）⇒ 只能维持加性别名期，待应用路由退役后进入四步改名第 4 步 |
| 孤儿权限码 `web.domains.read` | IAM 目录里没有，仅 1 条 app-api 路由在用 |

---

## 8. 复现命令

```bash
cd <workspace-root>/sdkwork-webserver/apps/sdkwork-webserver-pc
pnpm typecheck && pnpm test && pnpm build:prod && pnpm check:source-config && pnpm check:pnpm-script-standard

cd <workspace-root>/sdkwork-webserver
pnpm api:materialize:check && pnpm api:check && pnpm check:app-composition
pnpm db:validate && pnpm topology:validate && pnpm deploy:validate
pnpm check:shell-portability && pnpm check:database-prefix-ownership   # 后者预期 exit 1 / 45 findings（跨仓既存）
```
