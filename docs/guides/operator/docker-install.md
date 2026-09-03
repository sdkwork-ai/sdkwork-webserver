# sdkwork-webserver Docker 安装与部署手册（三环境 · 傻瓜式）

> 版本：2026-09-03 · 适用：`sdkwork-webserver` standalone 统一安装镜像
> 规范依据：`sdkwork-specs/DEPLOYMENT_SPEC.md` §6、`SDKWORK_WEBSERVER_SPEC.md` §17、`sdkwork-specs/PNPM_SCRIPT_SPEC.md` §4.4
> English version: [docker-install.en.md](./docker-install.en.md)

**核心理念：一个镜像打天下。** 镜像与环境无关（不烘焙任何域名/数据库/凭据），development / test / production 用**同一个镜像 tag**，环境差异全部是部署时输入（env 文件）。改环境 = 改 env 文件，不需要重新构建。

---

## 0. 十分钟快速上手（TL;DR）

### 场景 A：拿到安装 bundle 的全新 Docker 主机（最快路径）

```bash
# 1. 解压 bundle，进入目录
tar -xzf sdkwork-webserver-install-<version>.bundle.tar.gz
cd sdkwork-webserver-install-<version>.bundle

# 2. 一条命令部署（内置 postgres/redis，自动 load 镜像、生成 env、跑迁移）
bash deploy.sh --environment development   # 或 test / production

# 3. 验证
curl http://127.0.0.1:13800/healthz        # 预期 {"status":"ok"}

# 4. 浏览器访问
#    开发 http://127.0.0.1:13800  测试 http://127.0.0.1:18888  线上 http://127.0.0.1:18080
```

> 首次部署会从 `env/<env>.env.example` 生成 env 文件，对外暴露前**必须**把 `<CHANGE_ME>` 替换为真实密钥。

### 场景 B：仓库内打新镜像 + 三环境部署（开发/打包机路径）

在仓库根目录（WSL 内 ext4 环境，见 §2.4）：

```bash
# 1. 打新镜像（tag 版本自动取自 sdkwork.app.config.json currentVersion）
pnpm build:container:standalone -- --skip-platform-gateway

# 2. 部署三个环境（development/test/production 一次性）
bash scripts/docker/deploy-docker-environment.sh all --validate

# 3. 验证（预期全部 200）
for p in 13800 18888 18080; do curl -s --noproxy '*' http://127.0.0.1:$p/healthz; echo; done
```

### 部署完成后的访问入口

| 环境 | 管理面（SPA+API 同源，推荐） | healthz | 数据面（域名 Host 路由） |
| --- | --- | --- | --- |
| 开发 | http://127.0.0.1:13800 | `/healthz` | `http://server-dev.sdkwork.com:80`（Host: server-dev.sdkwork.com） |
| 测试 | http://127.0.0.1:18888 | `/healthz` | `http://server-test.sdkwork.com:18898`（Host: server-test.sdkwork.com） |
| 线上 | http://127.0.0.1:18080 | `/healthz` | `http://server.sdkwork.com:18098`（Host: server.sdkwork.com） |

> 域名形式需在本机 hosts 把三个域名指向 `127.0.0.1`（或 WSL IP）。数据面按域名路由，不带 Host 头直连会落默认站点；管理端口无此要求，是测试首选入口。

---

## 1. 前置条件与一次性准备

| 依赖 | 要求 | 检查命令 |
| --- | --- | --- |
| Docker | 24+，守护进程可达（WSL 内亦可） | `docker version` |
| Node.js | 22+（仅打包机需要） | `node -v` |
| pnpm | 10+（仅打包机需要） | `pnpm -v` |
| Rust toolchain | cargo 1.8x（仅打新镜像需要） | `cargo --version` |
| 端口 | 13800/18888/18080 + 数据面端口未被占用 | `ss -ltn \| grep -E '13800\|18888\|18080'` |

一次性准备：

```bash
# 1. 环境文件：从 example 复制（bundle 链 deploy.sh 会自动复制；仓库链手动）
cd deployments/docker/env
cp development.env.example development.env
cp test.env.example test.env
cp production.env.example production.env

# 2. 把所有 <CHANGE_ME> 替换为真实密钥（数据库密码、会话密钥等）
grep -n 'CHANGE_ME' development.env test.env production.env

# 3. 宿主机挂载目录（模块导入面）
sudo mkdir -p /opt/deploy
```

---

## 2. 打新的镜像包

