# Docker 安装包部署指南（docker-install）

> 版本：2026-08-30 · 适用：`sdkwork-webserver` standalone 统一安装镜像
> 规范依据：`sdkwork-specs/DEPLOYMENT_SPEC.md` §6、`sdkwork-specs/PNPM_SCRIPT_SPEC.md` §4.4

## 1. 设计概要

`pnpm build:container:install` 产出**一个统一安装镜像包**（self-contained install bundle）：

- **一个镜像**：镜像本身与环境和实例数无关（environment-neutral），构建时**不**烘焙任何环境、域名、数据库或凭据信息；生命周期环境（development / test / production）与实例数全部是**部署时输入**，由容器 entrypoint 启动时解析。
- **任意环境**：development、test、production 使用同一个镜像 tag，部署时通过 env 文件选择环境。
- **每个环境支持多实例**：N 个实例共享同一网络与同一份 secrets/data 卷；每个实例拥有独立 compose 项目名、独立节点身份（`SDKWORK_WEBSERVER_NODE_UUID`）和独立管理端口；仅实例 1 发布 80/443 边缘端口并先执行数据库迁移。

## 2. 打包安装包

```bash
# 仓库根目录（sdkwork-webserver）
pnpm build:container:install                       # 构建镜像 + 打包
pnpm build:container:install -- --skip-image-build # 复用已构建镜像
pnpm build:container:install -- --tag 0.1.0 --out dist/docker-install --dry-run
```

从 sdkwork-space 根目录（WSL / CI）也可以走 `bin` 脚本：

```bash
bash bin/build-webserver-docker.sh                 # 默认读取 manifest 版本
bash bin/build-webserver-docker.sh --out /opt/deploy/packages
```

产物布局：

```text
dist/docker-install/sdkwork-webserver-install-<version>.bundle/
├── image.tar.gz / image.sha256 / image.env   # 镜像归档 + 校验 + tag
├── compose/
│   ├── docker-compose.bundle.yml             # 环境中立的多实例模板
│   └── docker-compose.bundle-edge.yml        # 实例 1 的 80/443 边缘 overlay
├── env/
│   ├── development.env.example
│   ├── test.env.example
│   └── production.env.example
├── deploy.sh                                 # 通用部署脚本（唯一入口）
├── manifest.json                             # 版本 / 镜像 / sha256 元数据
└── README.md
```

## 3. 部署（任意 Docker 主机）

把 bundle 目录拷贝到目标主机后，唯一入口是 `deploy.sh`：

```bash
# 内置 postgres/redis（默认）
bash deploy.sh --environment development

# 生产环境 3 实例
bash deploy.sh --environment production --replicas 3

# 外部 postgres/redis + 2 实例
bash deploy.sh --environment production --external --replicas 2

# 其它操作
bash deploy.sh --environment test --ps
bash deploy.sh --environment test --logs 2        # 跟踪实例 2 日志
bash deploy.sh --environment test --down
bash deploy.sh --environment test --down --purge  # 连同卷/网络一并删除
```

关键规则：

- `--environment` 必填（development | test | production），缺失或未知**在产生任何副作用之前**报错退出；重复执行 apply 幂等（原地更新既有栈）。
- 首次部署会从 `env/<environment>.env.example` 生成 `env/<environment>.env`，**请先填好密钥再对外暴露**。
- 若镜像未加载，脚本自动 `docker load image.tar.gz`。
- 实例 1 先启动并等待健康（含数据库迁移），随后才启动实例 2..N，避免迁移竞态。

仓库内等价命令：

```bash
pnpm deploy:apply:standalone:docker -- --environment production --replicas 3
```

## 4. 环境配置（部署时选择环境）

编辑 `env/<environment>.env`（继承现有 `deployments/docker/env/*.env.example` 的全部键）：

| 键 | 作用 |
| --- | --- |
| `SDKWORK_WEBSERVER_IMAGE_TAG` | 镜像 tag（bundle 的 `image.env` 也会提供） |
| `SDKWORK_WEBSERVER_ENVIRONMENT` | 生命周期环境（compose 必填项） |
| `WEBSERVER_POSTGRES_*` / `PG_MAX_CONNECTIONS` | 内置 postgres 依赖 |
| `SDKWORK_DATABASE_*`、`SDKWORK_WEBSERVER_REDIS_*` | 数据库 / Redis 连接（`--external` 时指向外部实例） |
| `SDKWORK_SPACE_HOST_PATH` | 宿主机 `/opt/deploy` 绑定挂载（模块导入面） |
| `SDKWORK_WEBSERVER_PRIMARY_DOMAIN`、`SDKWORK_CORS_ALLOWED_ORIGINS` | 域名与 CORS |
| `SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT` | bundle 默认 `bundled`（镜像内置网关进程），可改 `docker`/`external` |

