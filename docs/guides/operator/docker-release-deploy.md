# SDKWork Web Server —— Docker 部署指南（基于 Release 版本）

面向**从 GitHub Release 直接部署**的场景：无需克隆仓库、无需 `bin/` 单操作员通道，
下载指定 release 版本的产物即可运行。

> 仓库内开发/运维通道（`bin/docker-deploy.sh`、`bundle` 安装）见
> [`docs/guides/operator/docker-install.md`](docker-install.md)。本文只覆盖 release 产物部署。

---

## 0. TL;DR

```bash
# 1. 选版本（默认最新正式版）
VERSION=0.1.9

# 2. 下载并解压
curl -fsSL -o sdkwork.tar.gz \
  "https://github.com/sdkwork-ai/sdkwork-webserver/releases/download/v${VERSION}/sdkwork-webserver-linux-x64-standalone-server-${VERSION}.tar.gz"
mkdir -p /opt/sdkwork && tar -xzf sdkwork.tar.gz -C /opt/sdkwork --strip-components=1

# 3. 写配置（端口 / 数据库 / Redis 全部可改，见 §2）
#    归档自带完整样例，改端口/库地址时从它复制最省事
cp /opt/sdkwork/etc/examples/docker-release.config.toml \
   /opt/sdkwork/etc/webserver/config.toml
vi /opt/sdkwork/etc/webserver/config.toml

# 4. 构建镜像（Dockerfile.release 与 docker/ 入口都在归档里）
cd /opt/sdkwork && docker build -f Dockerfile.release -t sdkwork-webserver:${VERSION} .

# 5. 运行
docker run -d --name sdkwork-webserver \
  -p 8080:8080 \
  -v /opt/sdkwork/etc/webserver:/etc/sdkwork/webserver:ro \
  sdkwork-webserver:${VERSION}
```

验证：

```bash
curl -fsS http://127.0.0.1:8080/healthz
```

---

## 1. 获取 release 产物

每个 release 为每个平台包提供 4 类附件（实测命名，源自 `sdkwork.workflow.json` 的 `outputGlobs`）：

| 附件后缀 | 用途 |
| --- | --- |
| `.tar.gz` | 通用压缩包，**Docker 部署用它** |
| `.tar.gz.sha256` | 校验和 |
| `.tar.gz.sigstore.json` | Sigstore 签名（验证发布者身份与完整性） |
| `.tar.gz.cdx.json` | CycloneDX SBOM（软件物料清单） |

平台包命名：

| 平台包 | 适用 |
| --- | --- |
| `sdkwork-webserver-linux-x64-standalone-server-<ver>.tar.gz` | x86_64 Linux，通用（推荐） |
| `sdkwork-webserver-linux-arm64-standalone-server-<ver>.tar.gz` | ARM64（Graviton / Ampere） |
| `linux-ubuntu-x64-standalone-server-<ver>.deb` | Ubuntu / Debian 主机 |
| `linux-rhel-x64-standalone-server-<ver>.rpm` | RHEL / Rocky / AlmaLinux |

> 矩阵里另有 `*-test-*` 变体（`linux-ubuntu-x64-standalone-server-test-deb`、
> `linux-rhel-x64-standalone-server-test-rpm`），那是**测试环境的预发布件**，
> 生产部署请用非 test 的那一份。

下载：

```bash
gh release download "v${VERSION}" \
  --repo sdkwork-ai/sdkwork-webserver \
  --pattern "sdkwork-webserver-linux-x64-standalone-server-*.tar.gz" \
  --dir /tmp/sdkwork-release
```

校验（**先验后解**）：

```bash
cd /tmp/sdkwork-release
sha256sum --check "sdkwork-webserver-linux-x64-standalone-server-${VERSION}.tar.gz.sha256"
```

解压后的目录布局（顶层目录为 `sdkwork-webserver/`，故用 `--strip-components=1`）：

