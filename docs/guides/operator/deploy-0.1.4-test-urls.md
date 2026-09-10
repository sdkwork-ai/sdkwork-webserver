# SDKWork Web Server 0.1.4 — 五环境部署测试 URL 清单

> 部署时间：2026-09-09 深夜 · 镜像 `registry.sdkwork.com/apps/sdkwork-webserver-standalone:0.1.4`
> 外部依赖：宿主 Ubuntu-22.04 OS 级 PostgreSQL(5432) + Redis(6379 免密)，容器经 192.168.31.116 访问

## 一、hosts 配置（本机测试前提）

Windows 测试机请先在 `C:\Windows\System32\drivers\etc\hosts`（管理员编辑）追加：

```
127.0.0.1 server-dev.sdkwork.com server-app-dev.sdkwork.com server-admin-dev.sdkwork.com
127.0.0.1 server-test.sdkwork.com server-app-test.sdkwork.com server-admin-test.sdkwork.com
127.0.0.1 server-staging.sdkwork.com server-app-staging.sdkwork.com server-admin-staging.sdkwork.com
127.0.0.1 server-demo.sdkwork.com server-app-demo.sdkwork.com server-admin-demo.sdkwork.com
127.0.0.1 server.sdkwork.com server-app.sdkwork.com server-admin.sdkwork.com
```

- 本机（Windows）访问用 `127.0.0.1`；局域网其它设备把上述 IP 换成 `192.168.31.116`。
- 每个环境另有 13 个品牌备用域（birdcoder/dtupay/noaper/skubc/zowalk/offer86/86offer × .com/.cn），同样规则可加。

## 二、五环境域名 URL（HTTP 已全量验证 200）

| 环境 | PC/H5/控制台入口 | 备用入口 | 管理端口 (API) | HTTPS 端口 |
| --- | --- | --- | --- | --- |
| development | http://server-dev.sdkwork.com | http://server-app-dev.sdkwork.com · http://server-admin-dev.sdkwork.com | http://server-dev.sdkwork.com:13800/healthz | 443 |
| test | http://server-test.sdkwork.com | http://server-app-test.sdkwork.com · http://server-admin-test.sdkwork.com | http://server-test.sdkwork.com:18888/healthz | 28430 |
| staging | http://server-staging.sdkwork.com | http://server-app-staging.sdkwork.com · http://server-admin-staging.sdkwork.com | http://server-staging.sdkwork.com:18081/healthz | 38431 |
| demo | http://server-demo.sdkwork.com | http://server-app-demo.sdkwork.com · http://server-admin-demo.sdkwork.com | http://server-demo.sdkwork.com:19080/healthz | 38432 |
| production | http://server.sdkwork.com | http://server-app.sdkwork.com · http://server-admin.sdkwork.com | http://server.sdkwork.com:18080/healthz | 38430 |

说明：
- 端口 80（dev）/ 18898（test）/ 18099（staging）/ 19098（demo）/ 18098（production）为各环境边缘 HTTP 入口，hosts 指好后直接 `http://server-<env>.sdkwork.com` 即可，无需带端口（dev 的 80 除外已默认）。
- 管理端口是容器 3800（管理/健康面）的宿主映射，浏览器直接打开 `/healthz` 应返回 200。
- demo 种子账号（此前会话确认）：admin / admin@sdkwork.com（tenant 100001）。

## 三、健康与部署证据

- 15 个容器全部 healthy（每环境 webserver + gateway + knowledgebase-rpc），镜像统一 0.1.4。
- `bin/doctor.sh`：development / test 11/11 全绿；staging / demo / production 各仅 1 项 `WEBSERVER_REDIS_PASSWORD` 空值 FAIL —— 这是你要求的 Redis 免密与严格占位门禁的固有冲突，属意向性例外，请勿填密码。
- install bundle：`dist/docker-install/sdkwork-webserver-install-0.1.4.bundle`（sha256 579f111a…），可直接用于其它 Ubuntu 机器离线安装。
- staging 签名密钥轮换备份：`/opt/deploy/sdkwork-webserver/backups/manual/iam_tenant_signing_key_staging_20260909T234435Z.sql`。

## 四、已知开放项

- **HTTPS 边缘**：五个环境的 443/HTTPS 端口当前 TLS 握手被服务端拒绝（alert 49）。原因是 TLS 运行时快照（证书分配流水线）未激活，属服务端 fail-closed 行为，不影响 HTTP 测试。如需 HTTPS，需要后续激活证书分配 worker 或手动放置 tls-runtime 快照。
- 本机若有代理（7897 端口 Clash 类），浏览器访问 HTTPS 会先被代理劫持；测试 HTTP 不受影响。
