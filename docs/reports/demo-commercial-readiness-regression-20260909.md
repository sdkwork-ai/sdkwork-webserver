# demo 商业化落地 — 全面回归健康评估与技术债务清单

日期：2026-09-09 深夜　范围：demo（http://server-demo.sdkwork.com:19098/）全面回归，识别功能性缺口与历史技术债务

## 一、demo 真实用户链路（已确认端到端健康）
```
浏览器 → webserver 边缘 (sdkwork-webserver-demo-i1-webserver-1, 0.1.3, healthy, :19098→80)
       → bundled gateway (sdkwork-webserver-demo-gateway-gateway-1, healthy, /healthz 200 + /readyz 200)
       → demo DB (sdkwork_ai_demo@192.168.31.116:5432)
```
- 前端：demo 控制台启动修复（pc-core 枚举含 demo）已固化进 0.1.3 镜像并持久生效，`index-KWbP0Pq8.js` allow-list 含 demo；messagingPcUrl 指向 messaging-demo 域。
- API：bundled gateway `/healthz`、`/readyz` 全 200，无错误日志；模块 sidecar（import.conf `nginx.standalone.demo.conf`）upstream=gateway → 解析 172.30.0.4 = bundled gateway。
- 数据库：42 个模块全部迁移完成、1095 张业务表、种子数据齐全（iam_user=2：admin/Administrator+system；iam_tenant=1；iam_organization=1）。**非空壳**。
- 对外域名：dev/test/staging/demo/prod 控制台 HTTP 全 200。

## 二、demo 已修复并落地项（本会话前半）
1. pc-core 生命周期枚举 + readEnum 加 demo（根因 `environment is invalid`）。
2. materialize/entrypoint 等 6 文件 demo 支持，发布 0.1.3 并重部署 demo（持久固化）。
3. demo 全工作区 wave1 枚举缺口修复（messaging/deployments/agents/company/community/memory + webserver build 门禁，源码级）。

## 三、发现的技术债务（商业化落地前需治理）

### 【债务 A · 高 · 已下线】独立 api-cloud-gateway 冗余栈（5 环境普遍 unhealthy）
> ✅ **已治理（2026-09-09 深夜，用户确认下线）**：已 `docker compose -p sdkwork-api-cloud-gateway-<env>-<i1|i2|deps> down` 下线 5 环境 × 3 = **15 个容器全部**（保留 data/secrets 数据卷可回滚）。下线后 5 环境 webserver bundled gateway + 边缘全 healthy，demo 主链路 HTTP 200。拓扑收敛为 webserver bundled gateway 单一路径。
- 现象：`sdkwork-api-cloud-gateway-{env}-i1/i2-gateway-1`(+deps) 在 dev/test/staging/demo/prod **全部存在**，今天 10:04 启动，**大部分 unhealthy**。
- 根因：i1+i2 **多实例竞争同一 skills Snowflake node lease** → readiness 持续 `skills Snowflake node lease is unhealthy`（503）。`SDKWORK_IM_ID_NODE_ID` 已区分（demo i1=25/i2=26），但 skills snowflake 租约仍竞争。
- 证据：**零真实业务流量**——独立栈 3h 内 gateway_request 全为 /readyz 健康检查；webserver import.conf 实际 dial bundled gateway（172.30.0.4），独立栈(192.168.224.x)不同网、未被 dial。
- 影响：纯空转 + readiness 假警报 + 资源/内存占用；**不阻塞 demo 主链路**（bundled gateway 健康）。
- 判断：这是 webserver「docker/bundled 模式」（`SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=docker`+`HOST=gateway`）之外的 **external/attach 模式过渡产物**，两套并存未收敛。
- 治理选项（需架构决策）：① 若 demo 商业化只需 bundled gateway（same-origin /api），则下线该独立栈（5 环境 × 3 容器），消除噪音与资源浪费；② 若 demo 需独立 external API 入口（api-*.domain 供移动端/桌面/第三方），则保留并**修复多实例 snowflake lease 竞争**（明确 node 身份/去 i2 或改单实例）。

### 【债务 B · 中】webserver 后端 OpenAPI 契约仍未含 demo
- `sdks/sdkwork-webserver-app-sdk|backend-sdk/.../sdkgen.yaml` 环境 enum 仍 4 值 → 生成的 SDK `CreateDeploymentRequest` 只收 4-env。
- 影响：webserver console/admin 的「应用发布/部署到 demo」动作被编译层卡死（TS2322），无法把应用部署到 demo。
- 治理：扩 sdkgen enum → 5-env → 重新生成 SDK → Rust 端点接收 demo → 放开 UI（pc-console-core/data-source deploymentEnvironment + 下拉选项）。

### 【债务 C · 中】demo 枚举缺口 wave2/3（打包门禁/构建正则/纯类型/静默回退）
- 工作区打包门禁 `bin/package-docker-bundles.sh`：GATEWAY_ENVS 4 值、WEBSERVER_ENVS 甚至缺 staging（development test production）；JSON environments 字段同步。
- 各 app vite.config 环境正则/数组缺 demo（agents/birdcoder/kernel/gameengine/terminal 等）→ `cloud.demo` 模式可能误回退。
- 纯类型/静默回退：sdkwork-core/image/manager/github/news(flutter) 等 resolveRuntimeEnv demo 静默回退 development。
- 测试遍历脚本补 demo。

### 【债务 D · 低】独立栈以外的运行噪音
- gateway 内 `im.security outbox_scope_discovery` worker 高频刷日志（scope_count=0 循环），信息量大但无害，可考虑降噪。

## 四、demo 可用种子账户（供登录验证）
- 用户：`admin` / admin@sdkwork.com / Administrator / active / tenant 100001

## 五、建议治理优先级（供决策）
1. 【A】先定独立 gateway 栈去留（demo 商业化拓扑收敛）——需你确认。
2. 【B】webserver 后端 sdkgen 契约扩 demo + 重生成 SDK（解锁"部署应用到 demo"）。
3. 【C】wave2/3 枚举补齐（纯加 demo、零风险，可批量）。
4. demo 真实登录/业务端到端（用 admin 种子账户走一遍）——在拓扑收敛后进行。