```
/opt/sdkwork/
├── bin/                       # 5 个服务二进制
│   ├── sdkwork-api-webserver-standalone-gateway
│   ├── sdkwork-webserver-website-delivery-edge-runtime
│   ├── sdkwork-webserver-node-daemon
│   ├── sdkwork-webserver-agent
│   └── sdkwork-webserver-certificate-worker
├── share/sdkwork/             # 各模块运行资源（iam / drive / deploy / mcp …）
│   ├── webserver/web/pc       # PC 静态产物
│   ├── webserver/web/h5       # H5 静态产物
│   └── <module>/database/database.manifest.json
├── etc/                       # 配置模板与清单（不含任何真实凭据）
│   ├── examples/docker-release.config.toml   # 本部署方式的完整配置样例
│   └── examples/docker-release-deploy.md     # 本文档副本
├── specs/                     # app 清单与配置 JSON Schema
├── Dockerfile.release         # 容器镜像定义
├── docker-compose.release.yml # 编排（含自带 PG/Redis 的选项）
├── docker/entrypoint-release.sh # 容器入口
└── sdkwork.app.config.json
```

> **凭据不入包**：`etc/` 下只有模板与 schema，真实密码一律通过运行时挂载的
> `secrets/*.secret` 注入（见 §2.3）。
>
> **容器部署件随包发布**：release 归档**故意不带**仓库内的
> `bin/container/entrypoint-standalone.sh`（它只在源码树里），所以归档自带一套
> release 专用容器件：`Dockerfile.release`、`docker/entrypoint-release.sh`、
> `docker-compose.release.yml`。因此下面第 4 步的 `docker build` 直接在解压目录里
> 执行即可，不需要额外下载或克隆仓库。

配套资产（本文与下列文件是一套，**已包含在 release 归档内**）：

```
Dockerfile.release             # 镜像定义
docker/entrypoint-release.sh   # 容器入口
docker-compose.release.yml     # 编排（含自带 PG/Redis 的选项）
etc/examples/docker-release.config.toml  # 配置样例
```


---

## 2. 配置

配置优先级：**显式挂载的 `config.toml` > 镜像内置默认值**。

### 2.1 目录准备

```bash
sudo mkdir -p /opt/sdkwork/etc/webserver/secrets
sudo chmod 0750 /opt/sdkwork/etc/webserver
sudo chmod 0700 /opt/sdkwork/etc/webserver/secrets
```

镜像内以非特权用户 `sdkwork`(uid 10001) 运行，因此宿主机上的配置与密钥文件
必须对该 uid 可读：

```bash
sudo chown -R 10001:10001 /opt/sdkwork/etc/webserver
sudo chmod 0640 /opt/sdkwork/etc/webserver/config.toml
```

### 2.2 自定义端口

控制面监听端口由 `[ingress].bind` 决定。**同时**要把 `ports:` 映射改掉，否则外部访问不到：

```toml
[profile]
deployment_profile = "standalone"
environment = "production"
profile_id = "standalone.production"
node_id = 0

[ingress]
# 容器内监听地址与端口。0.0.0.0 才能从容器外访问。
bind = "0.0.0.0:8080"
# 运维/健康检查 socket，只走回环，不要暴露。
data_plane_operations_bind = "127.0.0.1:9091"
# 对外 URL：CORS 白名单与生成链接都以此为准。
public_http_url = "http://webserver.example.com"
cors_allowed_origins = ["http://webserver.example.com", "http://127.0.0.1:8080"]
management_expose_allowed = true
```

对应 compose：

```yaml
ports:
  - "8080:8080"      # 左：宿主端口（可任意改），右：与 bind 保持一致
```

> `data_plane_operations_bind` 只监听回环，供宿主健康检查与指标采集使用，**不要**映射到宿主。

### 2.3 自定义数据库

`[database]` 段支持全部常用连接参数：

```toml
[database]
engine = "postgresql"
host = "db.example.com"
port = 5432
name = "sdkwork_ai_prod"
schema = "sdkwork_ai_prod"
# schema 不存在时是否回退到 public；建议保持 false 以免污染 public。
schema_fallback_public = false
username = "sdkwork_app"
# 密码只从文件读，不写进配置本身。
password_file = "/etc/sdkwork/webserver/secrets/database.secret"
# disable / require / verify-ca / verify-full
ssl_mode = "require"
max_connections = 20
# 首次启动自动建表；如由外部迁移流水线管理，设为 false。
auto_migrate = true
```