### 2.1 版本号在哪里改

镜像 tag 版本取自 **`sdkwork.app.config.json` → `release.currentVersion`**。发新版本只改这一处：

```bash
# 例：0.1.0 → 0.1.1
node -e "const f='sdkwork.app.config.json';const j=require('./'+f);j.release.currentVersion='0.1.1';require('fs').writeFileSync(f,JSON.stringify(j,null,2)+'\n')"
```

### 2.2 路线 A：仓库 release 链（standalone 镜像）

```bash
# 一步到位：release 构建 + standalone 镜像（tag = registry.sdkwork.com/apps/sdkwork-webserver-standalone:<version>）
pnpm build:container:standalone

# 已有 release 产物、且网关走独立容器（attach/docker）时跳过内嵌网关：
node scripts/docker/build-standalone-image.mjs --skip-platform-gateway

# 只重跑 release 安装包（tar.gz + SBOM），不构建镜像：
node scripts/webserver-release.mjs package --deployment-profile standalone
```

产物：

```text
dist/release/sdkwork-webserver-linux-x64-standalone-server-<version>.tar.gz   # 安装包 + SBOM
docker image registry.sdkwork.com/apps/sdkwork-webserver-standalone:<version> # 统一镜像
```

### 2.3 路线 B：自包含安装 bundle（交付给任意 Docker 主机）

```bash
pnpm build:container:install                       # 构建镜像 + 打包 bundle
pnpm build:container:install -- --skip-image-build # 复用已构建镜像，只重新打包
pnpm build:container:install -- --tag 0.1.0 --out dist/docker-install --dry-run
```

从 sdkwork-space 根目录（WSL / CI）也可以走 `bin` 脚本：

```bash
bash bin/build-webserver-docker.sh                 # 默认读取 manifest 版本
bash bin/build-webserver-docker.sh --out /opt/deploy/packages
```

bundle 布局：

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

### 2.4 WSL / DrvFS 构建须知（重要，否则必然失败）

源码若在 `/mnt/<盘>`（Windows 盘挂载），**禁止直接跑 pnpm/cargo**：pnpm store 的 SQLite 走 9p 文件系统必报 `disk I/O error`。正确做法是 rsync 到 WSL ext4 副本构建：

```bash
# 1. 同步仓库 + pnpm workspace 引用的全部兄弟仓库（缺一个都会
#    ERR_PNPM_WORKSPACE_PKG_NOT_FOUND；清单见 pnpm-workspace.yaml）
mkdir -p ~/sdkwork-build && rsync -a \
  --exclude target --exclude 'node_modules*' --exclude dist --exclude .git \
  /mnt/e/sdkwork-space/sdkwork-webserver/ ~/sdkwork-build/sdkwork-webserver/
#    同时同步 pnpm-workspace.yaml 引用的 ../<repo> 兄弟仓库 + ../sdkwork-specs + ../sdkwork-github-workflow

# 2. ext4 副本内安装依赖（首次 ~5min）
cd ~/sdkwork-build/sdkwork-webserver && pnpm install

# 3. Rust 依赖闭包：cargo 缺哪个兄弟仓库就从 /mnt/e 单仓 rsync 进来重试
#    （脚本开头加 [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"，否则 cargo ENOENT）

# 4. UI 静态资源：rsync 排除了 dist，需要先构建浏览器端依赖闭包
pnpm --filter "@sdkwork/webserver-pc..." --filter "@sdkwork/webserver-h5..." build

# 5. 然后在 ~/sdkwork-build/sdkwork-webserver 内执行 §2.2 的打包命令
```

构建完成后把产物拷回仓库目录（compose 卷挂载依赖仓库内路径）：

```bash
cp ~/sdkwork-build/sdkwork-webserver/target/release/sdkwork-api-webserver-standalone-gateway \
   /mnt/e/sdkwork-space/sdkwork-webserver/target/release/
```

### 2.5 产物校验

```bash
sha256sum dist/release/*.tar.gz                     # 记入发布报告
docker images | grep sdkwork-webserver-standalone  # 确认新 tag 存在
```

---

## 3. 环境配置（选环境 = 改 env 文件）

env 文件位置：仓库链 `deployments/docker/env/<env>.env`；bundle 链 `env/<env>.env`（deploy.sh 自动生成）。完整键位说明见各 `.env.example` 内注释与 [CONFIG_PATHS.md](./CONFIG_PATHS.md)。

