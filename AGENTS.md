## UI Hard Gates (Required for frontend/UI changes)

<!-- comm-contract:start -->

## Communication Contract (Global)

- Follow `/Users/d/.codex/policies/communication/BigPictureReportingV1.md` for all user-facing updates.
- Use exact section labels from `BigPictureReportingV1.md` for default status/progress updates.
- Keep default updates beginner-friendly, big-picture, and low-noise.
- Keep technical details in internal artifacts unless explicitly requested by the user.
- Honor toggles literally: `simple mode`, `show receipts`, `tech mode`, `debug mode`.
<!-- comm-contract:end -->

1. Read-only reviewer agent must output `UIFindingV1[]`.
2. Fixer agent must apply findings in severity order: `P0 -> P1 -> P2 -> P3`.
3. Required states per changed UI surface: loading, empty, error, success, disabled, focus-visible.
4. Required pre-done gates:
   - eslint + typecheck + stylelint
   - visual regression (Playwright snapshots)
   - accessibility regression (axe)
   - responsive parity checks (mobile + desktop)
   - Lighthouse CI thresholds
5. Done-state is blocked if any required gate is `fail` or `not-run`.