密码文件（**权限必须是 0600**）：

```bash
sudo install -m 0600 -o 10001 -g 10001 /dev/null \
  /opt/sdkwork/etc/webserver/secrets/database.secret
echo -n '实际密码' | sudo tee /opt/sdkwork/etc/webserver/secrets/database.secret >/dev/null
```

> **凭据不入包、不入配置**：配置文件里只有文件路径。镜像内以非特权用户 `sdkwork`(uid 10001) 运行，
> 该文件需对该用户可读。

### 2.4 自定义 Redis

解析链缓存为四层：`file → memory → redis → database`。
**Redis 是可选层**。

```toml
[resolution_cache]
enabled = true
# 本地 hosts 风格种子文件，启动时载入内存层。
file = "/etc/sdkwork/webserver/resolver.seed"
memory = true
memoryMaxEntries = 65536
memoryTtlSeconds = 60
database = true
# 解析失败的负缓存 TTL（秒），用于吸收上游 DNS 抖动。
negativeTtlSeconds = 10

[resolution_cache.redis]
# redis://host:port；rediss:// 走 TLS。含密码时 redis://:密码@host:port
url = "rediss://cache.example.com:6379"
# 记录 TTL（秒）
ttlSeconds = 300
# 集群级键前缀，多环境共用一套 Redis 时用它隔离。
prefix = "sdkwork:resolver"
# 连接超时（毫秒）
connectTimeoutMs = 1000
# 单次操作超时（毫秒）。Redis 卡住时降级到上层，不会拖住解析。
operationTimeoutMs = 250
```

`[resolution_cache.redis]` 的**字段与默认值**（取自 `sdkwork-webserver-resolver-cache/src/config.rs`）：

| 字段 | 类型 | 默认值 |
| --- | --- | --- |
| `url` | string | **必填**，无默认 |
| `ttlSeconds` | u64 | `300` |
| `prefix` | string | `sdkwork:resolver` |
| `connectTimeoutMs` | u64 | `1000` |
| `operationTimeoutMs` | u64 | `250` |

外层 `[resolution_cache]` 字段：

| 字段 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | 总开关，关掉则直连系统解析器 |
| `file` | string | 无 | 种子文件路径 |
| `memory` | bool | `true` | 内存层 |
| `memoryMaxEntries` | usize | `65536` | 内存层 LRU 上限 |
| `memoryTtlSeconds` | u64 | `60` | 内存层 TTL |
| `database` | bool | `false` | 数据库层（读解析记录） |
| `negativeTtlSeconds` | u64 | `10` | 负缓存 TTL |

> **命名规则**：这两个结构体是 `#[serde(rename_all = "camelCase")]`，所以 TOML 里必须写
> `memoryMaxEntries`、`ttlSeconds` 这类**驼峰**键；写成 `memory_max_entries` 不会被识别。
>
> **生效范围**：`resolution_cache` 属于数据面解析链，由 `serve-imports` / `data-plane` 读取
> （`data_plane/runtime.rs` 的 `build_resolution_chain`）；管理面 `serve-management` 不消费该段，
> 在管理面启动路径上改这一段不会有任何效果，也不会报错。
> Redis 连接是**懒加载**的，Redis 不可用时降级到上层，不会阻塞启动，
> `operationTimeoutMs` 同时兜底单次往返。

环境变量可覆盖 Redis URL（便于 compose 内统一注入，不必改配置文件）：

```bash
SDKWORK_WEBSERVER_REDIS_URL=redis://cache.example.com:6379
```

> **容器内的优先级**：`docker/entrypoint-release.sh` 只在 `SDKWORK_WEBSERVER_REDIS_URL`
> **未被显式设置**时，才用 `${CONFIG_ROOT}/secrets/redis.secret` 的内容拼出 URL
> （主机/端口取 `SDKWORK_WEBSERVER_REDIS_HOST` / `_PORT`，默认 `cache:6379`）。
> secret 文件缺失或为空时**不报错**，只打一条 WARNING 并降级到内存层——
> 解析缓存是可选层，缺 Redis 不该阻断启动。
> 显式设置的 URL 优先，因此外部托管 Redis 时直接设该变量即可。

