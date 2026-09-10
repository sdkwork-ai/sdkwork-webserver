# 容量规划 Runbook（sdkwork-webserver）

规范：PRD §8.1（性能与内存目标）、REQ-2026-0027/0033/0034（资源压力准入、运行指标）。
适用症状：内存/CPU 逼近上限、拒绝计数上升、上游容量饱和、连接数逼近上限。

## 1. 观测入口

```bash
# 数据面运行指标（loopback，固定基数；RED + 容量）
curl -s http://127.0.0.1:3901/metrics | grep -E "connection|request|upstream|pressure|reject"

# 管理面指标
curl -s http://127.0.0.1:3800/metrics

# 只读环境诊断（9 项，exit 70 = FAIL）
bin/doctor.sh            # --json / --export 可机读
```

关键序列：`connections_active`、`requests_active`（准入许可占用）、
`requests_rejected_total`（按拒绝类别）、`upstream_request_capacity` /
`upstream_physical_connections`、`resource_pressure`（Windows/Linux 采样，
带迟滞的准入门）、响应/上游延迟直方图。

## 2. 容量基线（配置 → 预算）

| 维度 | 配置键 | 调整原则 |
| --- | --- | --- |
| 连接数 | `limits.maxConnections` | 每连接内存预算 × 数 ≤ 进程内存预算的 ~60% |
| 并发请求 | `limits.maxConcurrentRequests` | 非排队准入；配合 `operationsReserveRequests` 预留管理余量 |
| 上游物理连接 | upstream `maxConnections` / target 上限 | 上游总预算 ≤ 出口 fd/内存预算 |
| 数据库连接池 | `SDKWORK_DATABASE_MAX_CONNECTIONS` | ≈ (总内存预算内) max_connections = 2×vCPU + 磁盘吞吐经验值 |
| 容器限制 | compose `deploy.resources` / systemd `MemoryMax` | 兜底，避免分配器耗尽；留 20-30% 余量 |
| 用量计量 | `usageMetering.maxBuckets` | 默认 65536 bucket；hostname 高熵流量下调 |

资源压力准入（`deployment.resourcePressure`）开启后，进程在到达 OS 耗尽前
开始有界拒绝/ shedding——这是 PRD §8.1 的"emergency margin"机制，生产建议开启。

## 3. 扩容路径

- **纵向**：提高连接/请求预算 + 容器/systemd 限制同比例放大；确认 fd 上限
  （`LimitNOFILE=65536` / compose `ulimits.nofile`）。
- **横向（云数据面）**：按"节点身份"模型渲染新 Node（见
  deployments/kubernetes/README.md §Scaling Contract），扩容入口层而非克隆节点。
- **横向（standalone 管理面）**：当前为单主机通道；升级/扩容遵循
  failed-rollout.md 的维护窗口契约。

## 4. 压测与浸泡（PRD §8.1 证据缺口）

当前仓库尚无负载/浸泡基准（PRD Phase 3 验收项）。在建立官方基准前：
- 变更容量相关配置后，用 `scripts/webserver-release-smoke.mjs` 做功能冒烟；
- 记录 24h `connections_active` / RSS 曲线确认无单调增长（ reload、证书轮换、
  断连客户端、失败上游场景）。
