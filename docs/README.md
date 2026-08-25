# SDKWork Web Server Documentation

This directory contains the current product, architecture, operations, and decision authority for
`sdkwork-webserver`. Global standards remain in `../sdkwork-specs`; repository documents link to
those standards instead of copying normative text.

| Document | Purpose |
| --- | --- |
| [product/prd/PRD.md](product/prd/PRD.md) | Product scope, behavior, and commercial release gates |
| [architecture/tech/TECH_ARCHITECTURE.md](architecture/tech/TECH_ARCHITECTURE.md) | Current runtime and module architecture |
| [product/prd/PRD-cloud-site-delivery-data-plane.md](product/prd/PRD-cloud-site-delivery-data-plane.md) | Live Drive/Wiki cloud delivery product contract |
| [architecture/tech/TECH-cloud-site-delivery-data-plane.md](architecture/tech/TECH-cloud-site-delivery-data-plane.md) | Compiled descriptor and provider data-plane design |
| [architecture/tech/TECH-app-domain-publishing-fallback.md](architecture/tech/TECH-app-domain-publishing-fallback.md) | User app publishing domains (default `<slug>.app.<suffix>` + custom) and the Deploy control-plane fallback for unmatched hosts |
| [standards-alignment.md](standards-alignment.md) | Current SDKWork integration and verification evidence |
| [engineering/reviews/REVIEW-20260731-domain-certificate-deployment-data-model.md](engineering/reviews/REVIEW-20260731-domain-certificate-deployment-data-model.md) | Current Web/Deploy/IAM domain, certificate, deployment, and database ownership review |
| [engineering/reviews/REVIEW-20260723-webserver-production-readiness.md](engineering/reviews/REVIEW-20260723-webserver-production-readiness.md) | Consolidated implementation, configuration, deployment coverage, verification, and production gate review |
| [architecture/decisions/](architecture/decisions/) | Accepted and proposed architecture decisions |
| [product/requirements/](product/requirements/) | Requirement contracts and their verification evidence |

Operational deployment instructions live under `../deployments/`; source runtime configuration is
documented by `../etc/README.md`.
