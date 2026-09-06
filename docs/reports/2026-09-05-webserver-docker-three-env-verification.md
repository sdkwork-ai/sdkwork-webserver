# sdkwork-webserver WSL Ubuntu 22.04 Docker 三环境部署安装更新验证报告

- 日期：2026-09-05
- 宿主：Windows + WSL `Ubuntu-22.04`（Docker 29.3.0 在 WSL 内）
- 源码：`E:\sdkwork-space\sdkwork-webserver` @ `6c362d9b`（工作区含 ~20 个未提交改动）
- 构建副本（ext4）：`~/sdkwork-build/sdkwork-webserver`
- 范围：development / test / production 三环境的 Docker 部署安装更新，并附带修复 staging

---

## 1. 结论

| 项 | 结果 |
| --- | --- |
| 三环境（开发 / 测试 / 线上）部署 | 全部 healthy，四环境（含预发）齐备 |
| 管理面（SPA + 后端 API 同源） | 4/4 环境 `200` |
| 模块导入面（域名 Host 路由） | 4/4 环境 `200` |
| 网关舰队直连 healthz | 4/4 环境 `200` |
| 网关 attach 契约（别名 + 8080） | 4/4 环境通过（**production 修复前是断的**） |
| 宿主依赖（PostgreSQL 5432 / Redis 6379） | 4/4 库可连接，Redis PONG |
| `deployment-contract.test.mjs` | 29 / 29 PASS |
| `validate-docker-deployment.mjs --matrix --compose` | 8 / 8 OK |

镜像已从当前工作区源码重建并滚动更新四环境。

---

## 2. 本次发现并修复的缺陷

### 2.1 P0：production 网关 attach 契约断裂（模块 `/api/` 反代不可用）

- 现象：`docker exec sdkwork-webserver-production getent hosts sdkwork-api-cloud-gateway` **解析失败**；
  容器内 `curl http://sdkwork-api-cloud-gateway:8080/healthz` 返回 `000`。
- 根因：`sdkwork-api-cloud-gateway` bundle 的 compose（`docker-compose.bundle.yml`）默认监听
  **3900** 且不注册别名；development / test 之前用 `/tmp/gateway-attach-override-<env>.yml`
  打过补丁，**production 没有**，且临时文件在 `/tmp` 里不可持久。

  容器侧证据（修复前）：

  | 环境 | 容器端口映射 | 网络别名 |
  | --- | --- | --- |
  | development | `0.0.0.0:3910->8080/tcp` | 含 `sdkwork-api-cloud-gateway` |
  | test | `0.0.0.0:3911->8080/tcp` | 含 `sdkwork-api-cloud-gateway` |
  | staging | `0.0.0.0:3912->8080/tcp` | 含 `sdkwork-api-cloud-gateway` |
  | **production** | **`0.0.0.0:3913->3900/tcp`** | **缺失** |

- 修复：新增持久 override 并落到源码（随包分发），不再依赖 `/tmp`：
  - `sdkwork-api-cloud-gateway/docker-compose.bundle.attach-override.yml`（新增，源码根）
  - `sdkwork-api-cloud-gateway/scripts/docker/package-install-bundle.mjs`（新增一行 copy，保证重新打包时带上）
  - 已安装 bundle：`dist/docker-install/.../compose/docker-compose.bundle.attach-override.yml`
  - 内容要点：`ports: !override ["${GATEWAY_INSTANCE_HOST_PORT}:8080"]`、容器内 bind `0.0.0.0:8080`、
    网络键 `sdkwork` 下 `aliases: [sdkwork-api-cloud-gateway]`、readyz healthcheck（`start_period: 240s`）。
- 重建方式：绕过 bundle `deploy.sh`（它会重写 env 并抹掉运维修复），直接 compose 并复刻 deploy.sh 的
  变量注入：`GATEWAY_INSTANCE_HOST_PORT`、`GATEWAY_PROFILE_ID=standalone.<env>.i1`、
  `GATEWAY_IM_ID_NODE_ID`（取 env 文件）、`GATEWAY_MIGRATE_ON_START`、
  `POSTGRES_ASSETS_DIR=<bundle>/postgres`、`--profile instance`。
  development / test 保留内置 postgres/redis（追加 `-f docker-compose.bundle.embedded.yml`），
  production 维持外部宿主依赖，不改动各环境既有的依赖模式。

### 2.2 环境文件缺口（bundle 安装路径会踩到）

`deployments/docker/env/*.env` 相对各自的 `.example` 缺了 5 个键。仓库链的 compose 会用
`WEBSERVER_POSTGRES_HOST` / `WEBSERVER_REDIS_HOST` 推导，所以仓库链看不出问题；但 bundle 链的
`docker-compose.bundle.yml` 直接取 `SDKWORK_DATABASE_HOST`（缺省回落到 `postgres`），缺键会连错库。

已补齐到 development / test / production：

```dotenv
SDKWORK_DATABASE_HOST=host.docker.internal
SDKWORK_DATABASE_PORT=5432
SDKWORK_WEBSERVER_REDIS_HOST=host.docker.internal
SDKWORK_WEBSERVER_REDIS_PORT=6379
PG_MAX_CONNECTIONS=200
```

