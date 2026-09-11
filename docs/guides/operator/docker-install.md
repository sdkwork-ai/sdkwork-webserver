# sdkwork-webserver Docker 安装与部署指南

> 规范依据：`sdkwork-specs/DOCKER_SPEC.md`（镜像/安装/配置标准）、`sdkwork-specs/MODULE_BIN_SPEC.md`（`bin/` 入口标准）、`sdkwork-specs/DEPLOYMENT_SPEC.md` §6/§6.1（容器安装与外部依赖）。
> 本文档为唯一权威 Docker 安装文档，取代并已合并：`WSL_DOCKER_DEPLOY.md`、`WSL_EXTERNAL_DEPLOY.md`、`docker-remote-deploy.md/.en.md`。English version: [docker-install.en.md](./docker-install.en.md)

**核心理念：一个镜像打天下 + 一套 bin 入口。** 镜像与环境无关（不烘焙域名/数据库/凭据），五个生命周期环境（development / test / staging / demo / production）共用同一个镜像 tag；环境差异全部是部署时 env 输入。部署统一走 `bin/` 五入口（`MODULE_BIN_SPEC.md`），不再有第二套部署脚本。

---

## 0. 快速上手（TL;DR）

### 场景 A：拿到安装 bundle 的全新 Ubuntu/WSL Docker 主机（最快路径）

```bash
bin/docker-deploy.sh install --environment <environment> [--host ssh://[user@]host] [--replicas N]
```

bin/ 会完成 bundle 同步、sha256 校验、镜像加载与健康门禁；bundle 内执行器由 bin/ 调用，无需手动执行任何脚本。

### 场景 B：仓库内开发/运维机（bin/ 标准入口）

```bash
bin/docker-image.sh build --image-tag <version>      # 构建规范镜像（--image-tag 可省略，默认取 release.currentVersion）
bin/docker-deploy.sh install --environment demo      # 本机 WSL 部署
bin/docker-deploy.sh install --environment demo --host ssh://ops@10.0.0.8   # 远程 Ubuntu
```

> 五个环境的完整命令（install / upgrade / rollback / status / logs / down）见 §5。

### 场景 C：从仓库在远程机安装

```bash
bin/docker-image.sh save --image-tag <version> -o image.tar.gz   # 可选离线介质
bin/docker-deploy.sh install --environment <env> --host ssh://[user@]host[:port]
```

---

## 1. 拓扑与角色

- `sdkwork-webserver` 是**唯一公网边缘**（standalone-only）：自托管 PC/H5 SPA（SDK API 同源 `/`），并通过 `imports.d`（默认 `cloud` 集）反代平台 API 与兄弟模块域名。
- `sdkwork-api-cloud-gateway` 是 API 网关，**永不公网直出**，仅由 webserver 反代触达（容器 `:3900`，主机健康端口按环境 3910-3914）。
- **禁止安装宿主 nginx**（`NGINX_SPEC.md` §0）；公网 `:80/:443` 由 webserver 容器发布。

## 2. 前置条件（Ubuntu 22.04 / WSL Ubuntu）

| 项 | 要求 |
| --- | --- |
| Docker Engine + compose 插件 | 必须 |
| 宿主 PostgreSQL | `5432`（系统服务；`setup-host-external-deps.sh` 按环境建库） |
| 宿主 Redis | `6379`（系统服务，无密码，`bind 0.0.0.0`） |
| 空间目录 | `/opt/deploy`（模块克隆目标 `/opt/deploy/sdkwork-space`） |
| Drive 缓存 | `/opt/deploy/drive`（`SDKWORK_DRIVE_WEBSITE_CACHE_ROOT`） |
| 证书目录 | `/etc/sdkwork/certs/letsencrypt/<cert-name>/`（TLS 环境级） |

外部依赖是**默认模式**（`DEPLOYMENT_SPEC.md` §6.1）；嵌入式 postgres/redis 容器仅作显式 opt-in（`bin/docker-deploy.sh install --environment <env> --deps embedded`）。旧的 `15432` 端口已退役，任何文档/脚本不得再引用。

## 3. 五环境矩阵

