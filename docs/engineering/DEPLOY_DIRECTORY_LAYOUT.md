# SDKWork WSL（Ubuntu 22.04）部署目录结构与 webserver 挂接规范

> 依据 2026-09-10 对 `/opt/deploy` 实际部署态的盘点整理。权威规范：`SDKWORK_WEBSERVER_SPEC.md` §17.3 / §13.6、`ENVIRONMENT_SPEC.md` §6.2.2 / §5.1.0.1、`RUNTIME_DIRECTORY_SPEC.md`、`APPLICATION_DEPLOY_LAYOUT_SPEC.md`、`OPERATIONS_SPEC.md`。

---

## 1. 总览：三平面模型

WSL 内所有模块部署围绕 `/opt/deploy` 展开，分为三个平面：

| 平面 | 路径 | 职责 |
| --- | --- | --- |
| 模块部署根 | `/opt/deploy/<module>/` | 每个独立模块的 bundle（compose/env/脚本/镜像归档）、备份、发布产物 |
| 工作区检出台 | `/opt/deploy/sdkwork-space/` | 全工作区源码镜像：webserver 导入面的 sidecar 配置源、各模块 Adaptive Web 静态根、数据库模块清单 |
| 共享设施 | `/opt/deploy/{archives, backups, deps, drive, logs}` | 镜像归档、跨模块备份、依赖、云盘数据、日志 |

**实测顶层结构：**

```text
/opt/deploy/
├── archives/                          # 镜像/离线包归档
├── backups/                           # 跨模块备份索引
├── deps/                              # 共享依赖
├── drive/                             # 云盘对象存储（挂入 webserver 容器，rw）
├── logs/                              # 运营日志
├── sdkwork-space/                     # 工作区检出台（~134 个仓库，挂入 webserver 容器，rw）
├── sdkwork-webserver/                 # webserver 模块部署根
├── sdkwork-api-cloud-gateway/         # 网关模块部署根
├── sdkwork-cloudrouter/               # cloudrouter 模块部署根
├── .previous-20260830-000926/         # 历史迁移留存（勿动）
└── .static-backup-20260905-163931/    # 静态迁移备份
```

---

## 2. 模块部署根规范：`/opt/deploy/<module>/`

所有独立模块遵循统一布局（差异点见 §2.3）：

```text
/opt/deploy/<module>/
├── bundle/                            # 运行时 bundle（install/upgrade 的推送目标）
│   ├── compose/
│   │   ├── docker-compose.bundle.yml  # 主 compose（webserver 额外有 -edge/-gateway 变体）
│   │   └── postgres/init/             # 嵌入式依赖模式的 PG 初始化脚本
│   ├── env/
│   │   ├── <env>.env                  # 环境部署输入（deploy 从不重写已配置值）
│   │   ├── <env>.env.example          # 模板
│   │   └── <env>.i1.env               # 实例分层覆盖（多副本抗并发回退保险，webserver 使用）
│   ├── deploy.sh                      # bundle 入口：install/upgrade/status/logs/config
│   ├── release.sh                     # 发布门禁：健康检查、自动回滚、追加式台账、发布锁
│   ├── release-state/<env>/           # 追加式发布台账（gateway 模块）
│   ├── image.tar.gz                   # 镜像归档（目标机已有同版本镜像时 push 阶段排除）
│   ├── image.env                      # 镜像 tag 记录
│   ├── image.sha256                   # 镜像摘要证据
│   └── manifest.json                  # bundle 清单
├── backups/
│   └── sdkwork-<module>-<env>-<UTC时间戳>/   # 变更前备份集（env + config + database.dump）
└── packages/                          # 静态发布产物（如 cloudrouter 的 pc/h5 tar）
```

### 2.1 三个模块的 compose 变体差异

