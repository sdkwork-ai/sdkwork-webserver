# sdkwork-space 全舰队模块完整性与对齐审计

- 日期：2026-09-11
- 范围：`E:/sdkwork-space` 下**全部 100 个受治理仓**（不再只审 79 个）
- 触发：`逐个模块检查 sdkwork-space，确保 sdkwork-space 下的所有的独立模块都已经完整并确保对齐`
- 结论一句话：**舰队口径此前是错的（79 ≠ 100）；修好口径后又发现 4 个门禁层缺陷（含 1 对自相矛盾的门禁、1 处死代码常量）；拉平口径后的诚实基线是 58 条门禁 30 红、脚本落位 137 违规 / 76 仓对齐；剩余 137 条里 55% 卡在 3 个规范歧义上，必须先裁决再动手。**

> **后续取代（2026-09-11 晚，`MODULE_BIN_SPEC.md` v1.4）**：本审计把 bundle 执行器
> 迁到 `bin/bundle/` 的动作**已被 §2.1 的「No generic grouping subdirectory」废止**。
> 现在 4 个平台面模块（im / webserver / api-cloud-gateway / cloudrouter）的执行器均为
> 扁平命名 `bin/docker-bundle-deploy.sh` / `bin/docker-bundle-release.sh`
> （gateway 另有 `bin/docker-bundle-prepare-envs.sh`），由各模块打包器拷入 bundle 根为
> `deploy.sh` / `release.sh`。下文出现 `bin/bundle/` 的历史记录按此换算阅读。

---

## 1. 舰队口径修正：100 个受治理仓，不是 79

权威谓词在 `sdkwork-specs/tools/lib/workspace-check-runner.mjs`：

```js
// 目录名 sdkwork-* 且根目录有 AGENTS.md
export function listWorkspaceRepositoryRoots(workspaceRoot, { prefix = 'sdkwork-' } = {})
```

实测：

| 口径 | 数量 | 说明 |
| --- | --- | --- |
| 目录名 `sdkwork-*` | 100 | 全部 |
| 其中根有 `AGENTS.md`（**受治理**） | **100** | 权威口径 |
| 其中有 `sdkwork.app.config.json`（产品身份） | 79 | **旧门禁口径** |

**21 个受治理仓此前对所有 manifest-keyed 门禁不可见**：
`app-topology、catalog、comments、connect、core、database、fs、id、integration、log、miniapp-engine、rpc-framework、sdk-commons、simulator、skills-private、specs、superpowers、test、ui、utils、zip`。

### 1.1 但这 21 仓不需要扩 ops/deploy 门禁（重要）

对 21 仓逐个实测部署面：

| 信号 | 结果 |
| --- | --- |
| `deployments/deploy.yaml` | **0/21 存在** |
| `etc/topology/` | **0/21 存在** |
| `bin/` 脚本 | 仅 `sdkwork-specs` 5 个、其余 0–1 个 |

→ 这 21 仓是**库 / 工具 / 规范仓，没有部署生命周期面**。所以：

- `check-operations-conformance` / `check-module-bin` / `check-deploy-standard` 的 manifest 作用域**是正确的**，不应扩权（扩权会凭空造出 21×N 条假 FAIL）。
- `check-application-deploy-layout` 的 `isDeployableRepo`（需 `Cargo.toml` + assembly 兜底，且 `FRAMEWORK_REPOS` 白名单）**也是正确的**——21 仓里唯一有 assembly 的 `catalog`/`log` 已在框架仓白名单。
- **真正被漏审的只有「通用仓规」**——即脚本落位。这才是需要修门禁的地方。

---

## 2. 门禁层缺陷（4 处，全部已修）

门禁层缺陷优先于业务债修：它会让所有统计失真，并且会让人去"修"根本不存在的债。

### 2.1 `check-script-placement` 作用域过窄（已修）

`findModules()` 原用 `sdkwork.app.config.json` 判定"是不是模块"→ 21 仓被跳过。

