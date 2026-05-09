# SpecCompanion .codex command map

| Action           | Command               | Source                                 |
| ---------------- | --------------------- | -------------------------------------- |
| setup deps       | `pnpm install`        | `README.md`                            |
| lint/static gate | `pnpm ui:gate:static` | `package.json`                         |
| test             | `pnpm verify`         | `package.json`                         |
| workflow smoke   | `pnpm workflow:smoke` | `package.json`, `src-tauri/src/lib.rs` |
| build            | `pnpm run build`      | `README.md`, `package.json`            |
| lean dev         | `pnpm run dev:lean`   | `README.md`, `package.json`            |