---

## 3. 启动

### 3.1 单容器

```bash
docker run -d \
  --name sdkwork-webserver \
  --restart unless-stopped \
  -p 8080:8080 \
  -v /opt/sdkwork/etc/webserver:/etc/sdkwork/webserver:ro \
  -v /opt/sdkwork/etc/webserver/secrets:/etc/sdkwork/webserver/secrets:ro \
  sdkwork-webserver:0.1.9
```

> `secrets` 单独只读挂载，避免把配置目录整体可写。

### 3.2 自带 PostgreSQL / Redis（compose）

```bash
docker compose -f docker-compose.release.yml up -d
```

编排文件自带 `postgres:16` 与 `redis:7` 两个服务。**密码不进 `.env`**，而是和
`config.toml` 的 `password_file` 读同一个文件，因此角色与应用不会各自拿一份密码而连不上：

```bash
sudo install -d -m 0700 -o 10001 -g 10001 /opt/sdkwork/etc/webserver/secrets
echo -n '换成一个强数据库密码' | sudo tee \
  /opt/sdkwork/etc/webserver/secrets/database.secret >/dev/null
echo -n '换成一个强 Redis 密码' | sudo tee \
  /opt/sdkwork/etc/webserver/secrets/redis.secret >/dev/null
sudo chmod 0600 /opt/sdkwork/etc/webserver/secrets/*.secret
```

| 凭据 | 文件 | 消费方 |
| --- | --- | --- |
| 数据库密码 | `secrets/database.secret` | `config.toml` 的 `password_file` + compose `POSTGRES_PASSWORD_FILE` |
| Redis 密码 | `secrets/redis.secret` | compose `redis-server --requirepass` + entrypoint 拼出的 `SDKWORK_WEBSERVER_REDIS_URL` |

> `database.secret` 与 `redis.secret` 都由 compose 的 `secrets:` 段挂载。
> 轮换密码只需改这一个文件，两侧同时生效，值也不会出现在 `docker inspect` 或 shell 历史里。

改端口 / 数据库名 / 数据库账号时**只改 `.env`**（密码除外，它在 secret 文件里）：

```bash
cat > .env <<'ENV'
SDKWORK_VERSION=0.1.9
# 自定义宿主端口（左）与容器内端口（右，必须与 config.toml 的 bind 一致）
HOST_HTTP_PORT=8080
CONTAINER_HTTP_PORT=8080
# 自定义数据库名 / 账号（须与 config.toml 的 [database].name / .username 一致）
POSTGRES_DB=sdkwork_ai_prod
POSTGRES_USER=sdkwork_app
# 配置文件所在目录（默认 /opt/sdkwork/etc/webserver）
SDKWORK_CONFIG_DIR=/opt/sdkwork/etc/webserver
ENV

docker compose -f docker-compose.release.yml --env-file .env up -d
```

> **`.env` 里的 `POSTGRES_DB` / `POSTGRES_USER` 只决定数据库角色侧建什么**，
> 应用侧读的是 `config.toml` 的 `[database].name` / `.username`——两者必须改成同一个值。
> 这不是重复，是两个必须对齐的独立配置面：`config.toml` 在启动时被物化成
> `SDKWORK_DATABASE_*` 覆盖环境变量，所以**在 compose 里注入 `SDKWORK_DATABASE_*` 是无效旋钮**，
> 本编排刻意不提供。

### 3.3 连接外部数据库 / Redis

不用 compose 内置服务，只挂载配置：

```bash
docker compose -f docker-compose.release.yml --env-file .env up -d webserver
```

此时 `config.toml` 里的 `[database].host` 与 `[resolution_cache.redis].url`
应指向外部实例（容器内不能用 `localhost`，用服务名或宿主机网关名）。

---

## 4. 验证

