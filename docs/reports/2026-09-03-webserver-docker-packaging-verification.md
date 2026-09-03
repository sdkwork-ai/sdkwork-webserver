# sdkwork-webserver Docker 三环境打包部署验证报告

- 日期：2026-09-03
- 范围：`E:\sdkwork-space\sdkwork-webserver`（WSL Ubuntu-22.04，/mnt/e 挂载）
- 结论：**通过**。同一镜像/同一安装包已部署 development / test / production 三个环境，全部容器 healthy，HTTP 层（healthz + 域名 Host 头 SPA）逐项 200。

## 1. 产物

| 产物 | 位置 | 大小 |
| --- | --- | --- |
| standalone 安装包 | `~/sdkwork-build/sdkwork-webserver/dist/release/sdkwork-webserver-linux-x64-standalone-server-0.1.0.tar.gz`（ext4 构建副本） | 61,902,608 B，440 entries，SBOM 567 components |
| Docker 镜像 | `registry.sdkwork.com/apps/sdkwork-webserver-standalone:0.1.0`（三环境共用同一镜像） | — |
| webserver 二进制（宿主 bind-mount 混合模式） | `/mnt/e/sdkwork-space/sdkwork-webserver/target/release/sdkwork-api-webserver-standalone-gateway` | 84,295,536 B |

构建链（全部在 WSL ext4 副本 `~/sdkwork-build/` 执行，DrvFS 上 pnpm 会 `disk I/O error`）：

```
node scripts/webserver-release.mjs package --deployment-profile standalone
node scripts/docker/build-standalone-image.mjs --skip-platform-gateway
bash scripts/docker/deploy-docker-environment.sh all --validate   # DEPLOY_EXIT=0
```

## 2. 环境矩阵

| 环境 | 容器 | 管理面 (3800→host) | 数据面 HTTP | 数据面 HTTPS | 域名（Host 头） |
| --- | --- | --- | --- | --- | --- |
| development | `sdkwork-webserver-development` (healthy) | 13800 | 80 | 443 | `server-dev.sdkwork.com` |
| test | `sdkwork-webserver-test` (healthy) | 18888 | 18898 | 28430 | `server-test.sdkwork.com` |
| production | `sdkwork-webserver-production` (healthy) | 18080 | 18098 | 38430 | `server.sdkwork.com` |

数据库：统一 `host.docker.internal:15432` → 库 `sdkwork_ai_dev` / `sdkwork_ai_test` / `sdkwork_ai_prod`；Redis `host.docker.internal:6379`。
网关：attach 模式（`SDKWORK_WEBSERVER_SPEC` §17.3），sidecar 字面直连 `sdkwork-api-cloud-gateway:8080`；网关容器来自 `sdkwork-api-cloud-gateway` 0.1.1 bundle（项目 `sdkwork-api-cloud-gateway-<env>-i1`，网络 `sdkwork-api-cloud-gateway-<env>`），同一 `sdkwork-api-cloud-gateway:local` 镜像跨三环境。

## 3. 验证结果（2026-09-03 10:48 +08:00）

### 3.1 管理面 healthz

| 检查项 | 结果 |
| --- | --- |
| `http://127.0.0.1:13800/healthz` (dev) | 200 `{"status":"ok"}` |
| `http://127.0.0.1:18888/healthz` (test) | 200 `{"status":"ok"}` |
| `http://127.0.0.1:18080/healthz` (prod) | 200 `{"status":"ok"}` |

### 3.2 数据面 SPA（域名 Host 头 + 同源管理端口）

| 检查项 | 结果 |
| --- | --- |
| `http://127.0.0.1/` + Host `server-dev.sdkwork.com` | 200 SPA HTML (`SDKWork Web Server`, zh-CN, 含 bootstrap token) |
| `http://127.0.0.1:18898/` + Host `server-test.sdkwork.com` | 200 SPA HTML |
| `http://127.0.0.1:18098/` + Host `server.sdkwork.com` | 200 SPA HTML |
| 管理端口同源 SPA（13800/18888/18080，带/不带 Host 头） | 全部 200 SPA HTML |

无 502/5xx；无 302 干扰（此前观察到的 302/301 系上版验证脚本缺陷的输出混入，干净验证不存在）。

### 3.3 网关 attach 契约（三环境）

| 检查项 | development | test | production |
| --- | --- | --- | --- |
| 容器内 bind | `0.0.0.0:8080` | `0.0.0.0:8080` | `0.0.0.0:8080` |
| 容器内 `8080/readyz` | 200 | 200 | 200 |
| webserver 容器解析 `sdkwork-api-cloud-gateway` | 172.18.0.5 | 172.19.0.5 | 172.21.0.5 |
| 网关容器状态 | healthy | healthy | healthy |

## 4. 部署期间修复记录

