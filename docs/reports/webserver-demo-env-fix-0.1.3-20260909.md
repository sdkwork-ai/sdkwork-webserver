# sdkwork-webserver demo 环境修复 + 固化发布 0.1.3 + demo 全工作区枚举对齐（wave1）

日期：2026-09-09　目标：修复 demo（http://server-demo.sdkwork.com:19098/）前端 `environment is invalid`，让 demo 作为一级环境与 dev/test/staging/prod 全面对齐

## 一、根因
demo 是一级生命周期环境，但 webserver **浏览器端 pc-core 生命周期枚举缺 demo**：
- `apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-core/src/index.ts`
  - L1 `WebserverLifecycleEnvironment` 原 4 值（缺 demo）
  - L38 `readEnum(value.environment, ["development","test","staging","production"] as const, ...)`
- 容器启动时 entrypoint 按 `environment=demo` 物化 runtime-env.json（`"environment":"demo"`），浏览器解析时 demo 不在允许列表 → 抛 `environment is invalid`。

镜像 0.1.2 内旧 pc bundle allow-list 实测 = `environment,[development,test,staging,production]`（缺 demo）。

## 二、修复文件（webserver，源码级）
| 文件 | 改动 |
|---|---|
| `apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-core/src/index.ts` | L1 枚举 + L38 readEnum 数组加 `demo`（根因修复） |
| `apps/sdkwork-webserver-pc/scripts/materialize-runtime-env.mjs` | `SUPPORTED_ENVIRONMENTS` + demo（L10） |
| `apps/sdkwork-webserver-h5/scripts/materialize-runtime-env.mjs` | 同上 |
| `apps/sdkwork-webserver-pc/scripts/browser-topology.d.mts` / h5 同 | 类型 union + demo |
| `deployments/docker/scripts/entrypoint-standalone.sh` | demo 分支：`environment_dist_alias`、messaging_pc_url→`https://messaging-demo.sdkwork.com/notifications`、api_hosts 每品牌加 `api-demo.<brand>`、import env 列表 |
| `scripts/docker/build-standalone-image.mjs` | `SUPPORTED_ENVIRONMENTS` + demo（L41，构建门禁） |

> 注：h5 前端**无** readEnum 环境校验，故 h5 bundle 无需变动（其 materialize 已含 demo）。

## 三、镜像机制（关键认知）
5 环境**共享镜像内同一份** `/app/share/sdkwork/webserver-pc|h5` 静态 bundle。容器启动时 entrypoint `ensure_spa_static_root` 将其拷到 `/app/share/sdkwork/webserver/web/pc|h5`，并按**当前 env 重写 runtime-env.json**（非每环境独立 dist 子目录）。因此 pc bundle 升级为 5-env 超集后，**5 环境共享生效且向后兼容**（dev/test/staging/prod 的 env 值仍在允许列表）。

## 四、固化发布（用户选「重烘焙镜像」）
走轻路径**不重跑 cargo**：
1. 把修复后 `apps/sdkwork-webserver-pc|h5/dist/standalone/demo` 覆盖进 `.sdkwork/runtime/docker-standalone-context/share/sdkwork/webserver-pc|h5`
2. 用工作树 entrypoint（17 处 demo）覆盖 context entrypoint
3. `docker build -f deployments/docker/Dockerfile.standalone -t registry.sdkwork.com/apps/sdkwork-webserver-standalone:0.1.3 <context>` → **成功**（镜像 5d5093ce68ba，5.93GB）
4. `bin/docker-deploy.sh install --environment demo --host wsl --deps external --image-tag 0.1.3 --yes --skip-backup` → demo 容器 Recreated/Started **healthy**

> ⚠️ `build-standalone-image.mjs` 的 `stageContext` 每次会 `rmSync` 重建 context（从 release archive 解包覆盖手动改动），改动 context 后勿重跑 staging。

## 五、验证证据
- 镜像 0.1.3 内 pc bundle allow-list = `environment,[development,test,staging,demo,production]`（grep PASS）
- demo 容器 serve `index-KWbP0Pq8.js`（修复后），allow-list 含 demo；runtime-env = `environment:"demo"` / `profileId:"standalone.demo"` / `messagingPcUrl:"https://messaging-demo.sdkwork.com/notifications"`（原错误 127.0.0.1 已修）
- demo 修复**已烘焙进镜像** → 容器重启/重建后不丢失（持久，非 docker cp 临时态）
- HTTP：demo index/bundle/runtime-env 全 200；dev/test/staging/production 全 200（无回归）

## 六、demo 全工作区枚举缺口 — wave1 已修（源码级，未部署在线）
| 仓库 | 文件 | 改动 |
|---|---|---|
| sdkwork-messaging | pc-core `runtime-config.ts`；`scripts/materialize-runtime-env.mjs`；common `messaging-runtime/src/index.ts` | type+readEnum+SUPPORTED_ENVIRONMENTS + demo |
| sdkwork-deployments | pc-core `index.ts`；Rust `app_domains.rs`（[&str;4]→[&str;5]+文案）；`scripts/deploy.ps1`（ValidateSet+foreach+注释） | + demo |
| sdkwork-agents | **生成源头** `materialize-client-app-surfaces.mjs` + 已生成 `h5/bootstrap/environment.ts` | type+Set+throw + demo |
| sdkwork-company | `company-runtime/browser-environment.ts` | type+白名单 + demo |
| sdkwork-community | `community-runtime/browser-environment.ts` | type+白名单 + demo |
| sdkwork-memory | pc-core `config/runtime-config.ts` | type+readEnum + demo |

改动均为**单向加 "demo" literal**（超集扩展），community 全仓 tsc 无一条错误指向改动文件（既有跨仓依赖噪音除外），判定无类型回归。

## 七、需后端契约先行（未贸然 UI-only 改）
webserver 的 `pc-console-core/index.tsx` `deploymentEnvironment`（L709-723+L267/311/357 选项）与 `pc-admin-applications/data-source.ts`（L622-636+L72/142/240）若直接加 demo，会因返回喂给 `deploymentRequest` 对象字面量（index.tsx L681、data-source.ts L594）而受**webserver 自生成 SDK `CreateDeploymentRequest`（environment 仍 4-env，源 `sdks/.../openapi sdkgen.yaml` L5956）** TS2322 卡死。
**正确路径**：先扩 webserver 后端 sdkgen 环境 enum → 5-env，重新生成 SDK，Rust 端点接收 demo，再放开 UI。作为架构性待办记录。

## 八、wave2/3 待办（后续批次）
- 工作区级打包门禁 `bin/package-docker-bundles.sh`（GATEWAY_ENVS/WEBSERVER_ENVS 缺 demo）
- 各 app vite.config（agents/birdcoder/kernel/gameengine/terminal 等）环境正则缺 demo
- 纯类型/静默回退（sdkwork-core/sdkwork-image/sdkwork-manager/sdkwork-github/sdkwork-news flutter 等）
- 测试遍历脚本补 demo

## 九、对外访问 URL（供浏览器测试）
| 环境 | HTTP | 镜像 |
|---|---|---|
| development | http://server-dev.sdkwork.com:13800/ | 0.1.2 |
| test | http://server-test.sdkwork.com:18898/ | 0.1.2 |
| staging | http://server-staging.sdkwork.com:18099/ | 0.1.2 |
| demo（已修） | http://server-demo.sdkwork.com:19098/ | **0.1.3** |
| production | http://server.sdkwork.com:18098/ | 0.1.2 |