| 模块 | compose 文件 | 说明 |
| --- | --- | --- |
| `sdkwork-webserver` | `bundle.yml`（postgres/redis/webserver/sdkwork）+ `bundle-gateway.yml`（gateway/knowledgebase-rpc sidecar 组）+ `bundle-edge.yml`（边缘单服务） | 主 bundle 声明嵌入式 PG/Redis；实际部署按 deps 模式选外部/嵌入式 |
| `sdkwork-api-cloud-gateway` | `bundle.yml` + `bundle.embedded.yml` + `bundle.attach-override.yml` | attach-override 定义被 webserver 挂接的契约（见 §4.2） |
| `sdkwork-cloudrouter` | `bundle.yml`（单服务，env/config + env/secrets 两个配置目录） | 容器单端口 3900，环境级管理端口映射 395x |

### 2.2 端口平面（宿主侧）

| 模块 | 端口 | 说明 |
| --- | --- | --- |
| webserver | 80 / 443 + 13xxx / 18xxx | 公网边缘 + 管理面 |
| api-cloud-gateway | 391x → 容器 8080 | dev=3910, test=3911, staging=3912, production=3913 |
| cloudrouter | 395x → 容器 3900 | dev=3950, test=3951, staging=3952, production=3953, demo=3954 |
| 健康探针 | `/healthz`、`/readyz` | 各模块容器内发布 |

---

## 3. 容器命名与数据卷规范

```text
容器：sdkwork-<module>-<env>[-i<N>]-<service>-<idx>     例：sdkwork-cloudrouter-development-i1-cloudrouter-1
网络：sdkwork-<module>-<env>                            例：sdkwork-api-cloud-gateway-development
卷：  sdkwork-<module>-<env>[-i<N>|-组名]{-data|-secrets|-postgres-data|-postgres-tls|-redis-data|-gateway-data|-gateway-secrets}
```

实测卷清单（按模块）：

- **webserver**：`-data`（→ `/var/lib/sdkwork/webserver`）、`-secrets`（→ `/etc/sdkwork/webserver/secrets`）、`-postgres-data`、`-redis-data`（嵌入式模式）、gateway sidecar 组 `-gateway-data`（→ `/var/lib/sdkwork/api-gateway`）、`-gateway-secrets`（→ `/run/secrets/sdkwork/api-gateway`）
- **api-cloud-gateway**：`-data`、`-secrets`、`-postgres-data`、`-postgres-tls`、`-redis-data`
- **cloudrouter**：`-data`（→ `/var/lib/sdkwork/router`）、`-secrets`（→ `/etc/sdkwork/router/secrets`）、`-postgres-data`、`-redis-data`（嵌入式预留）

---

## 4. webserver 容器的挂接结构（实测）

### 4.1 挂载清单（`sdkwork-webserver-development-i1-webserver-1`）

| 类型 | 宿主路径 | 容器路径 | 读写 | 用途 |
| --- | --- | --- | --- | --- |
| bind | `/opt/deploy` | `/opt/deploy` | **ro** | 全部署根只读视图（读兄弟模块 bundle/env、镜像清单等） |
| bind | `/opt/deploy/sdkwork-space` | `/opt/deploy/sdkwork-space` | **rw** | 导入面 sidecar 源 + 各模块 Adaptive Web 静态根（PC/H5 dist） |
| bind | `/opt/deploy/drive` | `/opt/deploy/drive` | rw | 云盘对象存储 |
| volume | `sdkwork-webserver-<env>-data` | `/var/lib/sdkwork/webserver` | rw | ACME 账户、ACME webroot、TLS 材料、management.log |
| volume | `sdkwork-webserver-<env>-secrets` | `/etc/sdkwork/webserver/secrets` | rw | webserver 密钥材料 |

### 4.2 容器内进程与配置面

同一镜像二进制 `sdkwork-api-webserver-standalone-gateway` 以两个角色运行：

```text
serve-imports      # nginx 兼容的模块导入面（反代聚合，PID 1 子进程）
serve-management   # Adaptive Web 管理控制台（entrypoint 生成配置后拉起）
```

容器内 `/etc/sdkwork/webserver/` 结构：

