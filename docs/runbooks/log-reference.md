# 日志 Runbook（sdkwork-webserver）

容器日志策略（OPERATIONS_SPEC.md §2）：`json-file` 驱动、`max-size 50m`、
`max-file 3`；服务只写 stdout/stderr。日志只通过 `docker compose logs` 读取，
不要直接读 `/var/lib/docker/containers/...`。

## 1. 读取日志（默认有界读取，不会挂住终端）

```bash
bin/docker-deploy.sh logs --environment demo                       # 实例 1，默认 200 行
bin/docker-deploy.sh logs --environment demo --tail 1000
bin/docker-deploy.sh logs --environment demo --since 15m
bin/docker-deploy.sh logs --environment demo --instance 2
bin/docker-deploy.sh logs --environment demo --service sdkwork-webserver
bin/docker-deploy.sh logs --environment demo --follow              # 显式才持续跟踪
```

## 2. 导出日志（工单附件）

```bash
bin/docker-deploy.sh logs --environment demo --tail 5000 --export ./incident
# 生成 incident/webserver-demo-i1-| 环境 | 健康端口 |
|---|---|
| development | 13800 |
| test | 18888 |
| staging | 18081 |
| demo | 19080 |
| production | 18080 |-<UTC>.log.gz + .sha256
```

导出物按密钥级材料对待，不要粘贴到公开渠道。

## 3. 健康端口

/healthz

健康探测：`bin/doctor.sh --environment demo` 会同时检查容器健康状态与
`http://127.0.0.1:<port>SDKWORK_DATABASE_*`。

## 4. 常见日志特征

| 特征 | 含义 | 处置 |
|---|---|---|
| `panicked at` | 进程级崩溃 | 容器会重启；抓取日志并升级 |
| 大量 `connection refused`（DB/Redis） | 依赖不可达 | 检查 `SDKWORK_DATABASE_*` HOST/PORT/密码 |
| `not healthy` / healthcheck 超时 | 启动过慢或依赖阻塞 | `doctor.sh` 看 compose 与 probe |
| 启动后无任何输出 | 容器在重启循环 | `doctor.sh` health 检查给出计数 |

## 5. 日志级别

`RUST_LOG`（或等价键）控制级别：`development/test` 默认 `debug`，
`staging/demo/production` 默认 `info`。生产开启 `debug` 需记录变更并设置过期时间。