修法（`sdkwork-specs/tools/check-script-placement.mjs`）：
1. `findModules()` 改为调用权威 `listWorkspaceRepositoryRoots()`；AGENTS-only 仓进审计，offFleet（有 manifest 无前缀：hub-installer、magic-studio、magic-studio-v2）仍只列不审。
2. 输出文案修正：`skipped (no sdkwork.app.config.json)` → `not governed (manifest but no sdkwork- prefix, listed only)`。

### 2.2 `check-script-placement` 主遍历不剪构建产物（已修）

已知假阳性：`sdkwork-birdcoder2/apps/desktop/.desktop-build/.../node-extract*/node-v24.17.0-win-x64/npm.ps1`（git-ignored 解压 Node runtime）。

修法：主遍历新增**跳过 git-ignored 树与文件**——每仓一次 `git ls-files --others --ignored --exclude-standard --directory`，同时收集目录（尾 `/`）与散落文件，分别用于剪子树和剪单文件。
- 与 scratch 的既有契约同构：**没进仓就不是源**。
- scratch 目录**豁免**该剪枝，否则 `SCRATCH-NOT-IGNORED` 语义会失效。
- 效果：假阳性 0。

### 2.3 CORS：一对**互相矛盾**的门禁（已修，且修掉我自己的回归）

| 门禁 | 要求 |
| --- | --- |
| `scripts/docker/validate-docker-deployment.mjs`（webserver 自有） | `SDKWORK_CORS_ALLOWED_ORIGINS` **必须**含 `http://<host>`（对所有环境） |
| `check:cors-standard`（舰队）→ `cors/registry.mjs#originEnvironmentViolation` | **production 不得出现非 https origin**（`production origin … must use https`） |

两者对 production 直接对撞。Phase A 我为满足前者加了 `http://server.sdkwork.com`，正好触发后者 —— **这是我引入的回归**。

权威判定在 `cors/registry.mjs` L258：`if (!isProductionEnvironment(environment)) origins.push('http://' + host)` → 只有非 production 才派生 `http://`。

修法：
1. `validate-docker-deployment.mjs` 按环境取 scheme（production→`https`，其余→`http`），与 registry 派生规则一致；
2. 移除 `production.env` / `production.env.example` 里的 `http://server.sdkwork.com`（保留管理口 `…_PUBLIC_HTTP_URL=http://server.sdkwork.com:18080`，那是另一变量）；
3. `align-cors-standard.mjs --fix` 归一白名单排序。

结果：`check-cors-standard` → **78 模块 / 777 carrier / 0 issue / 0 错误**。

### 2.4 `check-script-placement`：被剪枝的 ignored **文件**常量从未被读取（已修，第二轮）

§2.2 的修法收集了两组集合（ignored 目录 + ignored 文件），但 `walk()` 只消费了
`ignoredDirs`；`ignoredFiles` 作为参数**一路传递却从未被读取** —— 声明了、没实现，
与规范注释承诺不符。这是本工作区反复出现过的同一类漂移（注释/常量承诺了排除、实现没接）。

暴露点：`sdkwork-specs/.wm-fmt-apply.sh`（被 `.gitignore:27:/.wm-*` 忽略）仍被报为违规。

修法：`walk()` 的文件分支补上 `ignoredFiles.has(childRel)` 跳过；scratch 判定保持在前，
以免 `SCRATCH-NOT-IGNORED` 语义被静默吞掉：

```js
} else if (entry.isFile() && isScript(entry.name)) {
  if (inScratch) scratchOut.push(childRel);
  else if (ignoredFiles && ignoredFiles.has(childRel)) continue;  // 构建态，非源码
  else out.push(childRel);
}
```

效果（复跑 `--workspace`）：违规 **138 → 137**；**被审计脚本 1639 → 1610**
（-29 条 ignored 脚本不再进入审计）——说明该参数此前是**彻底死代码**，
影响面远大于那 1 条可见违规。