### 3.1 必填键速查表

| 键 | 作用 | 默认/示例 |
| --- | --- | --- |
| `SDKWORK_WEBSERVER_IMAGE_TAG` | 镜像 tag（三环境一致，即"同一镜像"） | `0.1.0` |
| `SDKWORK_WEBSERVER_*_HOST_PORT` | 各环境管理端口 | dev 13800 / test 18888 / prod 18080 |
| `SDKWORK_WEBSERVER_*_IMPORT_HTTP_HOST_PORT` | 各环境数据面 HTTP 端口 | dev 80 / test 18898 / prod 18098 |
| `WEBSERVER_POSTGRES_*` / `PG_MAX_CONNECTIONS` | 内置 postgres（embedded 模式） | `<CHANGE_ME>` 必须替换 |
| `WEBSERVER_POSTGRES_HOST` / `WEBSERVER_REDIS_HOST` | external 模式指向外部实例 | `host.docker.internal` |
| `SDKWORK_WEBSERVER_PRIMARY_DOMAIN` | 主域名（数据面路由/CORS 依据） | `sdkwork.com` |
| `SDKWORK_CORS_ALLOWED_ORIGINS` | 允许的跨域来源 | 含三环境域名 |
| `SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT` | /api/ 网关模式 | `bundled`（镜像内）\| `docker`（兄弟容器）\| external（attach） |
| `SDKWORK_DATABASE_SEED_LOCALE` | 种子数据语言 | `zh-CN` |

### 3.2 独立网关 attach 配置（网关已独立部署时）

模块 `/api/` 反代**字面直连** `sdkwork-api-cloud-gateway:8080`（永不重写，SDKWORK_WEBSERVER_SPEC §17.3）。网关独立舰队必须满足：网络别名 `sdkwork-api-cloud-gateway` + 容器内监听 8080。在 env 文件中：

```bash
SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=external
SDKWORK_MODULE_API_GATEWAY_HOST=sdkwork-api-cloud-gateway
SDKWORK_MODULE_API_GATEWAY_PORT=8080
# attach 网络：必须与网关舰队实际网络名一致（bundle 舰队为 sdkwork-api-cloud-gateway-<env>）
SDKWORK_MODULE_API_GATEWAY_ATTACH_NETWORK=sdkwork-api-cloud-gateway-development   # test/production 同理
```

自检：`docker exec sdkwork-webserver-development getent hosts sdkwork-api-cloud-gateway` 必须能解析；`docker exec <网关容器> curl -s http://127.0.0.1:8080/readyz` 必须 200。

---

## 4. 一键部署三环境

### 4.1 仓库链：`deploy-docker-environment.sh`（推荐开发/打包机使用）

```bash
bash scripts/docker/deploy-docker-environment.sh development --validate
bash scripts/docker/deploy-docker-environment.sh test        --validate
bash scripts/docker/deploy-docker-environment.sh production  --validate
# 或一次性三环境（development/test/production）：
bash scripts/docker/deploy-docker-environment.sh all --validate

# 其它操作
bash scripts/docker/deploy-docker-environment.sh staging          # 单目标部署
bash scripts/docker/deploy-docker-environment.sh all --down       # 停止三环境
bash scripts/docker/deploy-docker-environment.sh all --pull       # 先拉镜像再 up
```

规则：

- env 文件缺失直接报错（提示从 example 复制），不会产生半部署状态。
- `--validate` 在 compose up 前校验 env 完整性。
- 成功输出 `deployed <env> (sdkwork-webserver-<env>) -> http://127.0.0.1:<port>/healthz`。

### 4.2 bundle 链：`deploy.sh`（任意 Docker 主机，支持多实例）

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
- 若镜像未加载，脚本自动 `docker load image.tar.gz`。
- 实例 1 先启动并等待健康（含数据库迁移），随后才启动实例 2..N，避免迁移竞态。

仓库内等价命令：`pnpm deploy:apply:standalone:docker -- --environment production --replicas 3`

---

## 5. 部署后验证（3 分钟）

### 5.1 健康巡检（可直接复制执行）

