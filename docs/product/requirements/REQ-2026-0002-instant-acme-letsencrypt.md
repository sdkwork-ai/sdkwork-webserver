# REQ-2026-0002 instant-acme Let's Encrypt 自动签发

```yaml
id: REQ-2026-0002
title: instant-acme + Let's Encrypt 自动签发与 webserver_certificate 状态机
owner: SDKWork maintainers
status: draft
source: platform
problem: 租户站点需要免费 TLS 证书自动签发与续期，且控制面需统一状态机，不能依赖 Certbot 等外部进程作为默认路径。
goals:
  - certificates.issue 触发 instant-acme 向 LE（staging/prod）申请 DV 证书
  - certType=3 使用 rcgen 自签，无需触网
  - 自动续期仅适用于 certType=1 ACME 证书；自签名证书仅支持显式手动重签
  - webserver_certificate 状态与 renewal_status 正确流转
  - PEM 落地路径对齐 NGINX_SPEC.md
  - API 响应不含私钥
non_goals:
  - DNS-01 wildcard（REQ 后续）
  - Certbot/acme.sh 子进程默认集成
users:
  - 租户开发者
  - 平台运维
acceptance_criteria:
  - LE staging 环境下 HTTP-01 完成签发并写入 webserver_certificate
  - rcgen 自签 certType=3 在 dev  profile 可用
  - certType=3 开启自动续期返回标准校验错误，自动续期扫描只调度 certType=1
  - 签发结果的 SAN、算法、有效期、私钥配对及元数据必须与实际 X.509 叶证书一致
  - 签发失败 renewal_status=3 且可审计
  - TECH_ARCHITECTURE §2.1 开源栈与 ADR-20260623-acme-certificate-authority 一致
non_functional_requirements:
  security: 私钥 DB 加密；SECURITY_SPEC.md 脱敏
  privacy: 证书元数据 tenant 隔离
  performance: 单次签发 P95 < 120s（HTTP-01）
  reliability: staging rate limit 可重试
affected_surfaces:
  - backend
  - api
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - SECURITY_SPEC.md
    - NGINX_SPEC.md
  components:
    - crates/sdkwork-webserver-acme-service
    - crates/sdkwork-intelligence-webserver-service
verification:
  - cargo test -p sdkwork-webserver-acme-service
  - pnpm verify
```

See [PRD §4.2](../PRD.md) and [ADR-20260623-acme-certificate-authority](../../architecture/decisions/ADR-20260623-acme-certificate-authority.md).
