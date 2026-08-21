# ADR-20260623-acme-certificate-authority

Status: accepted
Requirement: REQ-2026-0002
Owner: SDKWork maintainers
Date: 2026-06-23
Updated: 2026-08-03 (self-hosted data plane semantics, durable accounts, revocation, ARI)
Specs: ARCHITECTURE_DECISION_SPEC.md, SECURITY_SPEC.md, SUPPLY_CHAIN_SECURITY_SPEC.md

## Context

SDKWork Web Server 需要在控制面内嵌 **免费 TLS 证书自动签发与续期**，并支持向自建数据面节点分发。产品要求快速落地、少运维依赖、与现有 Rust/Tokio 栈一致，证书由自建服务器（`sdkwork-api-webserver-standalone-gateway` 数据面 TLS 运行时）直接消费，不依赖外部 Nginx 边沿。

候选方案：

1. **Shell 调用 Certbot**：生态成熟，但是独立进程、状态分散、容器内耦合 Python/插件，不利于控制面统一状态机。
2. **Shell 调用 acme.sh / lego**：Go 单二进制，仍属外部进程，账户与订单状态需额外同步。
3. **Rust 内嵌 ACME 客户端（instant-acme）**：纯 Rust、async、RFC 8555，与现有 Tokio 服务同进程，账户凭证可序列化持久化，支持 ARI 续期扩展与证书撤销。
4. **rustls-acme / tokio-rustls-acme**：适合单服务自签 TLS，不适合多租户证书编排与 DB 状态机。

## Decision

1. **默认 CA**：生产使用 [Let's Encrypt](https://letsencrypt.org/)（免费、ACME、ISRG 根）；开发/联调使用 Let's Encrypt **Staging** 目录 URL。
2. **ACME 客户端库**：控制面采用 **[instant-acme](https://github.com/djc/instant-acme)**（async、纯 Rust、RFC 8555，MIT/Apache-2.0）。
3. **自签名（开发/内网）**：采用 **[rcgen](https://github.com/rustls/rcgen)** 生成 `certType=3` 证书，不触网。
4. **TLS 信任链与存储格式**：链路与节点落地使用 PEM；所有签发路径在统一出口使用 **x509-parser** 重新解析叶证书，并验证请求 SAN/算法、当前有效期、PKCS#8 私钥与叶证书 SPKI 配对以及返回元数据一致性后才允许持久化。
5. **V1 验证方式**：**HTTP-01**。挑战 token 由证书 worker 原子写入 webroot，自建数据面监听器通过 **窄优先级端点**（`acmeHttp01.webroot` 配置驱动）只服务精确的 `/.well-known/acme-challenge/<token>` 路径：不暴露目录、不接受任意 token、不覆盖其他路由、挑战结束后清理。DNS-01 与 wildcard 延后至 Phase 3。
6. **不引入 Certbot/acme.sh 运行时依赖** 作为 V1 默认路径；若治理批准，可作为灾备运维工具，但不写入产品默认架构。
7. **证书落地自建 TLS 运行时**：签发/续期成功后，worker 将节点 listener 绑定投影为版本化 TLS 材料（`material_root/<version-uuid>/fullchain.pem + privkey.pem`）与单调 `tls-runtime.json` 快照；数据面 `FileTlsRuntimeController` 轮询快照并热加载 Rustls 配置（A/B 恢复槽、指纹/有效期/SNI 校验）。外部 Nginx 边沿激活仅保留为文档标注的可选遗留路径，不参与证书生命周期。
8. **持久化 ACME 账户**：账户凭证经主密钥派生密钥 AES-256-GCM 加密后按 CA directory 写入 `SDKWORK_WEBSERVER_ACME_ACCOUNT_ROOT`（0600、原子写），签发/续期/撤销/ARI 复用同一账户，避免 LE 账户创建限流并保留账户身份。
9. **撤销与 ARI**：`POST /certificates/{certificateId}/revoke` 同步撤销（CA 确认后才本地标记 `status=3`，归档 listener 绑定，停止自动续期）；签发成功后查询 CA 建议续期窗口（RFC 9773 ARI）并记录，调度优先使用 ARI 窗口，回退固定 `renew_before_days`。

实现归属：

- `sdkwork-webserver-acme-service`：ACME 账户、订单、续期、撤销、ARI 编排。
- `sdkwork-webserver-certificate-worker`：后台续期与到期扫描 job + 节点 TLS 材料投影发布。
- 私钥：证书签发后写入受限证书根目录，PostgreSQL 的不可变证书版本只保存 `secret_bundle_ref` 与可公开验证的 X.509 元数据。解析器拒绝目录逃逸、符号链接、超限内容及非法 PEM；私钥不得进入数据库、API、日志或审计载荷。外部 Secret Manager/KMS 通过相同解析边界扩展。

## Alternatives

| 方案 | 优点 | 缺点 | 结论 |
| --- | --- | --- | --- |
| Certbot 子进程 | 运维熟悉 | 多进程、状态分裂、镜像臃肿 | 不采用为默认 |
| lego CLI | Go 单文件 | 外部进程、租户状态难统一 | 不采用为默认 |
| instant-acme 内嵌 | 与 Rust 栈一致、可测试、可审计 | 需自实现 HTTP-01 协作 | **采用** |
| rustls-acme | 接入简单 | 不适合多租户 DB 生命周期 | 仅参考 |

## Consequences

- Cargo workspace 新增 `instant-acme`、`rcgen` 依赖；需在 `SUPPLY_CHAIN_SECURITY_SPEC.md` 流程中登记 license 与版本 pin。
- `certificates.issue` 持久化异步 ACME 操作并返回 HTTP `202` 标准异步数据；完成后写入不可变证书版本、更新 `web_certificate` 聚合并触发节点 TLS 材料发布。
- HTTP-01 要求自建数据面监听器配置 `acmeHttp01.webroot`，且 worker 的 `SDKWORK_WEBSERVER_ACME_WEBROOT` 指向同一目录；验证窗口内该监听器必须公网可达。
- ACME 证书自动续期默认在到期前 30 天启动（ARI 窗口优先）；失败写入 `renewal_status=3` 并告警。自签名证书仅支持显式手动重签，不进入自动续期扫描。
- Staging CA 签发证书不受浏览器信任，仅用于联调；生产 profile 必须显式指向 LE 生产目录。
- 生产-like 环境必须配置 `SDKWORK_WEBSERVER_ACME_ACCOUNT_ROOT`（账户持久化）与 `SDKWORK_WEBSERVER_NODE_UUID`（TLS 材料分发），否则启动失败或跳过分发并告警。

## Verification

- 单元测试：acme-service 自签/材料校验/账户存储/挑战存储/revoke 原因/ARI 标识派生。
- 集成测试：`pebble_lifecycle.rs`（`#[ignore]`，需本地 pebble + pebble-challtestsrv）对本地 ACME CA 完成完整 HTTP-01 签发闭环、账户持久化复用断言；数据面 `data_plane_integration.rs` 验证 HTTP-01 窄优先级端点行为。
- 仓库 parity 测试：enqueue/claim/finalize/续期调度（ARI 窗口优先）/撤销/租约围栏。
- `pnpm verify` 与 `cargo test --workspace` 通过。

## Supersedes / Superseded By

- Supersedes: none
- Superseded By: none