```bash
# ① 容器应全部 healthy
docker ps --format '{{.Names}}\t{{.Status}}' | grep sdkwork-webserver

# ② 管理面 healthz 应全部 200 {"status":"ok"}
for p in 13800 18888 18080; do
  echo -n "$p -> "; curl -s --noproxy '*' http://127.0.0.1:$p/healthz; echo
done

# ③ 数据面 SPA（域名 Host 路由）应全部 200 并返回 HTML
curl -s --noproxy '*' -o /dev/null -w 'dev  %{http_code}\n' -H 'Host: server-dev.sdkwork.com'  http://127.0.0.1/
curl -s --noproxy '*' -o /dev/null -w 'test %{http_code}\n' -H 'Host: server-test.sdkwork.com' http://127.0.0.1:18898/
curl -s --noproxy '*' -o /dev/null -w 'prod %{http_code}\n' -H 'Host: server.sdkwork.com'      http://127.0.0.1:18098/

# ④ 网关 attach 契约（external 模式）
docker exec sdkwork-webserver-development getent hosts sdkwork-api-cloud-gateway
```

> WSL 环境若配了 `http_proxy`，curl 必须带 `--noproxy '*'`，否则结果不可信。

### 5.2 多实例拓扑（每个环境均支持）

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

- 每实例节点身份：`SDKWORK_WEBSERVER_NODE_UUID=standalone-<env>-i<i>`。
- 实例间负载均衡：对实例管理端口做外部 LB；80/443 边缘只在实例 1。
- 多实例前提：共享 PostgreSQL / Redis（内置或外部均可），由实例 1 完成迁移。

### 5.3 每实例独立配置（可选）

```text
env/production.env            # 环境级基础配置（所有实例共享）
env/production.i1.env         # 实例 1 专属覆盖（可选）
env/production.i2.env         # 实例 2 专属覆盖（可选）
```

deploy.sh 检测到 `env/<environment>.i<N>.env` 时以第二个 `--env-file` 叠加（后者覆盖前者）。典型用途：不同实例绑不同主域名、不同 clone URL、不同 TLS/ACME profile。管理端口与节点身份始终由脚本按实例自动分配。

---

## 6. 日常运维

### 6.1 升级到新镜像

```bash
# 1. 按 §2 打新镜像（bump currentVersion → build）
# 2. 改三份 env 的 SDKWORK_WEBSERVER_IMAGE_TAG=<新版本>
# 3. 原地重建（数据卷保留，迁移由容器启动时自动执行）
bash scripts/docker/deploy-docker-environment.sh all --validate
# 4. 复跑 §5.1 巡检
```

> 仓库链提示：`deployments/docker/docker-compose.<env>.yml` 把宿主 `target/release/sdkwork-api-webserver-standalone-gateway` 以只读方式挂进容器（混合模式）。若走此路径，升级时需把新构建的二进制同步拷到该路径（见 §2.4 最后一步）；使用纯镜像部署（bundle 链）则无此步骤。

### 6.2 停止 / 清理

```bash
bash scripts/docker/deploy-docker-environment.sh all --down   # 停止（保留卷）
# 彻底清理（谨慎：删除数据卷）
docker compose -p sdkwork-webserver-development down --volumes
```

### 6.3 回滚

env 文件把 `SDKWORK_WEBSERVER_IMAGE_TAG` 改回旧 tag，重新 `up` 即可（镜像仍在本地时秒级完成；数据库回滚需按 `database/` 的迁移策略另行处理）。

---

## 7. 常见问题排查（症状 → 原因 → 处置）

