# SpecCompanion .codex command map

| Action | Command | Source |
| --- | --- | --- |
| setup deps | `pnpm install` | `README.md` |
| lint fallback | `pnpm run build` | `package.json` (no dedicated lint script) |
| test | _none configured (blocks by default as NOT_RUN; bypass only with `CODEX_ALLOW_NOT_RUN_GATES=1` + explicit risk acceptance)_ | `README.md`, `package.json` |
| build | `pnpm run build` | `README.md`, `package.json` |
| lean dev | `pnpm run dev:lean` | `README.md`, `package.json` |