| 环境 | 管理端口键（默认） | 导入 HTTP（默认） | HTTPS（默认） | 域名 | 数据库 |
| --- | --- | --- | --- | --- | --- |
| development | `SDKWORK_WEBSERVER_DEV_HOST_PORT`（13800） | 80 | 443 | `server-dev.*`、`*-dev.*` | `sdkwork_ai_dev` |
| test | `…_TEST_HOST_PORT`（18888） | 18898 | 28430 | `server-test.*`、`*-test.*` | `sdkwork_ai_test` |
| staging | `…_STAGING_HOST_PORT`（18081） | 18099 | 38431 | `server-staging.*`、`*-staging.*` | `sdkwork_ai_staging` |
| demo | `…_DEMO_HOST_PORT`（19080） | 19098 | 38432 | `server-demo.*`、`api-demo.*` | `sdkwork_ai_demo` |
| production | `…_PROD_HOST_PORT`（18080） | 18098 | 38430 | `server.*`、`api.*` | `sdkwork_ai_prod` |

- 容器内部统一监听 **80/443**（无端口重映射）；webserver 容器内部网关端口固定 **3800**。
- demo 是独立演示环境：专属数据库/Redis key 前缀/`api-demo` 域名族，不与其它环境共享持久化。
- 完整端口键契约：`DOCKER_SPEC.md` §3.2。

## 4. 标准安装流程（bin/ 入口，推荐）

```bash
# 0. 构建规范镜像（一次）
bin/docker-image.sh build --image-tag <version>
# → registry.sdkwork.com/apps/sdkwork-webserver-standalone:<version>

# 1. 供应宿主 PostgreSQL/Redis（全部环境，一次性）
sudo bash deployments/docker/scripts/setup-host-external-deps.sh

# 2. 部署指定环境（幂等；可重复执行收敛到目标状态）
bin/docker-deploy.sh install --environment development
bin/docker-deploy.sh install --environment test
bin/docker-deploy.sh install --environment demo
# production 变更必须显式确认：
bin/docker-deploy.sh install --environment production --yes

# 远程 Ubuntu 服务器：
bin/docker-deploy.sh install --environment demo --host ssh://ops@10.0.0.8
```

说明：

- `install` 将安装 bundle 同步到目标主机 `/opt/deploy/sdkwork-webserver/bundle` 并执行 bundle `deploy.sh --environment <env>`；幂等。
- 多实例：`--replicas N`；仅实例 1 发布边缘端口，其余实例走端口步进（`DOCKER_SPEC.md` §3.2）。
- 所有变更类命令支持 `--dry-run` 先查看计划；production 缺 `--yes` 会直接拒绝（错误码 68）。
- `pnpm deploy:reapply:<env>` 等价于上述 bin 入口（薄别名）。
- **§5 给出五个环境逐条展开、可直接复制的命令。**

## 5. 按环境命令速查（可直接复制）

> 以下每一块都可以整段复制执行。`<version>` 用实际版本号替换（当前
> `sdkwork.app.config.json` → `release.currentVersion`，**省略 `--image-tag`
> 时自动取该值**）；`ops@10.0.0.8` 换成你的远程 Ubuntu 主机；`--host` 省略时
> 默认为 `wsl`（本机 WSL）。
> 全部变更类命令都可先加 `--dry-run` 打印计划而不执行。

### 5.1 development（开发）

域名 `server-dev.sdkwork.com` / `api-dev.*` · 管理面 `13800` · 导入 HTTP `80` · HTTPS `443` · 库 `sdkwork_ai_dev`

```bash
bin/docker-image.sh build --image-tag <version>                    # 全环境共用，已构建可跳过
bin/docker-deploy.sh install  --environment development            # 本机 WSL（幂等）
bin/docker-deploy.sh install  --environment development --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment development --image-tag <new-version>
bin/docker-deploy.sh rollback --environment development            # 无发布台账 → 幂等重装当前 bundle
bin/docker-deploy.sh status   --environment development
bin/docker-deploy.sh logs     --environment development
bin/docker-deploy.sh down     --environment development
bin/docker-deploy.sh stop    --environment development           # 停止（保留容器与卷，不重打包）
bin/docker-deploy.sh start   --environment development           # 启动已停止的栈（先起嵌入式依赖）
bin/docker-deploy.sh restart --environment development           # 只重启应用实例（依赖与网关不中断）
bin/docker-deploy.sh down     --environment development --purge --yes
bin/docker-deploy.sh stop    --environment development           # 停止（保留容器与卷，不重打包）
bin/docker-deploy.sh start   --environment development           # 启动已停止的栈（先起嵌入式依赖）
bin/docker-deploy.sh restart --environment development           # 只重启应用实例（依赖与网关不中断）

curl --noproxy '*' http://127.0.0.1:13800/healthz                            # 管理面
curl --noproxy '*' -H 'Host: api-dev.sdkwork.com' http://127.0.0.1/healthz   # import 平面
```

