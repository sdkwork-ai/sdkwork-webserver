# REQ-2026-0069 Deferred Application Source And Listing

```yaml
id: REQ-2026-0069
title: Create applications with identity only and attach source later from the list
owner: sdkwork-webserver
status: implemented
source: operator
problem: Console and Admin create-application wizards still walk operators through media, listing, source, and deployment before a record exists. Operators need a named application first and must add store listing and source when those assets are ready.
goals:
  - Require only application name (plus type defaults) to create a Console or Admin application.
  - Keep store media, listing copy, source package/Git, and deployment configuration optional at create time.
  - Expose labeled Add source / Modify source actions on each application list row.
  - Block publish until at least one source version exists.
non_goals:
  - Changing IAM permission catalogs or generated SDK method names.
  - Making store listing optional on later Edit once an operator chooses to submit listing assets.
  - Replacing the existing five-step wizard for operators who still want to complete everything in one pass.
users:
  - tenant Console application owners
  - Backend Admin application operators
acceptance_criteria:
  - Console and Admin create flows succeed when the operator submits name only from step 1.
  - Step 1 primary action is labeled Create now / 立即创建; Continue advances optional steps.
  - Optional wizard steps remain available through Continue and Skip for now.
  - Create without source does not call source-version or deployment APIs.
  - Application list operation column shows Add source code when no source version exists and Modify source code when one exists.
  - Application list Source code column shows No source yet / 未配置源码 or Source added / 已配置源码.
  - Publish is unavailable until the application has a source version.
  - Package-local zh-CN and en-US copy covers the deferred-create and source-row actions.

## Operator experience

### Create (Console and Admin)

1. Open **Create application** / **创建应用**.
2. Enter **Application name** only (type and runtime defaults apply).
3. Choose **Create now** / **立即创建** to save a draft application immediately.
4. Or choose **Continue** / **继续** to add store listing, source, and deployment in the same session.
5. Steps 2–4 (media, source, config) are marked optional in the wizard and may be skipped with **Skip for now** / **暂不设置**.

### Add or replace source later

1. In the application list, read the **Source code** / **源码** column (`No source yet` vs `Source added`).
2. Use the row action **Add source code** / **添加源码** when no version exists, or **Modify source code** / **修改源码** when one exists.
3. Upload ZIP, select a directory, or import an HTTPS Git repository with a version tag.
4. Publish remains blocked until at least one source version is stored.
non_functional_requirements:
  security: Source upload and Git import remain owner-scoped Drive and IAM write operations; UI labels never replace backend authorization.
  privacy: Listing and source dialogs do not display Drive credentials or private Git tokens.
  performance: List source-presence checks stay page-bounded (one latest-version lookup per visible row).
traceability:
  prd: PRD-FR-034, PRD-FR-035
  specs:
    - ../sdkwork-specs/REQUIREMENTS_SPEC.md
    - ../sdkwork-specs/FRONTEND_CODE_SPEC.md
    - ../sdkwork-specs/I18N_SPEC.md
    - ../sdkwork-specs/APP_SDK_INTEGRATION_SPEC.md
```