```text
/etc/sdkwork/webserver/
├── config.toml                     # webserver 自身运行配置
├── imports.d/                      # 导入面配置（entrypoint 启动时生成）
│   ├── import.conf                 # 当前激活 import set（include 列表）
│   ├── import.conf.cloud           # cloud 集模板：逐模块 include nginx.cloud.<env>.conf
│   ├── import.conf.standalone      # standalone 集模板：逐模块 include nginx.standalone.<env>.conf
│   └── product-edge-nginx.conf     # 产品级边缘公共配置
├── module-app-roots/
│   └── <module>.toml               # 逐模块 Adaptive Web 静态根目录（entrypoint 生成）
├── modules/                        # 导入模块清单
└── secrets/                        # （卷挂载）
```

导入面 include 规则（`SDKWORK_WEBSERVER_SPEC.md` §17.3，高内聚低耦合——webserver 不复制不改写模块配置）：

```nginx
# import.conf（cloud 集，示例片段）
include /etc/sdkwork/webserver/imports.d/product-edge-nginx.conf;
include /opt/deploy/sdkwork-space/<module>/deployments/webserver/nginx.cloud.<environment>.conf;   # × 77 模块 × 5 环境
```

激活集切换：`pnpm import:switch:cloud|standalone`（容器启动用 `SDKWORK_WEBSERVER_IMPORT_PROFILE`，默认 `cloud`），切换后重启 `serve-imports`。

---

## 5. 模块被挂接的两条通道

### 5.1 通道 A：反代导入面（sidecar conf）

**源**：每个模块检出台内的 `deployments/webserver/` 目录（实测 77 个模块具备）：

```text
/opt/deploy/sdkwork-space/<module>/deployments/
├── deploy.yaml                       # 部署声明
└── webserver/
    ├── server.common.toml            # 生成源（公共段）
    ├── server.<profile>.toml         # 生成源（standalone / cloud 段）
    ├── server.<env>.toml             # 生成源（环境段）
    ├── nginx.<profile>.<env>.conf    # 生成产物（勿手改）× 2 profile × 5 env
    ├── app-roots.example.toml        # Adaptive Web 静态根声明模板
    └── static/                       # 模块静态回退根
```

sidecar conf 结构（由 `server.common.toml + server.<profile>.toml` 生成）：

```nginx
upstream gateway {
    server <upstream>;          # 见下方契约
}
server {
    listen 80;
    server_name <module>-dev.sdkwork.com ...;    # 15 个域名族
    include snippets/gateway-api-locations.nonproduction.conf;   # 镜像内置片段库
    location / { root <静态根或代理>; ... }
}
```

**upstream 契约（attach 契约，`bundle.attach-override.yml` 权威）**：

| profile | upstream | 机制 |
| --- | --- | --- |
| `cloud` | `sdkwork-api-cloud-gateway:8080`（checkout-direct，绝不改写） | webserver 的 gateway sidecar 服务在 `sdkwork-webserver-<env>` 网络上注册网络别名 `sdkwork-api-cloud-gateway`（实测 Aliases 含该名），独立部署的 api-cloud-gateway 挂接 webserver 网络时必须走 attach-override（别名 + 8080 容器端口） |
| `standalone` | `127.0.0.1:3800`（进程内网关） | 容器 `network_mode: host` / 同机进程 |

### 5.2 通道 B：Adaptive Web 静态根（module-app-roots）

容器内 `/etc/sdkwork/webserver/module-app-roots/<module>.toml`（entrypoint 从模块的 `app-roots.example.toml` 生成），指向 **sdkwork-space 检出台内的 dist 目录**（这是 `sdkwork-space` 必须以 rw 挂入的原因）：

```toml
[app_roots]
tablet_surface = "pc"
pc_static_root = "/opt/deploy/sdkwork-space/<module>/apps/<module>-pc/dist/cloud/dev"
h5_static_root = "/opt/deploy/sdkwork-space/<module>/apps/<module>-h5/dist/cloud/dev"
static_fallback_root = "/opt/deploy/sdkwork-space/<module>/deployments/webserver/static"

[app_roots.pc_static_by_environment]
development = ".../dist/cloud/dev"   # × 5 环境
test        = ".../dist/cloud/test"
...
```

> 注意：cloud 导入集使用 `dist/cloud/<env>`，standalone 模式使用 `dist/standalone/<env>`（`ENVIRONMENT_SPEC.md` §5.1.0.1：所有模块双模式构建）。产物规范布局为 `apps/<module>-{pc,h5}/dist/<profile>/<env-alias>`，env-alias ∈ {dev, test, staging, demo, prod}。