### 5.2 test（测试）

域名 `server-test.*` / `api-test.*` · 管理面 `18888` · 导入 HTTP `18898` · HTTPS `28430` · 库 `sdkwork_ai_test`

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment test
bin/docker-deploy.sh install  --environment test --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment test --image-tag <new-version>
bin/docker-deploy.sh rollback --environment test                   # 无发布台账 → 幂等重装当前 bundle
bin/docker-deploy.sh status   --environment test
bin/docker-deploy.sh logs     --environment test
bin/docker-deploy.sh down     --environment test
bin/docker-deploy.sh stop    --environment test                  # 停止（保留容器与卷，不重打包）
bin/docker-deploy.sh start   --environment test                  # 启动已停止的栈（先起嵌入式依赖）
bin/docker-deploy.sh restart --environment test                  # 只重启应用实例（依赖与网关不中断）
bin/docker-deploy.sh down     --environment test --purge --yes
bin/docker-deploy.sh stop    --environment test                  # 停止（保留容器与卷，不重打包）
bin/docker-deploy.sh start   --environment test                  # 启动已停止的栈（先起嵌入式依赖）
bin/docker-deploy.sh restart --environment test                  # 只重启应用实例（依赖与网关不中断）

curl --noproxy '*' http://127.0.0.1:18888/healthz
curl --noproxy '*' -H 'Host: api-test.sdkwork.com' http://127.0.0.1:18898/healthz
```

### 5.3 staging（预发）

域名 `server-staging.*` / `api-staging.*` · 管理面 `18081` · 导入 HTTP `18099` · HTTPS `38431` · 库 `sdkwork_ai_staging`

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment staging
bin/docker-deploy.sh install  --environment staging --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment staging --image-tag <new-version>
bin/docker-deploy.sh rollback --environment staging                # 无发布台账 → 幂等重装当前 bundle
bin/docker-deploy.sh status   --environment staging
bin/docker-deploy.sh logs     --environment staging
bin/docker-deploy.sh down     --environment staging
bin/docker-deploy.sh stop    --environment staging               # 停止（保留容器与卷，不重打包）
bin/docker-deploy.sh start   --environment staging               # 启动已停止的栈（先起嵌入式依赖）
bin/docker-deploy.sh restart --environment staging               # 只重启应用实例（依赖与网关不中断）
bin/docker-deploy.sh down     --environment staging --purge --yes
bin/docker-deploy.sh stop    --environment staging               # 停止（保留容器与卷，不重打包）
bin/docker-deploy.sh start   --environment staging               # 启动已停止的栈（先起嵌入式依赖）
bin/docker-deploy.sh restart --environment staging               # 只重启应用实例（依赖与网关不中断）

curl --noproxy '*' http://127.0.0.1:18081/healthz
curl --noproxy '*' -H 'Host: api-staging.sdkwork.com' http://127.0.0.1:18099/healthz
```

### 5.4 demo（演示）

域名 `server-demo.*` / `api-demo.*` · 管理面 `19080` · 导入 HTTP `19098` · HTTPS `38432` · 库 `sdkwork_ai_demo`

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment demo
bin/docker-deploy.sh install  --environment demo --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment demo --image-tag <new-version>
bin/docker-deploy.sh rollback --environment demo                   # 无发布台账 → 幂等重装当前 bundle
bin/docker-deploy.sh status   --environment demo
bin/docker-deploy.sh logs     --environment demo
bin/docker-deploy.sh down     --environment demo
bin/docker-deploy.sh stop    --environment demo                  # 停止（保留容器与卷，不重打包）
bin/docker-deploy.sh start   --environment demo                  # 启动已停止的栈（先起嵌入式依赖）
bin/docker-deploy.sh restart --environment demo                  # 只重启应用实例（依赖与网关不中断）
bin/docker-deploy.sh down     --environment demo --purge --yes
bin/docker-deploy.sh stop    --environment demo                  # 停止（保留容器与卷，不重打包）
bin/docker-deploy.sh start   --environment demo                  # 启动已停止的栈（先起嵌入式依赖）
bin/docker-deploy.sh restart --environment demo                  # 只重启应用实例（依赖与网关不中断）

