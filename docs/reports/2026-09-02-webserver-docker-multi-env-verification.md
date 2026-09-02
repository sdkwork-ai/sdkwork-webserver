# sdkwork-webserver Docker 同一镜像三环境部署验证报告

- 日期：2026-09-02
- 环境：Windows 宿主 + WSL Ubuntu-22.04（Docker in WSL）
- 工作区：`/mnt/e/sdkwork-space/sdkwork-webserver`
- 结论：**同一镜像在 development / test / production 三环境部署全部 healthy，域名访问矩阵验证通过（详见下表）**

## 1. 镜像与部署拓扑

| 项 | 值 |
| --- | --- |
| 镜像 | `registry.sdkwork.com/apps/sdkwork-webserver-standalone:0.1.0` |
| 镜像 ID | `51a513b765ce`（5.69GB，debian:bookworm-slim + Rust 数据面） |
| 构建链 | `webserver-release.mjs package --deployment-profile standalone` → `build-standalone-image.mjs` → `deploy-docker-environment.sh all --validate` |
| 拓扑 | webserver 容器 = 唯一公共边缘（standalone-only）；attach 独立网关栈 `sdkwork-gateway-<env>_default`（别名 `sdkwork-api-cloud-gateway:8080`）；模块导入平面 `imports.d/import.conf`（71 模块 sidecar include） |
| 管理面端口 | dev 13800 / test 18888 / prod 18080（容器内 3800） |
| 数据面端口 | dev 80/443、test 18898/28430、prod 18098/38430 |

三环境容器最终状态（`docker ps`）：

```
sdkwork-webserver-development   Up (healthy)
sdkwork-webserver-test          Up (healthy)
sdkwork-webserver-production    Up (healthy)
sdkwork-gateway-development-gateway-1   Up (healthy)
sdkwork-gateway-test-gateway-1          Up (healthy)
sdkwork-gateway-production-gateway-1    Up (healthy)
```

## 2. 域名访问验证矩阵

验证方式：WSL 内 `curl --noproxy '*'`（Host 路由 / `--resolve` SNI）。**注意 WSL 环境代理（127.0.0.1:7897）必须绕过。**

### 2.1 管理面（HTTP）

| 环境 | 域名 | 地址 | /healthz | 控制台 SPA `/` |
| --- | --- | --- | --- | --- |
| development | server-dev.sdkwork.com | http://127.0.0.1:13800 | 200 `{"status":"ok"}` | 200 |
| test | server-test.sdkwork.com | http://127.0.0.1:18888 | 200 `{"status":"ok"}` | 200 |
| production | server.sdkwork.com | http://127.0.0.1:18080 | 200 `{"status":"ok"}` | 200 |

### 2.2 API 数据面（HTTP，Host 路由：webserver 边缘 → 网关 :8080）

| 环境 | 域名 | 地址 | /healthz |
| --- | --- | --- | --- |
| development | api-dev.sdkwork.com | http://127.0.0.1:80 | 200 |
| test | api-test.sdkwork.com | http://127.0.0.1:18898 | 200 |
| production | api.sdkwork.com | http://127.0.0.1:18098 | 200 |

### 2.3 production HTTPS（TLS :38430，自签引导证书）

| 域名 | 路径 | 结果 |
| --- | --- | --- |
| api.sdkwork.com | /healthz | 200 |
| server.sdkwork.com | / | 200 |
| notes.sdkwork.com | / | 200 |

证书：bootstrap 自签 `CN=sdkwork.com`，SAN 覆盖 `DNS:sdkwork.com, DNS:*.sdkwork.com` + 全部聚合 server_name（含多级子域 `edge.aiot.sdkwork.com`，14 个品牌域 × 全部模块 vhost）。运营者/ACME 真实证书放入同路径即自动取代，无需改配置。

### 2.4 dev/test HTTPS — 当前代次不适用

dev/test 数据面合并配置仅有 `listen 80`（模块 nonproduction sidecar 代次无 `listen 443 ssl`），容器内未监听 443；compose 的 `443/28430→443` 映射为死映射（TCP 可连、握手即断）。**TLS 数据面仅 production 提供**，属当前 sidecar 生成器的设计现状，非部署故障。