**规范依据（为什么不构成"放过违规"）**：§2.1 L105 的规则对象是
**committed** 的脚本（"is a violation, whether or not it is *tracked*" —— 区分的是
tracked vs untracked-but-not-ignored 两种"会被提交"的状态）；一个 git-ignored 路径
`git add .` 也带不进去，既不是"被提交的脚本"，也符合 L126–130
"generated tool output is not audited"。scratch 的含义（L138–141）本就定义为
**被 git 忽略**。剪枝只跳过 `--others --ignored` 集合（= 未跟踪**且**被忽略），
**不可能**掩盖 `scripts/`、`tools/` 里被跟踪或未忽略的脚本。

---

## 3. 诚实基线

### 3.1 脚本落位（`check-script-placement --workspace`）

| 指标 | 旧口径（79 仓，含假阳性） | **新口径（100 仓，修完门禁）** | 本轮迁移后 | 第二轮门禁修后 |
| --- | --- | --- | --- | --- |
| 审计根 | 79 | **100** | 100 | 100 |
| 审计脚本 | 427+ | **1637**（剪掉产物后为 1640） | 1640 | **1610** |
| 违规 | 112 | **156** | **139** | **137** |
| 对齐仓 | 58/79 | 75/100 | **76/100** | **76/100** |
| 假阳性 | 有（5 条） | **0** | 0 | 0 |

> **137 条违规全部是 git-tracked（实测）**：`tracked 134 / untracked 0 / ignored 1`
> （那 1 条即 §2.4 修掉的 ignored 文件）。含义是没有"删掉就完事"的草稿可白捡 ——
> 这 137 条都是需要一次**有人按标准决定落位**的真实迁移。

### 3.2 门禁全量回归（58 条）

- **根聚合 16 条**（`/e/sdkwork-space/.gate-names.txt`）：10 绿 / **6 红**
- **其余 `--workspace` 能力门禁 42 条**：18 绿 / **24 红**
- 合计 **28 绿 / 30 红**

红项归因（`✓`=本轮已修）：

| 门禁 | 主责模块 | 判定 |
| --- | --- | --- |
| `check:cors-standard` ✓ | sdkwork-webserver | 门禁矛盾，已修 |
| `check-topology-deployment-profiles` ✓ | sdkwork-webserver | 已修（见 §5.4） |
| `check:tailwind-integration` | appstore, birdcoder2 | 真债（feature 包禁止 `@import "tailwindcss"`） |
| `check:packages-layout` | agentstudio, birdcoder2 | 真债（仓根 `packages/`、缺 `repository-kind: application`、pnpm-workspace 遗留 glob） |
| `check:sdk-standard` | im, birdcoder2 | 真债（深生成传输导入、遗留生成名） |
| `check:database-initialization` | 多模块 | 真债（migration-debt） |
| `check:database-bootstrap-references` | documents, drama | 真债 |
| `check-api-operation-patterns` | rtc | 真债（Idempotency-Key 未 required、create 未 201） |
| `check-app-manifest-standard` | drama（真）+ cloudrouter（假） | **混合** |
| `check-app-sdk-consumer-imports` | im, birdcoder2 | 真债 |
| `check-base-url-resolution` | agents-pc, apikey | 真债（env base-url 链未走 `resolveBaseUrl`） |
| `check-component-port-bindings` | agents, cloudrouter, drama, im… | 真债（`requiredPorts` 未在对方 `publicExports`） |
| `check-credential-entry-bootstrap-standard` | mcp, notary, rtc, skills, terminal | 真债（vite.config.ts 未接 canonical IAM 入口） |
| `check-dependency-list-completeness` | agents, music, sdk-generator | 真债 |
| `check-deploy-standard` | 多模块 | 真债（placeholder upstream `http://127.0.0.1:8080`） |
| `check-frontend-composition` | cloudrouter, deployments, knowledgebase… | 真债 |
| `check-iam-web-adapter-standard` | promotion | 真债 |
| `check-pnpm-script-standard` | root-scripts（真）+ documentation-examples（假） | **混合** |
| `check-provider-session-identity` | agents | 真债 |
| `check-route-path-collisions` | drama | 真债 |
| `audit-api-assembly-workspace` / `audit-gateway-alignment-workspace` / `audit-gateway-route-composition-workspace` / `audit-apps-directory-index-workspace` / `audit-route-crate-naming-workspace` / `audit-repository-docs-workspace` / `audit-repository-docs-debt` | 多模块 | 真债（audit-* 报告器，exit 1 = 有问题） |
| `audit-agents-progressive-loading` | — | **模式错**（需 `--output`，非失败） |
| `audit-browser-workspace` | build-browser-client | 待判（该工具会**写**文件：`materialized standalone.test`） |