curl --noproxy '*' http://127.0.0.1:19080/healthz
curl --noproxy '*' -H 'Host: api-demo.sdkwork.com' http://127.0.0.1:19098/healthz
```

### 5.5 production（生产）

域名 `server.*` / `api.*` · 管理面 `18080` · 导入 HTTP `18098` · HTTPS `38430` · 库 `sdkwork_ai_prod`

> **所有变更类命令必须带 `--yes`**，否则直接被拒绝（错误码 68）；`down --purge`
> 在**任何**环境都必须带 `--yes`。

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment production --yes
bin/docker-deploy.sh install  --environment production --yes --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment production --yes --image-tag <new-version>
bin/docker-deploy.sh rollback --environment production --yes       # 无发布台账 → 幂等重装当前 bundle
bin/docker-deploy.sh status   --environment production             # 只读，不需要 --yes
bin/docker-deploy.sh logs     --environment production             # 只读，不需要 --yes
bin/docker-deploy.sh down     --environment production
bin/docker-deploy.sh stop    --environment production            # 停止（保留容器与卷，不重打包）
bin/docker-deploy.sh start   --environment production            # 启动已停止的栈（先起嵌入式依赖）
bin/docker-deploy.sh restart --environment production            # 只重启应用实例（依赖与网关不中断）
bin/docker-deploy.sh down     --environment production --purge --yes
bin/docker-deploy.sh stop    --environment production            # 停止（保留容器与卷，不重打包）
bin/docker-deploy.sh start   --environment production            # 启动已停止的栈（先起嵌入式依赖）
bin/docker-deploy.sh restart --environment production            # 只重启应用实例（依赖与网关不中断）

curl --noproxy '*' http://127.0.0.1:18080/healthz
curl --noproxy '*' -H 'Host: api.sdkwork.com' http://127.0.0.1:18098/healthz
```

### 5.6 多实例与依赖模式

```bash
# 多实例：仅实例 1 发布边缘端口，其余按步进（DOCKER_SPEC.md §3.2）
bin/docker-deploy.sh install --environment demo --replicas 3

# 依赖模式：外置宿主 PostgreSQL/Redis 为默认；embedded 为显式 opt-in
bin/docker-deploy.sh install --environment demo --deps external     # 默认
bin/docker-deploy.sh install --environment demo --deps embedded

# 回滚到指定版本（webserver bundle 不带发布台账，用 upgrade 指定旧 tag）
bin/docker-deploy.sh upgrade --environment demo --image-tag 0.1.0
```

> **回滚语义**：`sdkwork-webserver` 的安装 bundle 只含 `deploy.sh`，不含
> 发布台账，因此 `rollback` 会告警并退化为「幂等重装当前 bundle」。
> 要回退到某个具体版本，用 `upgrade --image-tag <旧版本>`。
> （`sdkwork-api-cloud-gateway` 的 bundle 带发布台账，`rollback --to <version>`
> 在那里才有效。）

## 6. 离线安装（bundle 直装）

无外网/无仓库检查环境使用发布产物：

```bash
bin/docker-image.sh save -o dist/image.tar.gz               # 产出镜像 tar.gz + sha256（无外网时离线携带）
bin/docker-image.sh load  -i dist/image.tar.gz              # 目标机侧导入镜像
bin/docker-deploy.sh install --environment <environment> [--host ssh://[user@]host] [--replicas N]
```

bundle 内容契约（内置执行器、`image.env`、五环境 env 矩阵、compose、sha256）见 `DOCKER_SPEC.md` §4。

## 7. import 模式（启动时选择）

在 `deployments/docker/env/<环境>.env` 中声明（默认 `cloud`）：

```sh
SDKWORK_WEBSERVER_IMPORT_PROFILE=cloud      # 导入各模块 nginx.cloud.<env>.conf（api-* 边缘 → gateway 上游）
# SDKWORK_WEBSERVER_IMPORT_PROFILE=standalone   # 同源方式：导入 nginx.standalone.<env>.conf
```