## 3. 本次修复记录

### 3.1 entrypoint-standalone.sh（仓库文件，已进镜像）

1. bootstrap app id 默认值 `sdkwork-web` → `sdkwork-webserver`（IAM 模板代次对齐）。
2. 模块静态根物化器重写：支持普通 `root /usr/share/sdkwork/<code>/web/{pc,h5,static}`（sed `#` 分隔符，规避 `s|…|` 下 `\|` 变字面管道）、扩展扫描 `snippets/*.conf`（agents 模块）、static 表面回退模块 `deployments/webserver/static/`（appbase）。三环境各物化 78 个模块静态根链接。
   - 过程教训：link 函数加了 static 分支但 sed 交替漏 `static`，提取端零匹配（发现于镜像内比对）。
3. TLS 证书引导 SAN 聚合：`ensure_domain_certificate` 增加额外 SAN 列表参数；`ensure_imported_sidecar_certificates` 两遍扫描（聚合每个证书目录下全部 server_name → 生成证书）；自签证书 SAN 不完整自动再生；`certificate_covers()` 固定串匹配；非自签（运营者/ACME）材料永不覆盖。

### 3.2 sdkwork-api-cloud-gateway（跨仓库契约对齐）

网关栈（`sdkwork-gateway-<env>`）与 webserver §17.3 attach 契约漂移：sidecar 字面直连 `sdkwork-api-cloud-gateway:8080`，而网关 compose 实际 bind 3900 且无别名。新增 `sdkwork-api-cloud-gateway/docker-compose.override.yml`：

- `SDKWORK_API_CLOUD_GATEWAY_BIND: 0.0.0.0:8080`
- `networks.default.aliases: [sdkwork-api-cloud-gateway]`
- healthcheck readyz 对齐 8080；`ports: !override` 保持宿主调试端口 3910-3913 → 8080

三栈 `up -d gateway` 重建后全部 healthy，API 平面端到端 200。

### 3.3 数据修复（宿主 PostgreSQL 127.0.0.1:15432）

- 三库 `iam_application_template`：app_key `sdkwork-web` → `sdkwork-webserver`（同名异 key 唯一约束崩溃）。
- production `iam_tenant_signing_key`：清空 tenant 100001 旧密钥行，bootstrap 以恢复的 `SDKWORK_IAM_SIGNING_MASTER_SECRET` 重新生成（credential-entry bootstrap Access-Token 签发成功）。

## 4. 遗留事项

1. **服务端 ALPN 缺陷**：production TLS 对「客户端 ALPN 仅 h2」的握手直接断连（数据面不支持 HTTP/2 且回退处理失败）。`h2,http/1.1` 列表可正确回退 http/1.1（openssl 验证），浏览器场景理论可用；`curl` 需 `--no-alpn`。建议数据面后续支持 h2 或修正 ALPN 回退语义。
2. entrypoint 与 gateway override 修改尚未 git 提交。
3. ~~`pnpm-lock.yaml` 与 `package.json` 依赖漂移~~ **已核实为误报**：工作区两文件均与 HEAD 一致（git 干净）；此前观察到的 4 依赖缺口源自 ext4 构建副本（`~/sdkwork-build/sdkwork-webserver`）的陈旧目录结构（副本仍保留 `sdkwork-web-app-sdk` 旧目录，HEAD `9be86902` 已更名为 `sdkwork-webserver-*`），副本 lockfile 的额外 importers 不应回灌工作区。建议下次前端构建前重新 rsync 副本。
4. `sdkwork-notes` 空间克隆的占位 dist（`apps/sdkwork-notes-pc/dist/...`）为未跟踪文件。
5. 宿主 80/443 由 development 环境独占（三环境端口矩阵约定）。
6. dev/test 数据面如需 TLS，需模块 sidecar 生成器支持 nonproduction `listen 443 ssl`。

## 5. 验证脚本存档

- `.sdkwork/runtime/verify-matrix-final.sh` — 完整矩阵
- `.sdkwork/runtime/verify-https.sh` / `tls-stability.sh` — HTTPS 专项
- `.sdkwork/runtime/fix-gateway-attach.sh` — 网关 override 重建