| 症状 | 原因 | 处置 |
| --- | --- | --- |
| `pnpm install` 报 `disk I/O error` | 在 /mnt/e 等 DrvFS 上跑 pnpm | 按 §2.4 用 ext4 副本构建 |
| `ERR_PNPM_WORKSPACE_PKG_NOT_FOUND @sdkwork/...` | ext4 副本缺兄弟仓库 | 按 pnpm-workspace.yaml 清单补 rsync `../<repo>` |
| `spawnSync cargo ENOENT` | 非登录 shell 无 PATH | 脚本头 `. "$HOME/.cargo/env"` |
| TS 找不到 `@sdkwork/ui-pc-react` 类型 | rsync 排除了 dist | 先 `pnpm --filter "@sdkwork/webserver-pc..." build` |
| `port is already allocated` | 端口被旧舰队占用 | 改 env 端口或先 `--down` 旧栈 |
| `network ... not found` | attach 网络名与舰队实际网络不符 | `docker network ls` 核对后更新 env 的 `SDKWORK_MODULE_API_GATEWAY_ATTACH_NETWORK` |
| webserver 起不来，挂载点变成目录 | target/release 二进制缺失，docker 建了同名目录 | `rmdir` 后按 §2.4 拷入二进制 |
| 网关容器报 `invalid reference format` | env 里 `GATEWAY_IMAGE=...:<VERSION>` 占位符未填 | 填实际镜像 tag（如 `:local`） |
| production 网关崩溃循环，日志含 `must contain sslmode=require` | P0-12 生产数据库强制 TLS | DB URL 加 `sslmode=require`（embedded postgres 已开 ssl，可直接用）；或设置 `GATEWAY_POSTGRES_SSL_MODE=require` |
| 网关报 `requires username "sdkwork_ai_prod"` | DB URL 用户名与 embedded postgres `POSTGRES_USER` 不一致 | 用户名用 `sdkwork_ai_prod`，密码取 `GATEWAY_POSTGRES_PASSWORD`（含特殊字符需 URL 编码） |
| curl 偶发 000/超时但服务正常 | 宿主代理（http_proxy=127.0.0.1:7897）劫持 | curl 加 `--noproxy '*'` |
| dev/test 数据面 443 连上即断 | 仅 production sidecar 声明 443 listener，dev/test 的 443 是死映射 | 预期行为；dev/test 走 HTTP 数据面端口 |
| TLS 客户端 ALPN 仅 h2 时 EOF | rustls 数据面不支持 h2 回退 fail-closed（已知遗留） | 客户端 ALPN 用 `h2,http/1.1`，curl 加 `--no-alpn` |
| `--down` 后仍有容器残留 | 多实例项目名动态发现 | `docker ps -a \| grep <app>-<env>-i` 逐一 `docker rm -f` |

---

## 8. 设计概要

`pnpm build:container:install` 产出**一个统一安装镜像包**（self-contained install bundle）：

- **一个镜像**：镜像本身与环境和实例数无关（environment-neutral），构建时**不**烘焙任何环境、域名、数据库或凭据信息；生命周期环境与实例数全部是**部署时输入**，由容器 entrypoint 启动时解析。
- **任意环境**：development、test、production 使用同一个镜像 tag，部署时通过 env 文件选择环境。
- **每个环境支持多实例**：N 个实例共享同一网络与同一份 secrets/data 卷；每个实例拥有独立 compose 项目名、独立节点身份和独立管理端口；仅实例 1 发布 80/443 边缘端口并先执行数据库迁移。

## 9. 与既有命令的关系

| 命令 | 用途 |
| --- | --- |
| `pnpm build:container:standalone` | 只构建统一安装镜像（不打包 bundle） |
| `pnpm build:container:install` | 构建 + 打包自包含安装 bundle |
| `pnpm deploy:apply:standalone:docker` | 仓库内运行 bundle deploy.sh |
| `scripts/docker/deploy-docker-environment.sh` | 仓库链三环境一键部署（external 布局） |
| `build:container:*` / `deploy:apply:*` | 新自动化必须使用这些入口 |

## 10. Webserver 规范合规要点（SDKWORK_WEBSERVER_SPEC.md）

| 规范点 | 实现 |
| --- | --- |
| §17 空间挂载 | 空间根 `/opt/deploy` 只读挂载；`sdkwork-space` checkout 子树为可写 overlay（entrypoint clone/pull 目标） |
| §17 模块导入 | `SDKWORK_SPACE_AUTO_DISCOVER` / `SDKWORK_SPACE_MODULES` / `MODULE_IMPORT_REQUIRED` / `PROBE_UPSTREAMS` 全部透传；模块静态资源按 `apps/*-{pc,h5}/dist/standalone/<envAlias>/` 从 checkout 解析 |
| §17.3 import 面 | `SDKWORK_WEBSERVER_IMPORT_PROFILE` 默认 `cloud`（双集合 imports.d，entrypoint 启动时物化）；`SDKWORK_WEBSERVER_IMPORT_LISTENER_PORTS` 透传（默认身份 80/443） |
| §17 多集群/多实例 | 每个容器内部统一监听网关端口 3800；宿主端口按环境/实例区分 |
| §17.4 standalone-only | 镜像/bundle 仅由 standalone 发布产物打包（`webserver-release.mjs --deployment-profile standalone`） |
| §8.1 网关 upstream | 模块 /api/ 反向代理走保留 upstream `gateway`；`SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT` 可选 `bundled`/`docker`/`external` |

验证报告实例：[docs/reports/2026-09-03-webserver-docker-packaging-verification.md](../../reports/2026-09-03-webserver-docker-packaging-verification.md)
