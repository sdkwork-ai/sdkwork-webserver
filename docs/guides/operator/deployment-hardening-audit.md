# 部署加固审计报告（Deployment Hardening Audit）

> 状态：已实施并验证 ｜ 适用：sdkwork-webserver / sdkwork-api-cloud-gateway（sibling）/ sdkwork-cloudrouter / sdkwork-im 统一安装 bundle
> 依据：`sdkwork-specs/OPERATIONS_SPEC.md` §6（运行时加固）、§1.2（release.sh 发布门禁）、ENVIRONMENT_SPEC（五环境）
> 镜像基线：`registry.sdkwork.com/apps/sdkwork-webserver-standalone:0.1.2`（image id `a244f53e5a6c`）

---

## 1. 审计范围与结论

对四个模块的统一安装 bundle compose 逐镜像审计了 **重启策略、健康检查、日志轮转、CPU/内存/pids 上限、文件描述符上限、Linux 安全选项、端口暴露面** 七个维度，并完成以下加固：

1. **进程数上限迁移到 `deploy.resources.limits.pids`** —— Compose v5 禁止同时设置 `pids_limit` 与 `deploy.resources.limits.pids`（报错 `can't set distinct values`）；全部 11 个服务已统一迁移，修复了 demo 重装失败。
2. **全部服务追加 `security_opt: [no-new-privileges:true]`** —— 阻断容器内提权（setuid/setgid），postgres/redis/Rust 应用均兼容。
3. **资源上限全部 env 可调**（`*_CPU_LIMIT` / `*_MEMORY_LIMIT`），默认值即生产安全水位。

## 2. 逐镜像加固矩阵

| 服务 | restart | healthcheck | 日志轮转 | CPU/内存 | pids | nofile | security_opt | 端口绑定 |
|---|---|---|---|---|---|---|---|---|
| webserver（实例） | unless-stopped | mgmt :3800 `/healthz`，start_period 240s | json-file 50m×3 | 4.0 / 4g | 512 | 65536 | no-new-privileges | 边缘 0.0.0.0:80/443；**管理面 127.0.0.1** |
| postgres（嵌入式） | unless-stopped | `pg_isready`，5s×12 | json-file 50m×3 | 2.0 / 2g | 256 | — | no-new-privileges | 仅内部网络 |
| redis（嵌入式） | unless-stopped | `redis-cli ping`，5s×12 | json-file 50m×3 | 1.0 / 1g | 128 | — | no-new-privileges | 仅内部网络 |
| gateway（sibling） | unless-stopped | `/readyz`，start_period 240s | json-file 50m×3 | 4.0 / 4g | 512 | 65536 | no-new-privileges | 仅内部网络（宿主发布 391x 由 deploy.sh 控制） |
| knowledgebase-rpc | unless-stopped | gRPC 健康探测 | json-file 50m×3 | 1.0 / 1g | 256 | — | no-new-privileges | 仅内部网络（50054 mTLS） |
| cloudrouter | unless-stopped | `/healthz`，start_period 60s | json-file 50m×3 | 4.0 / 4g | 512 | 65536 | no-new-privileges | 边缘 0.0.0.0（3900 容器 395x 映射） |
| im-gateway | unless-stopped | `/healthz`，start_period 60s | json-file 50m×3 | 4.0 / 4g | 512 | 65536 | no-new-privileges | 边缘 0.0.0.0:18079 |

补充治理项：

- **redis 内存封顶**：`--maxmemory 768mb --maxmemory-policy allkeys-lru`（`REDIS_MAXMEMORY` / `REDIS_MAXMEMORY_POLICY` 可调），突发流量淘汰旧缓存而非写失败；主动关闭 RDB/AOF（会话/限流数据可重建，低延迟优先）。
- **gateway 启动顺序**：`depends_on: knowledgebase-rpc: condition: service_healthy`，避免首启 mTLS 未就绪导致的拨号风暴。
- **cap_add 最小化**：webserver 仅保留 `NET_BIND_SERVICE`（绑定 80/443）；未使用 `cap_drop: ALL`（会破坏 postgres/redis entrypoint 的 chown 逻辑，见 §6 后续项）。

## 3. PostgreSQL 连接预算（外置依赖模式）

外置宿主 PG 18 `max_connections=400`（`superuser_reserved_connections=3`），五环境共约 **265 连接（66% 水位）**。

**预算公式**：

```
总连接 ≈ Σ_环境 [ webserver实例数 × 10（SDKWORK_DATABASE_MAX_CONNECTIONS）
              + gateway 50
              + cloudrouter 10
              + im 10
              + kb-rpc ≈ 10 ] + 运维/迁移临时连接
约束：总连接 ≤ max_connections − superuser_reserved − 15% 突发余量
```

**扩容判据**：任一环境 webserver 实例数 > 4，或新增环境时，按公式重算；超出即上调宿主 `max_connections`（每 +100 连接约 +0.5~1GB 宿主内存）并同步检查 `shared_buffers`。

## 4. 高可用（HA）与高并发能力评估

**当前已具备**：

- 单实例崩溃自动重启（`unless-stopped`）+ 健康门禁（install/upgrade 不 healthy 即失败，release.sh 自动回滚）。
- **多实例水平扩展**：`install/upgrade --replicas N` 每实例独立 compose project（`-i<i>`），边缘端口步进，管理端口自动分配；实例无共享本地状态（数据在 PG/Redis/命名卷）。
- 依赖健康启动顺序（kb-rpc → gateway），start_period 覆盖冷启动迁移窗口。