- 启动时 entrypoint 物化两套导入集并把选中集激活为 `imports.d/import.conf`（`SDKWORK_WEBSERVER_SPEC.md` §17.3.1）。
- 运行时切换（无需重建容器）：`pnpm import:switch:cloud|standalone` 后重启 `serve-imports`；`pnpm import:status` 查看激活集。
- 模块 PC/H5 静态资源跟随激活集（cloud 激活服务 `dist/cloud/<alias>`，standalone 激活服务 `dist/standalone/<alias>`）。

## 8. 验证

```bash
curl --noproxy '*' http://127.0.0.1:13800/healthz                                   # 开发管理面
curl --noproxy '*' -H 'Host: api-dev.sdkwork.com' http://127.0.0.1/healthz          # 平台 API 平面
curl --noproxy '*' -H 'Host: server-demo.sdkwork.com' http://127.0.0.1:19098/healthz
pnpm check:container-deployment    # 部署契约校验矩阵（validate-docker-deployment.mjs）
```

**每个环境的管理面/import 平面端口与完整验证命令见 §5.1–§5.5。**

验收清单：`sdkwork-specs/DOCKER_SPEC.md` §8。

## 9. 升级与回滚

```bash
bin/docker-image.sh update --image-tag <new-version>       # 拉取/更新镜像并清理悬空层
bin/docker-deploy.sh upgrade --environment <env> [--image-tag <new-version>]
bin/docker-deploy.sh rollback --environment <env>          # 重放上一个 bundle release（webserver 退化为幂等重装，见 §5.6）
bin/docker-deploy.sh status   --environment <env>
bin/docker-deploy.sh logs     --environment <env>
bin/docker-deploy.sh down     --environment <env> [--purge]     # --purge 任一环境都需 --yes
```

数据卷与宿主数据库不受部署脚本影响；回滚 = 重新部署上一个 bundle。
**按环境展开的可复制命令见 §5。**

## 9.1 运营生命周期（日志 / 配置 / 诊断 / 备份）

```bash
# 日志：默认有界读取，--follow 才持续跟踪
bin/docker-deploy.sh logs --environment <env> [--instance N] [--service webserver]
bin/docker-deploy.sh logs --environment <env> --tail 1000 --since 15m
bin/docker-deploy.sh logs --environment <env> --tail 5000 --export ./incident

# 配置：读取默认脱敏（***REDACTED***）；set/edit 先备份再做校验，生产需 --yes
bin/config.sh list     --environment <env>
bin/config.sh show     --environment <env>
bin/config.sh get      --environment <env> --key <KEY> [--reveal]
bin/config.sh set      --environment <env> --key <KEY> --value '<VALUE>'
bin/config.sh diff     --environment <env>      # 缺失键 / 未声明键 / 占位符
bin/config.sh validate --environment <env>
EDITOR=vi bin/config.sh edit --environment <env>

# 诊断：只读，9 项检查，存在 FAIL 时退出码 70
bin/doctor.sh --environment <env> [--json] [--export ./incident]

# 备份与恢复：集合在目标机 /opt/deploy/<module>/backups/
bin/backup.sh create  --environment <env> [--no-db] [--no-volumes]
bin/backup.sh list    --environment <env>
bin/backup.sh verify  --environment <env> [--set <name>]
bin/backup.sh restore --environment <env> --set <name> --yes
```

逐环境完整命令与处置路径见 `docs/runbooks/`（deploy / log-reference /
troubleshooting / backup-restore，中英双语）。

## 10. 规范索引

| 主题 | 权威规范 |
| --- | --- |
| 镜像命名/tag、bundle 布局、五环境矩阵 | `sdkwork-specs/DOCKER_SPEC.md` |
| `bin/` 五入口契约与共享库 | `sdkwork-specs/MODULE_BIN_SPEC.md` |
| import 机制与启动模式选择 | `sdkwork-specs/SDKWORK_WEBSERVER_SPEC.md` §17.3/§17.3.1 |
| 外部依赖标准（PostgreSQL/Redis） | `sdkwork-specs/DEPLOYMENT_SPEC.md` §6.1 |
| 公网边缘权威（禁宿主 nginx） | `sdkwork-specs/NGINX_SPEC.md` §0 |
