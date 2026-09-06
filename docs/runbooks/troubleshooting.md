# 排障 Runbook（sdkwork-webserver）

原则：先 `doctor.sh`，再读日志，最后动手。不要在没有证据时重启。

## 0. 一条命令定位

```bash
bin/doctor.sh --environment demo          # 9 项检查，退出码非 0 即有问题
bin/doctor.sh --environment demo --json   # 给看板/工单
bin/doctor.sh --environment demo --export ./incident
```

检查项：toolchain / bundle / compose / 容器健康 / HTTP 探测 / 镜像漂移 /
配置漂移 / 日志错误 / 磁盘。

## 1. 症状 → 处置

### 1.1 `bundle FAIL`
```bash
bin/docker-deploy.sh install --environment demo
```

### 1.2 `compose FAIL`（没有容器）
```bash
bin/docker-deploy.sh status --environment demo      # 看不到东西
bin/docker-deploy.sh install --environment demo     # 重新拉起
```

### 1.3 `health FAIL`（unhealthy / 重启循环）
```bash
bin/docker-deploy.sh logs --environment demo --tail 300
# 常见：依赖连不上 → 1.5；OOM → 调整资源；迁移失败 → 看 1.6
```

### 1.4 `probe FAIL`（端口不通）
```bash
bin/docker-deploy.sh status --environment demo       # 实例端口映射
bin/config.sh show --environment demo | grep -i PORT # 期望端口
```
端口矩阵：development=13800 … production=18080（实例 i 使用 基础端口+i-1 或运营商步进）。

### 1.5 `config FAIL`（漂移 / 占位符）
```bash
bin/config.sh diff     --environment demo     # 缺失键 / 未声明键 / 占位符
bin/config.sh validate --environment demo     # 模块校验器
bin/config.sh set SDKWORK_DATABASE_PASSWORD '<真实值>' --environment demo
bin/config.sh show    --environment demo      # 复核（密钥已脱敏）
```
`set` 会先在目标机留 `.bak.<时间戳>` 备份，再做校验；校验失败会提示恢复路径。
修改后需要一次完整 `install`（不是 restart）让 compose 重新插值：
```bash
bin/docker-deploy.sh install --environment demo
```

### 1.6 `image WARN`（运行镜像与期望 tag 不一致）
```bash
bin/docker-deploy.sh upgrade --environment demo --image-tag <期望tag>
```

### 1.7 `logs WARN`（ERROR/panic 多）
```bash
bin/docker-deploy.sh logs --environment demo --tail 1000 --export ./incident
```

### 1.8 `disk WARN/FAIL`
```bash
docker system df
docker image prune -f          # 仅在确认无正在使用的悬空镜像后
```

## 2. 数据损坏 / 需要回滚数据
```bash
bin/backup.sh list    --environment demo
bin/backup.sh verify  --environment demo --set <set名>
bin/backup.sh restore --environment demo --set <set名> --yes
bin/docker-deploy.sh install --environment demo
```

## 3. 升级失败回到旧版本

`bin/docker-deploy.sh rollback` 幂等重装当前 bundle（bundle 无 release.sh）。回退到旧版本请用 `bin/docker-deploy.sh upgrade --environment <env> --image-tag <旧版本>`。

## 4. 上报前必须附上的证据

1. `bin/doctor.sh --environment demo --export ./incident` 的报告文件；
2. `bin/docker-deploy.sh logs --environment demo --tail 500 --export ./incident` 的压缩包；
3. `bin/config.sh show --environment demo`（已脱敏）。
