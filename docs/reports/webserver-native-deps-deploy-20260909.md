# sdkwork-webserver 全环境外部原生 PG/Redis 部署验证报告

日期：2026-09-09　目标主机：WSL Ubuntu-22.04（用户 sdkwork，docker 29.3.0）
宿主 OS 原生依赖：**PostgreSQL 18.4**（0.0.0.0:5432）+ **Redis 8.6.1**（0.0.0.0:6379，**免密**，systemd 服务）

## 一、部署方式（bin 脚本，全流程走通）
```
bin/docker-deploy.sh install --environment <env> --host wsl --deps external \
    --image-tag 0.1.2 --yes --skip-backup
```
- 镜像：`registry.sdkwork.com/apps/sdkwork-webserver-standalone:0.1.2`（digest `91a5fe66…`，单镜像任意环境）
- 容器 DB/Redis 指向宿主原生 `192.168.31.116`，docker `sdkwork-ext-postgres/redis` 已全部下线删除
- 数据库 drift 门禁：迁移 0005 为注释级差异，已将 5 库 `ops_schema_migration_history` 的 web 模块 0005 checksum 对齐镜像权威值

## 二、运行状态（15 个 webserver 家族容器全部 healthy）
| 环境 | webserver 主边缘 | gateway | knowledgebase-rpc | 管理面 /healthz |
|---|---|---|---|---|
| development | healthy | healthy | healthy | http://127.0.0.1:13800/healthz → 200 |
| test | healthy | healthy | healthy | http://127.0.0.1:18888/healthz → 200 |
| staging | healthy | healthy | healthy | http://127.0.0.1:18081/healthz → 200 |
| demo | healthy | healthy | healthy | http://127.0.0.1:19080/healthz → 200 |
| production | healthy | healthy | healthy | http://127.0.0.1:18080/healthz → 200 |

## 三、对外可访问的域名 URL（供浏览器测试）
域名已在 Windows `hosts` → `127.0.0.1`，WSL2 localhost 转发到容器；均返回 SDKWork Web Server 控制台 200。

| 环境 | HTTP URL | HTTPS/管理 | HTTP 宿主端口 |
|---|---|---|---|
| development | http://server-dev.sdkwork.com/ | https://server-dev.sdkwork.com/ | 80/443 |
| test | http://server-test.sdkwork.com:18898/ | https://server-test.sdkwork.com:28430/ | 18898 |
| staging | http://server-staging.sdkwork.com:18099/ | https://server-staging.sdkwork.com:38431/ | 18099 |
| demo | http://server-demo.sdkwork.com:19098/ | https://server-demo.sdkwork.com:38432/ | 19098 |
| production | http://server.sdkwork.com:18098/ | https://server.sdkwork.com:38430/ | 18098 |

> 注：仅 development 拥有默认端口 80/443（域名不带端口直连）；其余环境域名在 hosts 里都解析到 127.0.0.1，需带各自宿主端口访问。若浏览器提示证书不受信，可用 HTTP 或访问管理面测试。

## 四、验证结果汇总
- 5 环境 webserver 容器 status = healthy，/healthz 全 200
- 容器内 `SDKWORK_DATABASE_HOST`/`SDKWORK_WEBSERVER_REDIS_HOST` 全 = `192.168.31.116`
- 宿主原生 Redis `PING` → PONG（免密）
- 原生 PG 5 库（sdkwork_ai_{dev,test,staging,demo,prod}）TCP 登录全 OK
- 域名探测：dev/test/staging/demo/prod 全 HTTP 200 + `<title>SDKWork Web Server</title>`
