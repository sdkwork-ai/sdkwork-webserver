# H5 Application Agent Instructions

## SDKWORK Soul

Read `../../../sdkwork-specs/SOUL.md` before executing tasks in this root. Follow specs before memory, dictionary before context, stop on ambiguity, and evidence before completion.

## SDKWORK Standards

Resolve this standards root once and use it as the global authority for the current task:

- `../../../sdkwork-specs/README.md`
- `../../../sdkwork-specs/SOUL.md`
- `../../../sdkwork-specs/AGENTS_SPEC.md`

Read only the relevant README task-matrix row or navigation heading, then load the selected authority sections.

Canonical SDKWORK specs path from this root:

- `../../../sdkwork-specs/README.md`
- `../../../sdkwork-specs/SOUL.md`
- `../../../sdkwork-specs/AGENTS_SPEC.md`
- `../../../sdkwork-specs/CODE_STYLE_SPEC.md`
- `../../../sdkwork-specs/NAMING_SPEC.md`
- `../../../sdkwork-specs/APP_H5_ARCHITECTURE_SPEC.md`
- `../../../sdkwork-specs/APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md`

Build scripts, dev runners, and `pnpm clean` must follow `CODE_STYLE_SPEC.md` §7.

## Application Identity

This is the H5 Adaptive Web surface for Web Server. Public-origin selection follows
`APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md` §2.1 (mobile → H5, fallback PC).

## Local Dictionary Structure

- `AGENTS.md`: local agent entrypoint
- `sdkwork.app.config.json`: H5 application manifest
- `etc/`: browser runtime profiles
- `src/`: thin H5 shell
- `.sdkwork/`: local skills and plugins

## Verification

```bash
pnpm typecheck
pnpm test
pnpm build:standalone
```