### 2.3 版本号与产物不一致

`sdkwork.app.config.json` 的 `release.currentVersion` 是 `0.1.0`，但已构建产物、安装 bundle、
四份 env 的 `SDKWORK_WEBSERVER_IMAGE_TAG` 全是 `0.1.1`。重新打包会回落成 `0.1.0`，制造漂移。

已把 `currentVersion` / `latest.BETA` 提到 `0.1.1`，并补一条 0.1.1 release note（原 0.1.0 条目
保留并置 `current: false`）。

### 2.4 staging 遗留

- `sdkwork-webserver-staging` 停在旧镜像 `0.1.0`、`Exited (143)`；
- 并行会话留下的 `sdkwork-webserver-staging-platform-api-gateway-1` 与
  `-knowledgebase-rpc-1` 在崩溃循环（缺 `SDKWORK_CLOUDROUTER_API_KEY_PEPPER`）。

处置（经确认后执行）：`docker rm -f` 移除两个崩溃容器与旧 staging 容器；staging.env 补齐到与
test.env 同一配置面（attach 网络/主机/非必需、DB 与 Redis 键、ACME 与 TLS 快照键、deploy 身份键），
镜像标签 `0.1.0 → 0.1.1`；用规范的 attach overlay
（`docker-compose.staging.yml` + `docker-compose.platform-api-gateway-attach.yml`）重新拉起。
原 `sdkwork-webserver-staging_platform-api-gateway-{data,secrets}` 两个卷**保留未删**，可回滚。

---

## 3. 镜像重建（从当前工作区源码）

链路（全部在 ext4 副本执行，`/mnt/e` 上 pnpm 会因跨设备硬链接失败）：

```bash
rsync -a --exclude node_modules --exclude target --exclude dist ... \
  /mnt/e/sdkwork-space/sdkwork-webserver/ ~/sdkwork-build/sdkwork-webserver/
cd ~/sdkwork-build/sdkwork-webserver
pnpm install                       # 工作区新增 @monaco-editor/react / monaco-editor
pnpm rebuild esbuild               # 补 native postinstall（pnpm 默认忽略构建脚本）
node scripts/webserver-release.mjs package --deployment-profile standalone
node scripts/docker/build-standalone-image.mjs --skip-platform-gateway --no-pull
```

- 首次失败点：PC 应用 typecheck 报 `Cannot find module '@monaco-editor/react' / 'monaco-editor'`
  —— 工作区 `package.json` 新增了依赖，但 ext4 副本的 `node_modules` 还是旧的。`pnpm install` 后通过
  （cargo release 已 warm，2m46s 完成，未重复编译）。
- `--skip-platform-gateway` 与现网镜像形态一致（镜像内是 `.bundled-gateway-omitted`；网关以外部
  舰队 attach，`SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=docker`）。
- `--no-pull` 复用已缓存的 `debian:bookworm-slim` 基础镜像（WSL 内 daemon 无代理）。

产物指纹：

| 产物 | 值 |
| --- | --- |
| 镜像 | `registry.sdkwork.com/apps/sdkwork-webserver-standalone:0.1.1` |
| 镜像 ID | `sha256:601ed4e0cda48b0a25b6847f5f9bef7b9dd75e4cdb6a0cf9f53029dfa50a4803`（旧：`cbeb4b4d1a30`） |
| 镜像大小 | 422,964,517 B（≈403 MiB） |
| 发布包 | `sdkwork-webserver-linux-x64-standalone-server-0.1.1.tar.gz`（71,943,432 B） |
| 发布包 sha256 | `364d9dc5286a666030d735c49618c9e3039320dd9e3cc5055e08f9e2a7354e63` |
| SBOM | 同名 `.cdx.json` + `.sha256`（已回写到 `dist/release/`） |

滚动更新：`deploy-docker-environment.sh all --validate` + `deploy-docker-environment.sh staging --validate`，
四环境 `Recreate → Started → healthy`。旧镜像 `0.1.1 (cbeb4b4d1a30)` 与 `0.1.0` 保留，可秒级回滚
（改 env 的 `SDKWORK_WEBSERVER_IMAGE_TAG` 后重跑 apply，或 `docker tag` 指回旧 ID）。

---

## 4. 访问入口（验证通过）

| 环境 | 管理面（推荐，SPA + API 同源） | 数据面（需 Host 头） | 网关直连 |
| --- | --- | --- | --- |
| 开发 | http://127.0.0.1:13800 | http://127.0.0.1:80 （Host: `server-dev.sdkwork.com`） | http://127.0.0.1:3910 |
| 测试 | http://127.0.0.1:18888 | http://127.0.0.1:18898 （Host: `server-test.sdkwork.com`） | http://127.0.0.1:3911 |
| 预发 | http://127.0.0.1:18081 | http://127.0.0.1:18099 （Host: `server-staging.sdkwork.com`） | http://127.0.0.1:3912 |
| 线上 | http://127.0.0.1:18080 | http://127.0.0.1:18098 （Host: `server.sdkwork.com`） | http://127.0.0.1:3913 |

