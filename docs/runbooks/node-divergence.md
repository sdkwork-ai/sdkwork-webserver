# 节点发散 Runbook（sdkwork-webserver）

规范：PRD §8.2（节点同步幂等/可续传/校验和/围栏）、§5（集群操作）、REQ-2026-0052/0054。
适用症状：某 Web Node 的期望配置/证书/TLS 快照与控制面不一致、
`web_certificate_node_state` 观测滞后、云数据面节点 runtime-set 代际落后。

## 1. 识别发散

```bash
# 证书分发观测（每节点状态）
psql "$DATABASE_URL" -c "SELECT server_uuid, certificate_version_id, observed_at
       FROM web_certificate_node_state ORDER BY observed_at ASC LIMIT 20;"

# 节点心跳与收敛
psql "$DATABASE_URL" -c "SELECT uuid, last_heartbeat_at, metadata->>'syncGeneration' AS gen
       FROM web_server WHERE deleted_at IS NULL;"

# 本地节点侧（Web Node Daemon / agent）
systemctl status sdkwork-webserver-node-daemon   # 或 sdkwork-webserver-agent（v3 兼容名）
ls /var/lib/sdkwork/webserver/sync/              # 期望/观测 checkpoint
cat /var/lib/sdkwork/webserver/tls-materials/tls-runtime.json | head
```

## 2. 场景处置

### 2.1 节点同步代际落后（desired > observed）

- 同步是幂等且可续传的：节点按 checksummed `sv1:sha256` 代际拉取，崩溃后从
  durable checkpoint 重放。先等一个同步周期；持续落后再查：
  - 内部 API 可达性（`SDKWORK_WEBSERVER_INTERNAL_API_BASE_URL`）；
  - 节点令牌文件（`/run/secrets/*`）有效性与 `wagent_` 前缀（机器凭据不回退到用户密钥）；
  - 单实例进程锁（REQ-2026-0055）：确认没有第二个 daemon 并发运行。

### 2.2 证书物料不一致

- 控制面权威 = `web_certificate_version` + `web_listener_certificate_binding`；
  节点只是投影。修正绑定后，worker 重投影并发布新的 `tls-runtime.json`（单调代际）。
- 节点本地 recovery 存储拒绝 scope/hash 冲突；如果日志出现 conflict，说明
  期望快照与本地槽位不匹配——**不要**手工删除 recovery 槽位，先核对租户/环境维度。

### 2.3 节点持续离线

- 云部署（K8s）：检查 StatefulSet/PVC/Secret 与 provider-event 精确路由
  （见 deployments/kubernetes/README.md 第 5-7 步）；节点身份是固定的，
  不要水平克隆一个已注册 Node。
- standalone：单节点即全部；按部署通道（compose bundle / deb）恢复进程与依赖
  （PostgreSQL/Redis 在主机侧）。

## 3. 恢复判据

- `web_certificate_node_state.observed_at` 追平最新版本；
- 节点 `/healthz` 与数据面 `/readyz`（loopback operations 监听）通过；
- TLS 快照 generation 与控制面发布一致，边缘实测指纹与
  `web_certificate_version.fingerprint_sha256` 相同。

集群级发散（>1 节点同时落后）优先怀疑控制面或内部 API，而不是逐节点修复。
