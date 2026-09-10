# 备份与恢复 Runbook（sdkwork-webserver）

规范：OPERATIONS_SPEC.md §5（RPO：生产 24h + 每次升级前；RTO：生产 4h）。

> **与 PRD §8.2 目标的差距（如实声明）**：PRD 要求数据库恢复目标
> RPO ≤ 5 分钟、RTO ≤ 15 分钟。当前已实现能力为"每日 pg_dump + 升级前备份 +
> 已验证的恢复演练"（RPO ≈ 24h，RTO ≈ 4h）。要达成 PRD 目标需启用
> WAL 流复制与物理基线备份（REQ-2026-0051 已验证该路径），并接入告警。
> 在此之前，不要在对外承诺中引用 PRD 恢复目标。
备份集生成在目标机 `/opt/deploy/sdkwork-webserver/backups/`，包含：配置（env 链）、
数据库（pg_dump 自定义格式）、卷（可选）、manifest.json + 每组件 `.sha256`。

## 1. 备份

```bash
bin/backup.sh create --environment demo                      # 配置 + 数据库 + 卷
bin/backup.sh create --environment demo --no-volumes         # 配置 + 数据库
bin/backup.sh list  --environment demo
```

生产建议加到 cron（每天 02:30）：
```cron
30 2 * * * cd /opt/deploy/sdkwork-webserver/bundle && bash deploy.sh --environment production --ps >/dev/null
30 2 * * * <repo>/bin/backup.sh create --environment production --host ssh://ops@db-host
```

## 2. 校验

```bash
bin/backup.sh verify --environment demo                     # 最新一组
bin/backup.sh verify --environment demo --set <set名>       # 指定一组
```

## 3. 恢复（破坏性，需 --yes）

```bash
bin/backup.sh restore --environment demo --set <set名> --yes
bin/backup.sh restore --environment demo --component config --set <set名> --yes  # 只恢复配置
bin/docker-deploy.sh install --environment demo        # 恢复后重新拉起
```

restore 流程：校验 checksum → `deploy.sh --down` 停栈 → 恢复所选组件 →
打印重新启动命令。数据库恢复用 `pg_restore --clean --if-exists`（forward-only
迁移回退的唯一受支持路径）。

## 4. 演练（每季度）

1. 选一组最近的备份：`bin/backup.sh list --environment demo`；
2. 在隔离目标机上恢复：`--host ssh://<scratch-host>`；
3. `bin/doctor.sh --environment demo` 全绿；
4. 记录耗时，确认满足 RTO（生产 4h）。

## 5. 保留与清理

| 环境 | 保留代数 |
|---|---|
| development / test | 3 |
| staging / demo | 7 |
| production | 30 + 每次发布前的最后一份 |

清理使用 `--purge` 前先备份：
```bash
bin/backup.sh create --environment demo
bin/docker-deploy.sh down --environment demo --purge --yes
```

## 6. 数据库连接键

本模块使用 `SDKWORK_DATABASE_*` 前缀；`host.docker.internal` 在宿主侧自动映射为 `127.0.0.1`。