**并发能力边界**：单容器 4 CPU/4GB/512 pids/nofile 65536；嵌入式 PG 200 连接、Redis 768MB。单宿主横向受限的根本约束是 **共享单宿主**（见 §5）。

**生产 HA 拓扑建议（当前单宿主 SPOF 的解法，按优先级）**：

1. **负载均衡入口**：LB（Nginx/HAProxy/云 NLB）替换单宿主 80/443，对 N 台宿主的 webserver 实例做健康检查轮询。
2. **PostgreSQL 流复制**：1 主 + 1 热备（streaming replication），`PG_MAX_CONNECTIONS` 对齐 §3 预算；failover 用 Patroni/repmgr。
3. **Redis 哨兵**：1 主 2 从 + 3 哨兵，应用侧 `REDIS_HOST` 换哨兵端点。
4. **宿主反亲和**：同一环境的 webserver 实例分散到 ≥2 台宿主（`--replicas` + 多宿主重复 install）。
5. **镜像仓库 HA**：`registry.sdkwork.com` 双实例 + 只读副本，避免拉取失败阻塞回滚。

## 5. 遗留风险清单（已知、未阻塞）

| # | 风险 | 影响 | 缓解现状 |
|---|---|---|---|
| 1 | env 文件明文密钥（DB 密码、signing secret） | 宿主文件泄露 = 凭据泄露 | 权限 0600 + deploy.sh 校验；生产建议接入 Docker secrets / Vault（gateway 已预留 `SDKWORK_VAULT_*`） |
| 2 | 嵌入式/外置 Redis 无密码 | 内网横向可直连 | 仅绑定内部网络；生产建议 `requirepass` + TLS |
| 3 | 管理面（:3800 等）无 TLS | 本机嗅探可见管理流量 | 已绑 127.0.0.1；远程管理走 SSH 隧道 |
| 4 | 单宿主 SPOF | 宿主宕机全环境不可用 | §4 HA 拓扑 1-4 落地前，依赖 backup 体系（RPO/RTO 见 OPERATIONS_SPEC §8） |
| 5 | `cap_drop: ALL` 未启用 | 攻击面略宽于最小化 | no-new-privileges 已阻断提权；启用 cap_drop 需先验证 postgres/redis entrypoint |

## 5a. 运维注意点（本次验证中发现）

1. **compose 修改的正确同步链路**：`install` 每次都会把 **dist 安装包** 的 compose 目录推送到 `/opt/deploy/<module>/bundle/compose/` 并覆盖。因此修改 compose 必须走 `仓库 deployments/docker/ → dist 安装包 bundle/compose/ → install` 的链路；直接改 `/opt/deploy` 会在下次 install 时被静默回滚（本次已实测踩坑）。
2. **多副本栈的生命周期操作**：`stop/restart` 的 `--replicas` 默认回 1，只遍历实例 1。对 `--replicas N` 部署的栈执行生命周期操作时必须重新传 `--replicas N`（`bin/docker-deploy.sh restart --environment X --replicas N`），否则其余实例不会被触碰。

## 6. 后续跟进项

1. `cap_drop: [ALL]` + 按服务白名单 `cap_add`（先在 dev 验证 postgres/redis entrypoint）。
2. gateway bundle `deploy.sh logs` 补齐 `--tail/--since/--export`（webserver 已支持，契约对齐）。
3. webserver entrypoint 的 rsync `--checksum` 在 DrvFs 上极慢，改为 size+mtime（dev-only 路径）。
4. 生产部署前按 §4 完成 LB + PG 流复制 + Redis 哨兵演练。

## 7. 验证记录

- Compose v5 校验：4 份 compose YAML 解析通过，11/11 服务含 `deploy.resources.limits.pids` 与 `security_opt`，`pids_limit` 关键字全仓清零（kernel cloud compose 为旧式单实例写法、无冲突，保留）。
- demo 重装（三次迭代）：`bin/docker-deploy.sh install --environment demo --deps external --host wsl --yes` 退出码 0；第三次安装容器 Recreate 后 `docker inspect` 实测：
  - webserver：`Memory=4G NanoCpus=4 PidsLimit=512 SecurityOpt=[no-new-privileges:true] Restart=unless-stopped Health=healthy`
  - gateway：`Memory=4G NanoCpus=4 PidsLimit=512 SecurityOpt=[no-new-privileges:true] Health=healthy`
  - knowledgebase-rpc：`Memory=1G NanoCpus=1 PidsLimit=256 SecurityOpt=[no-new-privileges:true] Health=healthy`
- 健康验证：mgmt `127.0.0.1:19080/healthz` → 200；边缘 `demo.sdkwork.com`（:19098，`--noproxy '*'`）→ 200。
- 多实例 HA：`install --replicas 2 --dry-run` 通过（`--replicas 2` 透传 deploy.sh）；deploy.sh 实例循环 = 每实例独立 compose project（`-i<i>`）+ 管理端口 `PORT_BASE+index-1` 步进 + 实例 1 独占边缘端口并先执行迁移（见 §5a 注意点 2）。

---

*English version: [deployment-hardening-audit.en.md](deployment-hardening-audit.en.md)*