**门禁卫生待修（与本轮同类的"扫描范围"缺陷）**：
- `check-app-manifest-standard` 会扫 `sdkwork-cloudrouter/target-test-fixtures/`（**git-ignored、0 跟踪文件**）→ 与 §2.2 同类，应共享"跳过 git-ignored"助手。
- `check-application-cloud-gateway-boundary` 把 webserver 对 `sdkwork-api-cloud-gateway` 的**合法 attach 契约**（`SDKWORK_MODULE_API_GATEWAY_ATTACH_NETWORK`、README §17.3 说明）当违规 → 需要区分"边缘模块的 sanctioned 引用"与边界违规。
- `check-deploy-standard --root <单仓>` 缺 `--deployment-profile` 时刷大量 domain/web-surface 假错 → 单仓校验必须带 profile。
- 门禁消息里出现 Windows 反斜杠（`specs\topology.spec.json`）→ 输出卫生。

---

## 4. 脚本落位：137 违规的分类与波次

### 4.1 分类（按**修法**分类，不按仓分类）

分类的依据是 `MODULE_BIN_SPEC.md` §2.1 的**规范文本本身**，不是直觉。§2.1 里有两句
决定性的话：

- L105–108：-authored script 出现在 `deployments/**`、`docker/**`、`scripts/**`、
  `tools/**`、`tests/**`、仓根、app 目录 = 违规，**无论是否 tracked**。
- L131–134：**「验证不是 shell 脚本」**——探针/守卫/审计必须是语言原生测试
  （`node --test` / `cargo test` / `python -m pytest`），
  **一个只做断言的 shell 脚本即使放进 `bin/` 也是违规**。

第二句话是分类的分水岭：**它意味着"把验证脚本搬进 bin/"不是修法，是换一个地方违规。**

| 类 | 含义 | 正确修法 | 规模 | 证据 |
| --- | --- | --- | --- | --- |
| **A** 仓内草稿 | 未跟踪 scratch | 删除 | 0 剩余 | 实测 137 条**全部 tracked**（`tracked 134 / untracked 0 / ignored 1`） |
| **B** 规范显式项 | bundle executor、容器 entrypoint/init | 迁移到 `bin/bundle/`、`bin/container/` | 平台面 4 仓已完成 | §5.2–5.5 |
| **C** 有实际动作的宿主/运维脚本 | `install-ubuntu.sh`、`prepare-host.sh`、`backup.sh`、`migrate-database.sh`… | **迁移**到 `bin/host/`、`bin/packaging/`、`bin/` | ~59（`scripts/` 前缀） | 逐个读过头部 |
| **D** 只做断言的验证脚本 | `verify.ps1`、`verify-standards.ps1`、`verify_phase1.ps1`、`verify_sdkwork_structure.ps1`、`verify_openapi_operation_ids.ps1`、`check-security-config.sh`、`daily-security-check.sh`、`smoke/*` | **删除**（断言该由 CI / `node --test` / `cargo test` 承担）；**不得**搬进 `bin/` | ~40 | 见 §4.1.1 |
| **E** agent-skill 包布局 | `sdkwork-superpowers/skills/**/scripts/*.sh`、`sdkwork-skills-private/*/scripts/install.*` | 需裁决（§6.1） | 9 | 见 §4.1.1 |
| **F** 测试 harness | `sdkwork-superpowers/tests/**/*.sh` | 需裁决（§6.3）；按 L132 归宿是**改写成 `node --test`** | 26 | 见 §4.1.1 |
| **G** 生成物内 scratch | `sdkwork-assets/.../generated/**/.sdkwork/manual-backups/bin/*` | 删除或 gitignore（§6.5） | 3 | — |

