# 证书事故 Runbook（sdkwork-webserver）

规范：PRD §8.4（证书事故 runbook）、REQ-2026-0048（bounded ACME 生命周期）。
适用症状：证书即将/已经过期、签发或续期操作卡在 `PENDING/RUNNING`、CA 拒绝、
节点 TLS 物料与控制面不一致。

## 1. 快速定位

```bash
# 操作状态（0=PENDING,1=RUNNING,2=SUCCEEDED,3=FAILED,4=EXHAUSTED）
psql "$DATABASE_URL" -c "SELECT id, operation_type, status, attempts, lease_owner,
       lease_expires_at, fencing_token, next_attempt_at
       FROM web_certificate_operation ORDER BY id DESC LIMIT 20;"

# 证书聚合与版本
psql "$DATABASE_URL" -c "SELECT uuid, status, renewal_status, auto_renew
       FROM web_certificate WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT 20;"

# 证书 worker 是否存活（systemd 部署）
systemctl status sdkwork-webserver-certificate-worker
journalctl -u sdkwork-webserver-certificate-worker -n 200
```

## 2. 场景处置

### 2.1 续期即将到期但 worker 未认领

- 确认 worker 进程存活、`SDKWORK_WEBSERVER_CERT_OPERATION_POLL_INTERVAL_SECS` 未被放大。
- 到期扫描周期由 `SDKWORK_WEBSERVER_CERT_RENEW_SCAN_INTERVAL_SECS`（默认 3600s）控制；
  紧急时重启 worker 触发即时扫描：`systemctl restart sdkwork-webserver-certificate-worker`。
- 手动续期走 API（返回 202 + operationId，异步完成）：
  `POST /backend/v3/api/certificates/{certificateId}/renew`。

### 2.2 操作卡在 RUNNING（worker 崩溃遗留）

租约机制会自愈：`lease_expires_at` 过期后，下一次认领（`FOR UPDATE SKIP LOCKED`）
会以 `fencing_token+1` 重新认领。若需立即恢复，等待租约到期即可；**不要**手工
UPDATE 状态行（会绕过围栏令牌校验）。

### 2.3 重试预算耗尽（EXHAUSTED）

- 查 `web_certificate.metadata.certificateOperationFailureCode` 定位失败类别
  （DNS 未验证 / CA 拒绝 / webroot 不可写）。
- 修复根因后重新发起 issue/renew；新操作有独立的重试预算。

### 2.4 ACME 账户/目录异常

- 账户加密存储于 `SDKWORK_WEBSERVER_ACME_ACCOUNT_ROOT`（AES-256-GCM）。
  **绝不可**手工编辑或删除账户文件——会导致每次签发新建 CA 账户（触发速率限制）。
- wildcard 域名需要 DNS-01（当前未实现，配置校验会显式拒绝）；使用 SAN 多域名证书替代。

### 2.5 节点 TLS 物料不一致（边缘仍用旧证书）

- 检查 `SDKWORK_WEBSERVER_TLS_MATERIAL_ROOT` 与 `tls-runtime.json` 快照的
  generation/fingerprint 是否已更新。
- worker 在每次成功操作后会重投影节点 listener 绑定并发布单调快照；数据面
  `FileTlsRuntimeController` 热加载，不落连接。若快照未更新，优先查 worker 日志，
  其次核对 `web_listener_certificate_binding` 的 `desired_version_id` 与当前版本一致。

## 3. 恢复后验证

```bash
# 公网端到端校验（证书链、主机名、有效期）
openssl s_client -connect server.sdkwork.com:443 -servername server.sdkwork.com </dev/null \
  | openssl x509 -noout -dates -subject -ext subjectAltName
```

PRD 成功指标：100% 生产域名持有有效 TLS 1.2/1.3 证书并通过到期/主机名/链校验。