```bash
# 健康检查
curl -fsS http://127.0.0.1:${HOST_HTTP_PORT}/healthz

# 日志：确认数据库连接、迁移结果、Redis 降级与否
docker compose -f docker-compose.release.yml logs -f webserver

# 自定义端口真的生效（容器内监听端口应与 bind 一致）
docker compose -f docker-compose.release.yml exec webserver \
  sh -c 'ss -lntp 2>/dev/null || netstat -lntp'
```

配置写错时，**该段所属的启动路径会明确失败**，例如 app config 物化阶段：

```
config materialization failed: unknown or unsupported key `xxx`
```

> 注意作用域：`[ingress]` / `[database]` 由管理面读取，写错通常伴随启动即失败或连不上库；
> 而 `[resolution_cache]` 只被数据面读取，在管理面路径上写错**不会报错也不会生效**，
> 需要用 `serve-imports` 验证（见 §4）。

### 4.1 验证解析缓存（数据面）

`[resolution_cache]` 由数据面进程消费，管理面启动成功**不代表**它生效。单独验证：

```bash
# 以数据面子命令启动，日志中会打印解析链构建结果与 Redis 降级情况
docker compose -f docker-compose.release.yml exec webserver \
  /app/bin/sdkwork-api-webserver-standalone-gateway serve-imports
```

Redis 连不上时应看到**降级告警而非崩溃**——这是设计行为（懒加载 + 分级超时）。

---

## 5. 运维

### 升级

```bash
VERSION=0.1.10
curl -fsSL -o sdkwork.tar.gz \
  "https://github.com/sdkwork-ai/sdkwork-webserver/releases/download/v${VERSION}/sdkwork-webserver-linux-x64-standalone-server-${VERSION}.tar.gz"
mkdir -p /opt/sdkwork && tar -xzf sdkwork.tar.gz -C /opt/sdkwork --strip-components=1
docker compose -f docker-compose.release.yml --env-file .env build --no-cache
docker compose -f docker-compose.release.yml --env-file .env up -d
```

`/opt/sdkwork/etc/` 是配置与凭据目录，**每次解压不会覆盖它**；
如需回滚，改 `.env` 里的 `SDKWORK_VERSION` 后重新 `up -d` 即可。

### 备份

需要备份的只有两样：

```bash
# 数据库
docker compose -f docker-compose.release.yml exec -T db \
  pg_dump -U sdkwork_app sdkwork_ai_prod | gzip > backup-$(date +%F).sql.gz

# 凭据（不进版本库）
tar -czf secrets-$(date +%F).tar.gz /opt/sdkwork/etc/webserver/secrets
```

### 常见问题

| 现象 | 原因与处理 |
| --- | --- |
| `unknown or unsupported key` | 配置键名拼错；键名区分大小写且不允许未知键 |
| `Pool creation error: pool timed out` | 数据库地址/端口/账号不可达；容器内 `localhost` 指向容器自身 |
| 端口能连但 404 | 宿主端口与 `config.toml` 的 `bind` 不一致（`ports` 右侧须与 `bind` 相同） |
| Redis 不生效 | `[resolution_cache]` 只在数据面生效；`serve-management` 不消费该段 |
| 权限 denied | `secrets/*.secret` 需 `0600` 且属主为 uid 10001 |

---

## 6. 规范依据

| 条款 | 要求 |
| --- | --- |
| `PACKAGING_SPEC.md` §5.4 | Web/静态包只含 `dist/` 产物；source map 仅可作为已声明的 opt-in 产物出货 |
| `PACKAGING_SPEC.md` §5.5 | 安装脚本只配置运行时布局并注册服务 |
| `RUNTIME_DIRECTORY_SPEC.md` §4 | 宿主机目录映射的权威来源 |
| `ENVIRONMENT_SPEC.md` §7.3 | 单应用主机允许凭据落在 `/etc/sdkwork/webserver/secrets/` |
| `DEPLOYMENT_SPEC.md` §6.1 | 外部 PostgreSQL/Redis 为默认依赖模式，嵌入式为显式 opt-in |