## 5. 多实例拓扑（每个环境均支持）

```text
宿主机（每个环境一组）
├─ 网络sdkwork-webserver-<env>            （实例与依赖共享）
├─ 卷  sdkwork-webserver-<env>-secrets    （共享：加密密钥/ACME 账号一致）
├─ 卷  sdkwork-webserver-<env>-data       （共享：TLS 材料/运行数据）
├─ 依赖项目  sdkwork-webserver-<env>-deps（内置模式: postgres + redis）
└─ 实例项目  sdkwork-webserver-<env>-i<i>
     ├─ i1：mgmt base+0 -> 3800，另发布 80/443 边缘；先启动，先迁移
     ├─ i2：mgmt base+1 -> 3800
     └─ iN：mgmt base+N-1 -> 3800
```

- 每实例节点身份：`SDKWORK_WEBSERVER_NODE_UUID=standalone-<env>-i<i>`（容器内 entrypoint 亦有按主机名兜底的唯一默认值）。
- 实例间负载均衡：对实例管理端口（base..base+N-1）做外部 LB；80/443 边缘只在实例 1。
- 多实例前提：共享 PostgreSQL / Redis（内置或外部均可），由实例 1 完成迁移。
- 卷是**按环境共享**的：同一环境 N 个实例引用同一份 secrets/data，跨实例密钥一致。

默认管理端口基准（env 文件可覆盖）：development `13800`、test `18888`、production `18080`；边缘端口 dev `80/443`、test `18898/28430`、prod `18098/38430`。

## 6. 与既有命令的关系

| 命令 | 用途 |
| --- | --- |
| `pnpm build:container:standalone` | 只构建统一安装镜像（不打包 bundle） |
| `pnpm build:container:install` | 构建 + 打包自包含安装 bundle（本指南） |
| `pnpm deploy:apply:standalone:docker` | 仓库内运行 bundle deploy.sh |
| `build:container:*` | 容器构建入口，新自动化请用 `build:container:*` / `deploy:apply:*` |

## 7. Webserver 规范合规要点（SDKWORK_WEBSERVER_SPEC.md）

bundle 的 compose 模板与部署脚本严格遵循 `SDKWORK_WEBSERVER_SPEC.md`：

| 规范点 | 实现 |
| --- | --- |
| §17 空间挂载 | 空间根 `/opt/deploy` 只读挂载；`sdkwork-space` checkout 子树为可写 overlay（entrypoint clone/pull 目标） |
| §17 模块导入 | `SDKWORK_SPACE_AUTO_DISCOVER` / `SDKWORK_SPACE_MODULES` / `MODULE_IMPORT_REQUIRED` / `PROBE_UPSTREAMS` 全部透传；模块静态资源按 `apps/*-{pc,h5}/dist/standalone/<envAlias>/` 从 checkout 解析（§13.6 / §17.1） |
| §17.3 import 面 | `SDKWORK_WEBSERVER_IMPORT_PROFILE` 默认 `cloud`（双集合 imports.d，entrypoint 启动时物化）；`SDKWORK_WEBSERVER_IMPORT_LISTENER_PORTS` 透传（默认身份 80/443） |
| §17 多集群/多实例 | 每个容器内部统一监听网关端口 3800，模块 `server.standalone.toml` upstream 在实例与宿主间保持一致；宿主端口按实例区分 |
| §17.4 standalone-only | bundle 仅由 standalone 发布产物打包（`webserver-release.mjs --deployment-profile standalone`），manifest 声明 `deploymentProfile: standalone` |
| §8.1 网关 upstream | 模块 /api/ 反向代理走保留 upstream `gateway`；bundle 默认 `SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=bundled`（镜像内网关进程，端口 3900），可切换 `docker`/`external` 并由 env 文件指定网关主机 |

## 8. 配置多个 webserver（每实例独立配置）

同一环境部署多个实例时，可为每个实例提供独立的配置覆盖文件：

```text
env/production.env            # 环境级基础配置（所有实例共享）
env/production.i1.env         # 实例 1 专属覆盖（可选）
env/production.i2.env         # 实例 2 专属覆盖（可选）
```

- deploy.sh 检测到 `env/<environment>.i<N>.env` 时，以第二个 `--env-file` 叠加（compose 后者覆盖前者），并打印提示。
- 典型用途：不同实例绑定不同主域名（`SDKWORK_WEBSERVER_PRIMARY_DOMAIN`）、不同 clone URL、不同 TLS/ACME profile 或任意部署输入。
- 未提供覆盖文件的实例继续使用环境基础配置；管理端口与节点身份始终由 deploy.sh 按实例自动分配，不受覆盖影响。

English version: [docker-install.en.md](./docker-install.en.md)