### 4.1.1 逐类实证（不是推测，是读过的文件）

**D 类的三类实证**——它们都是"读完就能判死"的：

| 文件 | 实证 | 结论 |
| --- | --- | --- |
| `sdkwork-llm/tools/verify_sdkwork_structure.ps1`（123 行） | 全篇只有 `Assert-PathExists` / `Assert-PathAbsent` 两个 helper + 断言列表 | 纯断言 → 违 L131 |
| `sdkwork-llm/tools/verify_phase1.ps1`（448 行） | `Assert-Contains` + 必需文件清单 | 纯断言 → 违 L131 |
| `sdkwork-llm/tools/verify_openapi_operation_ids.ps1`（92 行） | 断言 OpenAPI 里 operationId 清单 | 纯断言 → 违 L131 |
| `sdkwork-specs/scripts/verify.ps1`（23 行） | 是个**编排器**（循环跑 node checker） | 不是断言，但**标准入口家族里没有 `verify` 这个入口**（§2 家族 = docker-image/docker-deploy/config/doctor/backup/apps-*）→ 仍是自造入口 |
| `sdkwork-canvas/scripts/verify-standards.ps1` | 文件头写着 **`# SDKWork Settings 验证脚本索引`** | **复制粘贴从未改名** —— 与 `sdkwork-settings/scripts/verify-standards.ps1` 逐字重复 |
| `sdkwork-im/tools/converge-repo.sh`（19 行） | 写死 `C:/Users/admin/AppData/Local/Temp/` 与 `E:/sdkwork-space/sdkwork-im/tools/fix-sqlx09-sites.py` | 违反 §2.1 L84–85（禁绝对机器路径）；一次性调试残留 |
| `sdkwork-im/scripts/verify-deployment.sh`（185 行） | 文件头是**乱码**（`鏂囦欢`/`鎻忚堪`，GBK 被当 UTF-8 读），且是 verify | 已损坏的死件 |

**"verify 脚本族"是模板批量复制**（`by basename` 统计）：

```
4× verify.ps1                     -> specs, web-framework, audio, rpc-framework
4× verify_openapi_operation_ids.ps1 -> knowledgebase, documents, llm, memory
4× verify_phase1.ps1               -> knowledgebase, documents, llm, memory
4× verify_sdkwork_structure.ps1    -> knowledgebase, documents, llm, memory
4× install.ps1 / 4× install.sh     -> tts, skills-private
3× common.sh                       -> tts, api-cloud-gateway, deployments
```

含义：**18 条违规来自 6 个模板**。它们不是 18 个独立问题，是"一次模板决策 × N 仓复制"。

### 4.2 违规分布（第二轮门禁修后，137 条 / 23 仓）

```
sdkwork-superpowers 31  |  sdkwork-tts 15  |  sdkwork-im 12  |  sdkwork-api-cloud-gateway 11
sdkwork-specs 7  |  sdkwork-birdcoder2 7  |  sdkwork-knowledgebase 6  |  sdkwork-skills-private 6
sdkwork-terminal 5  |  sdkwork-deployments 4  |  sdkwork-web-framework 4
sdkwork-{agents,audio,birdcoder,documents,llm,memory} 各 3  |  sdkwork-{browser,codebox} 2
sdkwork-{appstore,canvas,rpc-framework,settings} 各 1

按顶层目录：scripts/ 59 · tests/ 26 · tools/ 26 · apps/ 6 · (root) 3 · skills/ 3 · crates/ 2 · deployments/ 2 · docs/ 2
按扩展名：  .sh 81 · .ps1 54
```