1. **ext4 构建闭包**：/mnt/e 禁止 pnpm（9p SQLite 限制）；rsync 至 `~/sdkwork-build`，补齐 pnpm-workspace 引用的 11 个兄弟仓库 + sdkwork-specs/sdkwork-github-workflow + Cargo 跨仓 path 依赖闭包（cloudrouter/id/im/kernel/memory/rpc-framework/database/knowledgebase/web-framework）。
2. **TS 构建**：`pnpm --filter "@sdkwork/webserver-pc..." --filter "@sdkwork/webserver-h5..." build` 先行构建 UI 依赖闭包（rsync 排除 dist 导致类型缺失）。
3. **attach 网络迁移**：网关舰队被并行会话重建为 0.1.1 bundle 形态，`deployments/docker/env/<env>.env` 的 `SDKWORK_MODULE_API_GATEWAY_ATTACH_NETWORK` 由 `sdkwork-gateway-<env>_default` 更新为 `sdkwork-api-cloud-gateway-<env>`。
4. **attach 契约 override**：bundle 网关默认 bind 3900 且无别名，用 bundle 专用 compose override（`ports: !override <host>:8080`、`SDKWORK_API_CLOUD_GATEWAY_BIND: 0.0.0.0:8080`、网络键 `sdkwork` 别名 `sdkwork-api-cloud-gateway`、8080 readyz healthcheck）重建，变量集复刻 deploy.sh（`POSTGRES_ASSETS_DIR`、`GATEWAY_INSTANCE_HOST_PORT`、`GATEWAY_PROFILE_ID`、`GATEWAY_IM_ID_NODE_ID`、`GATEWAY_MIGRATE_ON_START`、`--profile instance`）。
5. **P0-12 生产 fail-closed**：bundle embedded overlay 默认 DB URL 为 `sslmode=disable`，生产策略拒绝；在 `production.env` 追加 operator 覆盖 `SDKWORK_DATABASE_URL=postgresql://sdkwork_ai_prod:<GATEWAY_POSTGRES_PASSWORD URL 编码>@postgres:5432/sdkwork_ai_prod?sslmode=require&options=-c%20search_path%3Dsdkwork_ai_prod`（embedded postgres ssl=on，require 可用）。注意用户名必须为 `sdkwork_ai_prod`（与 embedded postgres `POSTGRES_USER` 一致，网关启动器会校验）。
6. **镜像占位符**：`production.env` 的 `GATEWAY_IMAGE=registry.sdkwork.com/sdkwork-api-cloud-gateway:<VERSION>` 占位符未填充，改为 `sdkwork-api-cloud-gateway:local`（三环境同一镜像）。
7. **env 文件被并行会话回退**：验证阶段发现 `production.env` 的修复被并行会话的 deploy.sh 重新生成回退（镜像占位符回归 + DB URL 丢失），已重新套用并重建。**若网关 bundle 被重新 deploy，上述两处修复与 attach override 均需重新套用**（长期修复方向：网关 bundle compose 参数化 bind/镜像/DB URL，避免 operator 手工 patch）。

## 5. 访问入口

WSL 内/宿主机 localhost（Windows 侧可直接访问 WSL 端口转发）：

- 开发：`http://127.0.0.1:13800`（SPA 同源，healthz `/healthz`）；数据面直连 `http://127.0.0.1/`（需 Host 头 `server-dev.sdkwork.com`）
- 测试：`http://127.0.0.1:18888`；数据面 `http://127.0.0.1:18898/`（Host 头 `server-test.sdkwork.com`）
- 生产：`http://127.0.0.1:18080`；数据面 `http://127.0.0.1:18098/`（Host 头 `server.sdkwork.com`）

域名形式（需在 Windows hosts 指向 127.0.0.1 或 WSL IP；数据面端口为各自 HTTP 端口）：

- `http://server-dev.sdkwork.com:80` → dev 数据面（Host 头匹配时）
- `http://server-test.sdkwork.com:18898`
- `http://server.sdkwork.com:18098`

> 说明：数据面按 virtual_host 域名路由，直连端口不带 Host 头会落到默认站点；管理端口（13800/18888/18080）同源服务 SPA + API，无需 Host 头，是测试最直接入口。

## 6. 已知约束与后续建议

- webserver 二进制为宿主 bind-mount 混合模式：镜像升级时需同步将新二进制拷贝至 `/mnt/e/.../target/release/sdkwork-api-webserver-standalone-gateway`。
- 并行会话运行网关 bundle `deploy.sh` 会回退 production.env（镜像/DB URL）与 attach override；建议将 attach 契约参数化合并进网关 bundle。
- `deployments/docker/env/<env>.env` 的 attach 网络名与网关 bundle 网络命名耦合（`sdkwork-api-cloud-gateway-<env>`），网关舰队重建/改名时需同步。
- ext4 构建副本 `~/sdkwork-build/` 与 /mnt/e 源码可能漂移；正式出包前建议重新 rsync 增量同步。
