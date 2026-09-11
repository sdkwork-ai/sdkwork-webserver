# 脚本放置标准收敛：`bin/` 单源迁移记录

- 日期：2026-09-11
- 范围：`sdkwork-webserver` 全仓脚本放置 + `sdkwork-specs` 规范/门禁同步
- 规范落点：`sdkwork-specs/MODULE_BIN_SPEC.md` §2.1（**v1.2 → v1.3**）

> **已被取代（2026-09-11 晚，`MODULE_BIN_SPEC.md` v1.4）**：本记录定义的 v1.3
> canonical sub-layout 中「`bin/bundle/` = bundle 执行器」一行**已废止**——v1.4 §2.1
> 明确禁止 `bin/bundle/`、`bin/docker/`、`bin/misc/` 等泛化分组目录，改由 §2.2
> 「脚本命名规范」按**文件名**声明族别：bundle 执行器现为扁平的
> `bin/docker-bundle-deploy.sh` / `bin/docker-bundle-release.sh`
> （gateway 另有 `bin/docker-bundle-prepare-envs.sh`），由模块自己的打包器按
> `DOCKER_SPEC.md` §4.1 固定的产物名拷入 bundle 根为 `deploy.sh` / `release.sh`。
> 下文各节的 `bin/bundle/` 路径均按此换算；其余结论（单源规则、`bin/container/`、
> `bin/host/`、`bin/packaging/`）仍然有效。

## 1. 标准（唯一规则，无例外表）

> **一切人写的脚本，源都在 `bin/` 下。**

产物里必须存在的脚本是 **build 复制品**，不是第二源码：镜像的 `ENTRYPOINT` 与依赖 init 由镜像构建从 `bin/` stage 进 build context；
bundle 根的执行器由打包器从 `bin/bundle/` 拷贝；deb/rpm 维护脚本由打包命令从 `bin/packaging/` 渲染。

### 1.1 Canonical sub-layout（v1.3 §2.1）

| 路径 | 放什么 |
| --- | --- |
| `bin/*.sh` | 九入口 —— 操作面 |
| `bin/lib/` | 被 `source` 的库（`bootstrap.sh`、`module.sh`） |
| `bin/bundle/` | `deploy.sh` / `release.sh` —— `bin/docker-deploy.sh` 的**私有实现**，禁止当操作入口出现在 operator 文档 |
| `bin/container/` | 镜像内脚本：容器 `ENTRYPOINT`、依赖 init（`postgres-init/`） |
| `bin/host/` | 操作员在 docker / WSL 宿主上跑的脚本 |
| `bin/packaging/` | 打包输入：deb/rpm 模板与 spec、发布验证器 |

路径落在任一 `bin/` 下即合规；该表固定**放哪里**，不改变**是否通过**。

## 2. 迁移映射（23 个脚本，全部 `git mv` 保留历史）

| 旧位置 | 新位置 |
| --- | --- |
| `deployments/docker/bundle/{deploy,release}.sh` | `bin/bundle/` |
| `deployments/docker/scripts/entrypoint-standalone.sh` | `bin/container/` |
| `deployments/docker/postgres/init/*.sh` | `bin/container/postgres-init/` |
| `deployments/docker/scripts/*`（14 个宿主助手，含 2 个 `.ps1`） | `bin/host/` |
| `scripts/deb/*`（5）+ `scripts/rpm/sdkwork-webserver.spec.template` | `bin/packaging/{deb,rpm}/` |
| `scripts/docker/verify-platform-api-plane.sh` | `bin/packaging/` |

空目录 `deployments/docker/{bundle,scripts,postgres/init}`、`scripts/{deb,rpm}` 已删除。
`scripts/` 现在只余 `.mjs` 构建工具与 `README.md`（非 shell 脚本，不受 §2.1 约束）。

## 3. 同步的消费方（漏一个即运行期失败）

- `bin/lib/module.sh` — `sdkwork_module_bundle_dir()` → `<repo>/bin/bundle`
- `scripts/docker/build-standalone-image.mjs` — 入口源 → `bin/container/entrypoint-standalone.sh`（新增 `BIN_ROOT`）
- `scripts/docker/package-install-bundle.mjs` — `BIN_ROOT` 常量；deploy/release 拷贝源、postgres-init 目录拷贝源、bundle README 文案
- `scripts/webserver-deb.mjs` / `scripts/webserver-rpm.mjs` — deb/rpm 模板根
- `deployments/docker/docker-compose*.yml`（8 个）— 入口挂载 `./scripts/...` → `../../bin/container/entrypoint-standalone.sh`；
  内置 postgres init 挂载 → `../../bin/container/postgres-init`（bundle 版 `${POSTGRES_INIT_HOST_DIR:-}` 兜底同步）
- `scripts/docker/deployment-contract.test.mjs` — 15 处路径断言
- `bin/bundle/deploy.sh` / `release.sh` — **repo 模式分支的 `../..` 深度重算**（`deployments/docker/bundle/` → `bin/bundle/`）
- `bin/host/*` — 互相引用的文案前缀；`setup-windows-port-forwarding-admin.ps1` 去掉硬编码 `/mnt/e/...` 绝对路径，改由 `$PSScriptRoot` 推导

## 4. 规范与文档