`.ps1` 占 **54/137（39%）** 是必须点出的：规范 §2.1 只明确提过一次 `.ps1`
（L55 `apps-pkg-installer.ps1` = `apps-pkg-installer.sh` 的 OPTIONAL Windows 伴生）；
§2.1 的豁免表只提 `.ps1`/`.sh` 作为 `snapshots/`、`fixtures/` 里的测试数据。
**规范没有为"Windows 伴生脚本"给出通用落位规则** → 见 §6.2。

### 4.3 波次计划

1. **波次 1（已完成）**：门禁层 4 处缺陷 + 平台面 4 仓（webserver / api-cloud-gateway /
   cloudrouter / im）的 B 类迁移。
2. **波次 2（无阻塞，可直接做）**：C 类 ~59 条 —— 把有实际动作的 `scripts/**`、`tools/**`
   迁到 `bin/host/`、`bin/packaging/`、`bin/`，并同步清扫引用（`package.json`、compose、
   Dockerfile、docs、contract test）。**这是 137 条里唯一无规范歧义的部分。**
3. **波次 3（阻塞在裁决）**：D（~40）、E（9）、F（26）三类合计 **~75 条**，
   占剩余 **55%**，全部卡在 §6.1–6.3 三个规范问题上。
   在问题解决前动手 = 制造新的违规（D 类搬进 `bin/` 仍违规）。
4. **强烈建议**：C 类不要手改 21 个仓 —— 先落官方 `align-script-placement.mjs`
   （`--dry-run` / `--fix`，映射表用 §4.1，`--fix` 同时重写已知引用），
   否则这轮会重复"人工逐仓 + 漏改引用"的成本与风险。

---

## 5. 本轮已完成的修复

### 5.1 门禁层（`sdkwork-specs/tools`）

- `check-script-placement.mjs`：AGENTS-keyed 枚举 + git-ignored 剪枝（§2.1、§2.2）。
- `check-topology-deployment-profiles.mjs`：无改动（作为判据使用）。
- webserver `scripts/docker/validate-docker-deployment.mjs`：scheme 按环境（§2.3）。

### 5.2 `sdkwork-api-cloud-gateway`：22 → 11

- 删 6 个未跟踪仓根 `tmp-*.sh`（`tmp-find-gwm-conflicts / tmp-fix-toml-workspace / tmp-list-cargo-diffs / tmp-restore-cargo / tmp-restore2 / tmp-wsl-build-container`）。
- `deployments/docker/bundle/{deploy,prepare-envs,release}.sh` → `bin/bundle/`；`docker/postgres/{entrypoint.sh,init/001-create-schema.sh}` → `bin/container/postgres/`。
- 同步：`bin/bundle/*` repo-mode 深度（`../../..`→`../..`，`POSTGRES_ASSETS_DIR` 指向 `bin/container/postgres`）、`bin/lib/module.sh`、`docker-compose.yml`（2 个挂载）、`package-environment-bundles.mjs`（拷贝源 + 3 处注释）、`package-install-bundle.mjs`（拷贝源 + 4 行 dry-run 输出）、`AGENTS.md`、`docker/README.md`、`deployment-contract.test.mjs` 注释。
- **顺带修一个潜伏 bug**：`bin/lib/module.sh` 的 `sdkwork_module_bundle_dir()` 指向 `%s/scripts/docker/bundle`，而该路径自 bundle 迁到 `deployments/docker/bundle` 后**就已不存在** → 包安装/升级路径解析到空目录。已修为 `bin/bundle`。

### 5.3 `sdkwork-cloudrouter`：3 → **0（已对齐）**

- `deployments/docker/bundle/{deploy,release}.sh` → `bin/bundle/`；`scripts/delete-redundant-pc-packages.ps1` → `bin/host/`。
- repo-mode 指向**原地保留**的 bundle 输入 `deployments/docker/bundle/{compose,env}`（compose 里 `../env/config|secrets` 相对挂载因此不受影响）。
- 同步 `bin/lib/module.sh`、`scripts/deployment-contract.test.mjs`（3 处路径断言）。
- 验证：`check-script-placement` **0 违规**；`check-operations-conformance` **12 PASS / 0 FAIL**（且已在新位置发现 bundle）；`bash -n` 过。

