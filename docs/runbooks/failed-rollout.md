# 发布失败 Runbook（sdkwork-webserver）

规范：PRD §8.2（失败金丝雀自动停止、30s 内恢复最后已验证版本）、FR-026（部署状态必须真实）。
适用症状：容器通道 `release.sh` 健康门失败、deb 升级后健康探针不通过、
应用部署记录长时间停留 PENDING。

## 1. 容器通道（deploy.sh / release.sh）

```bash
cd /opt/deploy/sdkwork-webserver/bundle
# 发布状态与追加式账本
bash deploy.sh --environment production --ps
tail -n 50 release-ledger.log 2>/dev/null || ls var/
# 逐实例健康门：新实例未通过 wait_container_healthy 前，旧实例保持服务
docker ps --format '{{.Names}} {{.Status}}' | grep sdkwork
```

- `release.sh` 自带摘要/sha256 完整性校验与**自动回滚**：新版本健康门失败时回退
  上一镜像标签并记录账本。人工回滚：
  `bash release.sh --rollback`（或按账本中上一 `--image-tag` 重新执行 `deploy.sh`）。
- 回滚后必须验证 `/healthz` 与业务入口，再决定是否修复后重发。

## 2. deb/systemd 通道

- 升级是"先停后装再启"的有界维护窗口（`TimeoutStopSec=45`）。postinst 启动后
  健康门失败时：
  ```bash
  systemctl status sdkwork-webserver
  journalctl -u sdkwork-webserver -n 200
  curl -fsS http://127.0.0.1:3800/healthz
  ```
- 回滚 = 重装上一个版本的 .deb（配置数据保留于 /etc/sdkwork/webserver 与
  /var/lib/sdkwork/webserver；数据库向前修复策略见 database/migrations）。
- 数据库迁移失败：迁移在启动漂移门控中 fail-closed，服务拒绝启动；按
  `docs/migrations/` 的迁移说明修复后重启（0006 为不可逆迁移，只能向前修复）。

## 3. 应用部署记录停留 PENDING（控制面）

这是**设计内**的真实状态：`web_deployment` 是命令意图，推进 status 的部署
执行权威属于 Deploy 控制面（REQ-2026-0061/0062 门，ADR-20260731 人工评审中）。
因此 `sites.activate`（要求存在成功部署）与 `deployments.rollback`（要求成功
来源版本）诚实地返回 409。处置：

- 向租户说明发布停留在"已接收"状态，不要重试激活。
- 跟踪 Deploy 执行权威上线后再启用完整发布流。

## 4. 升级后验证清单

1. `/healthz`、`/readyz` 通过；
2. 管理面登录与站点列表正常；
3. 数据面抽查一个静态站点与一个反代路由；
4. `bin/doctor.sh`（9 项只读诊断）无 FAIL。