- `MODULE_BIN_SPEC.md` v1.3：§2.1 增 Canonical sub-layout（见 §1.1）。
- `AGENTS.md`：`bin/` 映射表改为 `bin/bundle/deploy.sh`（并标注"私有实现、由打包器拷入 bundle"）；nginx 卸载路径改 `bin/host/`。
- `deployments/docker/README.md`：删除过期的 `postgres/init/`、`scripts/` 布局行，新增"本树脚本均为 build output"说明；快速开始与验证命令改新路径。
- `bin/README.md`：rollback 段不再教操作员直接跑 `release.sh`，改 `bin/docker-deploy.sh rollback`。
- `deployments/docker/nginx/README.md`、`docs/guides/operator/{CONFIG_PATHS,docker-install,docker-install.en}.md`、2 个 compose 头注释同步。
- **历史日期报告**（`docs/reports/2026-09-10-*`、`2026-09-11-*`、`webserver-demo-env-fix-0.1.3-*`）保留正文，仅加"路径迁移提示"指向 `bin/README.md`。
- 临时取证脚本按 §2.1 清理：删 `.sdkwork/audit/*.sh`（25，未跟踪）与 `.sdkwork/release-015/step*.sh`（4，**曾已提交**，
  `git rm --cached`）；`.sdkwork/audit-evidence/` 保留非脚本证据。
- `.gitignore`：`/.sdkwork/*.sh|*.py|*.conf|*.mjs` 只匹配直接子级，嵌套 scratch 会以 `??` 漏网 →
  改 default-deny `/.sdkwork/*` + 反选 `.gitignore`/`README.md`/`hosts.wsl.sdkwork`/`skills/`/`plugins/`。

## 5. 门禁工具修正（`sdkwork-specs/tools`）

1. `check-script-placement.mjs`：violation 增 `root` 归属，文本按模块分组 —— 原 `--workspace` 合并 79 个 root 后**无法定位模块**。
2. `check-script-placement.mjs`：合并 `walk()`/`walkScratch()` —— 原实现只审**根级** scratch，嵌套 `apps/<app>/.sdkwork/*.sh` 被静默跳过。
3. `check-operations-conformance.mjs`：bundle 发现顺序 → `['bin/bundle', 'deployments/docker/bundle', 'scripts/docker/bundle']`
   （canonical 优先，legacy 仅用于定位内容；放置问题归 placement 门禁，两个门禁不抢管辖权）；顺带修 `envDirs` 的绝对路径二次拼接 bug。

## 6. 顺带修复的真缺陷

- `package-install-bundle.mjs --dry-run` 原本**先执行完整镜像构建**再返回 → 提前到构建之前，现可离线打印 bundle 计划。
- 5 个 `*.env.example` 把本机 LAN IP `192.168.31.116` 烘焙进**出厂模板**（操作员在其它主机将继承不存在的主机）→
  20 行归一为 `host.docker.internal`。本机 `*.env`（被 gitignore）保持 LAN IP 不动。
- `production.env(.example)` 的 CORS 缺 `http://server.sdkwork.com`（`validate-docker-deployment.mjs` 要求 `http://<role>.<base>`，
  批量重写成 https-first 时只漏了 production）→ 补齐。

## 7. 验证证据

| 门禁 | 结果 |
| --- | --- |
| `check-script-placement.mjs --root sdkwork-webserver` | **427 脚本 / 0 违规**（迁移前 52） |
| `check-module-bin.mjs --root sdkwork-webserver` | passed |
| `check-operations-conformance.mjs --root sdkwork-webserver` | **12 PASS / 0 FAIL / 0 WARN / 0 N/A**（已识别 `bin/bundle/deploy.sh`） |
| `check-shell-portability.mjs --root sdkwork-webserver` | 65 文件 / 0 findings（`bash -n` on） |
| `check:deploy` / `check:deploy-layout` / `check:repository-docs` | ok |
| `check:container-deployment` | `{"ok":true,"count":10}` |
| `scripts/docker/deployment-contract.test.mjs` | 28 / 29 |
| 舰队回归：ops-conformance / module-bin | 79/79 clean（N/A 285 不变）/ 79/79 —— 门禁改动零回归 |
| 镜像 staging 实测 | build context 的 `entrypoint-standalone.sh` 与 `bin/container/entrypoint-standalone.sh` 字节一致（2225 行） |

## 8. 未完成

1. **`deployment-contract.test.mjs` 第 28 项 part 3 仍红（本次改动之前即红）**：该断言 `readFileSync` **未跟踪**的
   `deployments/docker/env/*.env`（`*.env` 被 gitignore，干净 checkout/CI 必然 ENOENT），且硬断言
   `WEBSERVER_POSTGRES_HOST/REDIS_HOST === host.docker.internal`；本机 `.env` 有意使用 LAN IP（`host.docker.internal`
   在 Clash TUN + mirrored networking 下不可达）。属"本地实测偏差 vs 契约"的另一工作流，需裁决：
   接受显式主机地址，或改为"仅当文件存在时校验它是外部主机端点"。
2. **舰队迁移未做**：`--workspace` 基线 **112 违规 / 21 模块 / 58 aligned**
   （`api-cloud-gateway` 22、`im` 15、`tts` 15、`birdcoder2` 10、`knowledgebase` 6、`terminal` 5 …）。
   需先给门禁主遍历加"跳过 git-ignored 树"，否则会把构建产物（`.desktop-build/.../node-extract*`、
   `.../generated/**/.sdkwork/manual-backups/bin`）误当源码去"修"。
3. 0.1.5 → 0.1.6 重建镜像与五环境重发未执行；工作树仍含 CORS 工作流的未提交改动，需确认是否一并烘焙。