### 5.4 `sdkwork-webserver`（自身）

- `specs/topology.spec.json` 两处真违规（`check-topology-deployment-profiles`）：
  1. `vocabulary.deploymentProfile.allowed` 含 `cloud` —— 但 `runtime.supportedDeploymentProfiles=["standalone"]` 且 `etc/topology/` 只有 `standalone.*.env` → **删除 cloud**。
  2. `surfaces."platform.api-gateway"` —— standalone-only 模块不得声明该面；且 `SDKWORK_WEBSERVER_PLATFORM_API_GATEWAY_HTTP_URL` / `VITE_...` 全仓**零引用**（死声明）→ **删除该面**。
- 结果：该门禁对 webserver **0 findings**。
- `specs/topology.spec.json` JSON 校验通过。

### 5.5 `sdkwork-im`：15 → 12

- `deployments/docker/bundle/{deploy,release}.sh` → `bin/bundle/`；`deployments/docker/postgres/init/001-create-schema.sh` → `bin/container/postgres-init/`。
- 同步：`bin/bundle/deploy.sh` repo-mode、`bin/bundle/release.sh` 深度、`bin/lib/module.sh`、`bin/README.md`、`deployments/docker/README.md`、`deployments/docker/docker-compose.yml` 挂载 → `../../bin/container/postgres-init`。
- 验证：`check-operations-conformance` **12 PASS / 0 FAIL**；`bash -n` 全过。

---

## 6. 待裁决（跨仓架构决策，不代批）

### 6.1 agent-skill 包布局（9 条）

`sdkwork-superpowers/skills/<skill>/scripts/*.sh`（3）、`sdkwork-skills-private/sdkwork-skills-{development,framework,ops-admin}/scripts/install.{sh,ps1}`（6）。

**冲突**：agent-skill 包要求脚本与 `SKILL.md` 同级，运行时才能执行；`MODULE_BIN_SPEC §2.1` 无豁免表。

**选项**：(a) §2.1 增一条豁免——"agent-skill 分发目录（`skills/**`、`**/skills/*/scripts/**`）是**分发布局**，不是模块脚本"；(b) 照搬进 `bin/` 并改 skill 包的引用/打包。

### 6.2 验证脚本（约 40 条）

规范 L131–134：**"验证不是 shell 脚本……只断言行为的 shell 脚本即使在 `bin/` 里也是违规"**。

但舰队有约 40 个 `scripts/verify.*`、`tools/verify_*.ps1`、`smoke/*`、`*-probe.ps1`；而 `bin/doctor.sh` **本身也在断言行为**——按字面读，`doctor.sh` 也违规。

**判定**：该条需收窄为「**仓级验证夹具**（repo-level verification harness）必须改写为语言原生测试」，而不是「一切断言脚本」。请确认收窄后的措辞。

### 6.3 测试 harness（26 条）

`sdkwork-superpowers/tests/**/*.sh`。现行排除清单只有「`node --test` / `cargo test` / `pytest`」，未覆盖 shell harness。

**选项**：(a) `tests/**` 排除（属测试资产，不是操作脚本）；(b) 视为违规并按 §6.2 改写。

### 6.4 bundle 的 compose/env 输入是否并入 `bin/bundle/`

webserver 已把 compose/env 提到 `deployments/docker/`；本轮 api-cloud-gateway/cloudrouter/im **保留**在 `deployments/docker/bundle/{compose,env}`（因 compose 内有 `../env/config|secrets` 相对挂载，搬动会连带改挂载）。

**问题**：`MODULE_BIN_SPEC §2.1` 只规定"脚本"落位，未规定 bundle 输入落位。是否要统一？（若要统一，建议 spec 增一句"bundle 输入（compose/env）与执行器分居是允许的；`sdkwork_module_bundle_dir()` 只指向执行器目录"。）

