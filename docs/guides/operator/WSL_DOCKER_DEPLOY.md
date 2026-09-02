# WSL Ubuntu Docker 部署（sdkwork-webserver）

本文档描述在 WSL Ubuntu 22.04 中用 Docker 部署 `sdkwork-webserver` 独立网关，
对齐 `sdkwork-api-cloud-gateway` 的双模式依赖模型。

## 域名约定

| 环境 | Docker 域名前缀 | nginx `:80` 路由 |
| --- | --- | --- |
| development | `server-dev.sdkwork.com` | `127.0.0.1:13800` |
| test | `server-test.sdkwork.com` | `127.0.0.1:18888` |
| production | `server.sdkwork.com` | `127.0.0.1:18080` |

支持后缀：仅 `sdkwork.com`

## 依赖模式

| 模式 | 命令 | PostgreSQL | Redis |
| --- | --- | --- | --- |
| 内置（默认） | `bash scripts/docker/deploy-docker-environment.sh development` | compose `postgres:16-alpine` | compose `redis:8-alpine` |
| 外部 | 同上 + `--external` | `WEBSERVER_POSTGRES_HOST` | `WEBSERVER_REDIS_HOST` |
| 三环境共享内置库 | `bash scripts/docker/deploy-docker-environment.sh all --embedded-shared` | 单个 postgres 服务 | 单个 redis 服务 |

## 前置要求

- WSL2 Ubuntu 22.04，Docker Engine 已安装并运行
- 已构建 standalone 镜像：`pnpm build:container:standalone`
- （可选）WSL nginx：用于 `:80` 域名访问

## 部署步骤

### 1. 准备环境文件

```bash
# In WSL Ubuntu (repo root; adjust drive letter if the workspace is not on E:)
cd /mnt/<drive>/sdkwork-space/sdkwork-webserver
# example: cd /mnt/e/sdkwork-space/sdkwork-webserver
cp deployments/docker/env/development.env.example deployments/docker/env/development.env
# 编辑 WEBSERVER_POSTGRES_PASSWORD、WEBSERVER_POSTGRES_DEV_PASSWORD 等
```

### 2. 内置模式（development）

```bash
bash scripts/docker/deploy-docker-environment.sh development --validate
docker compose -p sdkwork-webserver-development ps
curl http://127.0.0.1:13800/healthz
```

### 3. 外部模式（WSL 宿主 PostgreSQL/Redis）

宿主侧先执行 schema 脚本：

```bash
psql -h 127.0.0.1 -U postgres -d sdkwork_ai_dev \
  -v db=sdkwork_ai_dev -v app_user=sdkwork_ai_dev \
  -f deployments/docker/postgres/external-schema.sql
```

编辑 `deployments/docker/env/development.env`：

```dotenv
WEBSERVER_POSTGRES_HOST=host.docker.internal
WEBSERVER_REDIS_HOST=host.docker.internal
WEBSERVER_POSTGRES_DEV_PASSWORD=<实际密码>
```

启动：

```bash
bash scripts/docker/deploy-docker-environment.sh development --external --validate
```

### 4. 三环境并行（共享内置 postgres/redis）

```bash
cp deployments/docker/env/test.env.example deployments/docker/env/test.env
cp deployments/docker/env/production.env.example deployments/docker/env/production.env
bash scripts/docker/deploy-docker-environment.sh all --embedded-shared --validate
```

### 5. nginx + hosts（`:80` 域名访问）

```bash
sudo bash deployments/docker/scripts/install-wsl-nginx.sh
sudo bash deployments/docker/scripts/install-wsl-hosts.sh
curl http://server-dev.sdkwork.com/healthz
curl http://server-test.sdkwork.com/healthz
curl http://server.sdkwork.com/healthz
```

声明式 Web 配置位于 `deployments/webserver/`；`install-wsl-nginx.sh` 会将 nginx
站点安装到 `/etc/nginx/sites-enabled/sdkwork/<domain>.conf`。

Windows 浏览器访问时，还需在 `C:\Windows\System32\drivers\etc\hosts` 追加相同域名。

## 验证

```bash
pnpm check:container-deployment
pnpm test:container-deployment
node scripts/docker/validate-docker-deployment.mjs --matrix
```

## 运维

```bash
# 停止单环境栈
bash scripts/docker/deploy-docker-environment.sh development --down

# 停止外部模式栈
bash scripts/docker/deploy-docker-environment.sh test --external --down

# 停止共享栈
bash scripts/docker/deploy-docker-environment.sh all --embedded-shared --down
```

Authority: `deployments/docker/README.md`, `sdkwork-api-cloud-gateway/docs/guides/operator/WSL_EXTERNAL_DEPLOY.md`