---

## 6. 独立模块容器挂接明细（实测）

### 6.1 sdkwork-webserver（见 §4.1）

另有 gateway sidecar 组（`bundle-gateway.yml`，与 webserver 同网络 `sdkwork-webserver-<env>`）：

| 容器 | 挂载 |
| --- | --- |
| `...-gateway-gateway-1` | volume `...-gateway-data` → `/var/lib/sdkwork/api-gateway`；volume `...-gateway-secrets` → `/run/secrets/sdkwork/api-gateway`；网络别名 `gateway`、`sdkwork-api-cloud-gateway` |
| `...-gateway-knowledgebase-rpc-1` | 同上两组卷（RPC 依赖面） |

### 6.2 sdkwork-api-cloud-gateway（独立网关模块）

| 容器 | 挂载 |
| --- | --- |
| `sdkwork-api-cloud-gateway-<env>-i1-gateway-1` | bind `/dev/null` → `/run/sdkwork/api-gateway/postgres-ca-bundle.crt`（外部 PG 模式占位）；volume `-secrets` → `/run/secrets/sdkwork/api-gateway`；volume `-data` → `/var/lib/sdkwork/api-gateway` |
| `...-deps-knowledgebase-rpc-1` | 同上 |

端口：宿主 391x → 容器 8080（dev=3910）。嵌入式模式附加卷 `-postgres-data`、`-postgres-tls`、`-redis-data`，bundle 内 `postgres/entrypoint.sh + init/` 提供初始化。

### 6.3 sdkwork-cloudrouter

| 容器 | 挂载 |
| --- | --- |
| `sdkwork-cloudrouter-<env>-i1-cloudrouter-1` | bind `bundle/env/config/<env>.config.toml` → `/etc/sdkwork/router/config.toml`；bind `bundle/env/secrets/<env>/` → `/run/secrets/sdkwork/router`；volume `-data` → `/var/lib/sdkwork/router`；volume `-secrets` → `/etc/sdkwork/router/secrets` |

关键环境变量（compose `environment:` 显式映射，env 文件定义 ≠ 容器可见）：`SDKWORK_CLOUDROUTER_ENVIRONMENT`（demo 运行时映射为 staging）、`SDKWORK_CLOUDROUTER_MGMT_HOST_PORT`、`SDKWORK_IAM_SIGNING_MASTER_SECRET`（fail-closed）。config.toml 的 `[install].environment` 仅接受 development|test|staging|production。

---

## 7. 运营平面约定（OPERATIONS_SPEC.md）

- **备份**：`/opt/deploy/<module>/backups/sdkwork-<module>-<env>-<UTC时间戳>/`，含 env 快照、config.tar.gz、database.dump；staging/demo/production 的 install/upgrade 强制变更前备份（`--skip-backup` 记证据）。
- **发布锁与台账**：`release.sh`（健康门禁、自动回滚、追加式台账、发布锁）；gateway 模块台账在 `bundle/release-state/<env>/`。
- **doctor 9 项**：`bin/doctor.sh`（toolchain/bundle/compose/health/probe/config/logs/disk 等）。
- **证据链**：`bundle/manifest.json + image.sha256 + image.env` 与发布产物一一对应。

---

## 8. 已知偏差（盘点时发现，待回流规范）

1. `sdkwork-cloudrouter` 部署根缺少 `bundle/image.tar.gz / image.env / image.sha256 / manifest.json`（0.4.x 走源 bundle + 本机镜像通道，未产出自包含 stage-2 安装包）。
2. demo 环境为 2026-09-10 首次成功部署：`deploy.sh` 将运行时环境 `demo` 映射为 `staging`（router 二进制枚举不含 demo）；管理端口仍走 3954 demo 平面。该枚举缺口待回流到 specs。
3. webserver 各环境 `env/*.bak*` 手工备份文件散落在 bundle/env/（建议统一迁往 backups/）。
4. `/etc/sdkwork/webserver/modules` 容器内为空目录（清单机制未启用）。