### 6.5 `sdkwork-assets` 生成物内的 scratch（3 条）

`sdks/sdkwork-assets-app-sdk/sdkwork-assets-app-sdk-typescript/generated/server-openapi/.sdkwork/manual-backups/bin/{publish.sh,publish.ps1,sdk-gen.sh}` —— 已提交进仓、非 git-ignored 的"手工备份"目录。

**选项**：(a) 删除（生成物可重建）；(b) 加 `.gitignore` 并 `git rm --cached`。

### 6.6 承接 Phase A 的旧决策

1. `deployment-contract.test.mjs` #28 part 3：读 git-ignored 的 `deployments/docker/env/*.env`（CI 必 ENOENT）且硬断言 `host.docker.internal` —— 接受显式主机地址，还是改成"仅当文件存在时校验"？
2. webserver `0.1.5 → 0.1.6` 镜像重建 + 五环境重发（工作树含本轮的 CORS/topology 改动），是否一起烘焙？

### 6.7 `.ps1`：Windows 伴生脚本的通用落位（54 条，占 39%）

`MODULE_BIN_SPEC §2.1` 只在 L55 提过一次 `.ps1`（`apps-pkg-installer.ps1` =
`apps-pkg-installer.sh` 的 OPTIONAL Windows 伴生），豁免表只把 `.ps1`/`.sh` 当作
`snapshots/`、`fixtures/` 里的测试数据。**没有"Windows 伴生脚本"的通用规则**，
但门禁 `SCRIPT_EXT` 含 `.ps1` → 54 条全部计入违规。

**问题**：`.ps1` 与它的 `.sh` 孪生是否必须同置于 `bin/`？还是"伴生脚本跟随主脚本所在目录"
（即主脚本进 `bin/`，`.ps1` 自动合规）？

**建议**：(a) 明确"伴生脚本（同名 `.ps1`/`.cmd` 孪生）跟随主脚本落位，不单独判定"
——一次性消掉 54 条里的大部分；(b) 否则需给出 `.ps1` 的独立落位规则并逐条迁移。

> 注：`sdkwork-im/bin/` 已有 `chat-cli{,.sh,.ps1,.cmd}`、`verify-server{,.sh,.ps1,.cmd}`
> 这类三件套（`.sh`/`.ps1`/`.cmd` 伴生），**实践中已经采用"伴生跟随"**——
> 规范只是没写下来。

---

## 7. 复现命令

```bash
# 舰队口径
node -e "const f=require('fs'),p=require('path');const ws='E:/sdkwork-space';\
const g=f.readdirSync(ws,{withFileTypes:true}).filter(e=>e.isDirectory()&&e.name.startsWith('sdkwork-'))\
.map(e=>e.name).filter(n=>f.existsSync(p.join(ws,n,'AGENTS.md')));\
console.log('governed',g.length)"

# 脚本落位（唯一门禁）
node sdkwork-specs/tools/check-script-placement.mjs --workspace E:/sdkwork-space --json

# 残差分类（§4.1）：按扩展名 / 顶层目录 / 模板重复度分桶
node sdkwork-specs/.tmp/placement-residual-analysis.mjs

# 残差按 git 跟踪状态切分（§3.1 注）：证明"没有可白捡的草稿"
node sdkwork-specs/.tmp/placement-tracking-split.mjs

# 单仓运营合规
node sdkwork-specs/tools/check-operations-conformance.mjs --root sdkwork-cloudrouter

# 门禁全量回归
cd /e/sdkwork-space && while IFS= read -r g; do pnpm run "$g"; done < .gate-names.txt
```

> 单仓 `check-deploy-standard` 必须带 `--deployment-profile`，否则会刷大量 domain/web-surface 假错。
>
> 两个 `.tmp/` 分析脚本是**一次性的**（不入库）；脚本内容的权威版本是本报告 §4.1.1 的表格。
> 注意 `check-script-placement` 有违规时退出码为 1，`execFileSync` 会抛异常——
> 要从 `err.stdout` 读报告（这两个脚本已处理）。