HTTPS 边缘（本次唯一未打通的面）：

| 环境 | 端口 | 结果 |
| --- | --- | --- |
| 开发 | 443 | `000`（无监听） |
| 测试 | 28430 | `000`（无监听） |
| 预发 | 38431 | `000`（无监听） |
| 线上 | 38430 | `200`（`server.sdkwork.com` / `im.sdkwork.com`） |

---

## 5. 验证矩阵（实测输出）

```text
1. containers
   sdkwork-webserver-{development,test,staging,production}   Up (healthy)   ...webserver-standalone:0.1.1
   sdkwork-api-cloud-gateway-{development,test,staging,production}-i1-gateway-1   Up (healthy)

2. management plane
   development :13800 healthz=200  /=200  /backend/v3/api/servers=401  /openapi.json=200
   test        :18888 healthz=200  /=200  /backend/v3/api/servers=401  /openapi.json=200
   staging     :18081 healthz=200  /=200  /backend/v3/api/servers=401  /openapi.json=200
   production  :18080 healthz=200  /=200  /backend/v3/api/servers=401  /openapi.json=200

3. module import plane (Host-routed)
   development :80     console=200  runtime-env.json=200  api-edge(openapi)=200
   test        :18898  console=200  runtime-env.json=200  api-edge(openapi)=200
   staging     :18099  console=200  runtime-env.json=200  api-edge(openapi)=200
   production  :18098  console=200  runtime-env.json=200  api-edge(openapi)=200

4. gateway fleet healthz        3910/3911/3912/3913 -> 200/200/200/200

5. attach contract (probed from inside each webserver container)
   development alias=172.18.0.5  bind=8080/tcp  gateway:8080/healthz=421
   test        alias=172.19.0.5  bind=8080/tcp  gateway:8080/healthz=421
   staging     alias=172.26.0.7  bind=8080/tcp  gateway:8080/healthz=421
   production  alias=172.21.0.5  bind=8080/tcp  gateway:8080/healthz=421

6. TLS edge (SNI pinned)        production https://server.sdkwork.com:38430/ -> 200

7. host deps                    redis 6379 PONG；四库 psql select 1 -> 1
```

说明：

- `/backend/v3/api/servers=401` 是**期望值**——路由存在、鉴权拦截未带凭据的请求。
- attach 探测的 `421` 也是**期望值**：探测用的 Host 是别名 `sdkwork-api-cloud-gateway`，
  而网关 Host 白名单是精确匹配。这恰好证明连通性已通（网关有应答）；sidecar 转发时保留原始
  Host（如 `api-dev.sdkwork.com`），不会触发 421。

---

## 6. 已知边界与后续项

1. **dev / test / staging 无 443 监听**（P1，本次按约定仅记录）
   非生产代次的模块 sidecar 不声明 `listen 443 ssl`，数据面只物化 1 个 80 监听器、证书数 0；
   production 是 2 个监听器、994 证书。compose 里的 443 / 28430 / 38431 映射是死映射。
   影响：`https://api-dev.sdkwork.com` 这类 cloud API 边缘不可达（HTTP 的 `api-dev` 边缘正常，
   见矩阵第 3 行）。修复需改 `sdkwork-specs` 非生产 sidecar 模板并重生成 55+ 模块 sidecar。
   另注：dev/test 只有 71 个 vhost，production 有 994 个——同源，属 sidecar 生成差异。
2. **production HTTPS 必须带 SNI**：`curl https://127.0.0.1:38430/` 返回 `000`（rustls 无 SNI fail-closed）；
   带 SNI（`--resolve <domain>:38430:127.0.0.1`）即 200。浏览器天然带 SNI。
3. **依赖模式不统一**：development / test 的网关用**内置** postgres/redis 容器，
   production / staging 用**宿主原生** 5432 / 6379。本次刻意保留现状（避免搬动 dev/test 数据），
   但 `DEPLOYMENT_SPEC §6.1` 的宿主系统外部依赖标准指向全外部化，建议后续统一
   （四库在宿主原生实例上均已就绪、各 981 张表、凭据与 env 文件一致）。
4. **staging 平台卷残留**：`sdkwork-webserver-staging_platform-api-gateway-{data,secrets}` 保留未删，
   确认不再需要后可 `docker volume rm`。
5. **未提交改动**：本次镜像包含工作区约 20 个未提交改动；跨仓库改动
   （`sdkwork-api-cloud-gateway` 新增 override + 打包脚本一行、webserver 的 env / 版本号 / 文档）
   同样未提交，需各自提交。

---

## 7. 文档同步

- `docs/guides/operator/docker-install.md` / `docker-install.en.md`
  - 新增 **§3.3**（网关舰队侧持久 attach override 与重建命令，含三件自检）；
  - 新增 **§5.4 已知边界**（4 条实测结论）；
  - 访问入口矩阵补 staging 行。
- `sdkwork-api-cloud-gateway`：`docker-compose.bundle.attach-override.yml`（新增），
  `scripts/docker/package-install-bundle.mjs`（随包分发该 override）。
