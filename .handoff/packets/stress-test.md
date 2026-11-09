# Read-only stress test: Living Spec + Quantifier Map against the real SpecCompanion repo

You are reviewing a plan, not building. Do NOT edit any tracked file. Write your findings ONLY to `.handoff/stress-test-findings.md` and include that single file in the commit plan.

Read: `.handoff/packets/00-architecture.md` (the plan's binding contract), `.handoff/packets/01-core-spec-store.md`, `README.md`, `AGENTS.md`, `src-tauri/src/services/{spec_parser.rs,alignment.rs,evidence.rs,repository_tests.rs,test_runner.rs}`, `src-tauri/src/models/spec.rs`.

Planned capabilities: (1) Living Spec core: plain-English requirement lines with stable ids, linked to tests by stable test identity, badges proven/broken/unproven(/qualified), per-line history, a deterministic linker and auditor (the auditor must catch a passing test that proves nothing), a runner hook that rescores on each push and exits non-zero when a line goes red, a static web view and a portfolio roll-up that reads the portfolio-truth registry. (2) Boundary Cases: typed numeric/domain bounds parsed from quantified requirements, below/at/above examples, unit-contradiction rejection, coverage map naming untested boundaries, 30 requirement fixtures. (3) Dependency Proofs: authored dependencies, cycle detection, qualified-status propagation with minimal path, 20 synthetic graphs. (4) Quantifier Map: domain + quantifier kind (existential, finite-universal exhaustive, infinite-universal sampled), coverage view with no proof language, readable edition export, counterexample export with failing sub-domain, 30 synthetic specs.

Answer concretely, each with a verdict (fine / mitigated by X / scope cut needed):
1. Does building Living Spec as a standalone crate `living-spec/` (not inside `src-tauri`) fight the existing requirement model? What existing SpecCompanion concepts (stable requirement identity, placeholder-assertion detection, VERIFIED/PARTIAL/FAILED/UNKNOWN) should the new crate mirror or reuse so the two do not contradict each other? Name exact functions/rules worth porting (e.g. placeholder assertion patterns) with file:line.
2. Is the badge rule order in the contract sound? Find an input where it yields a misleading badge.
3. Stable test identity via id + whitespace-collapsed source fingerprint: failure modes (duplicate bodies, parametrized tests, libtest names) and the minimal fix.
4. Boundary model: which requirement phrasings will the parser realistically handle offline, and which should the 30 fixtures avoid or mark as needing an explicit annotation? How should unit contradiction be detected when a test's unit is only implied?
5. Quantifier Map: how should a claim's predicate be executed offline for 30 synthetic specs without an LLM or arbitrary code execution? Recommend a concrete design.
6. Anything in the existing repo (CI on macOS runners, lockfile rules, git guards in `pnpm git:guard:all`, `.husky`, commitlint) that a new top-level `living-spec/` dir or new CI job would trip. Check `scripts/git/` and `package.json` scripts.
7. Top 5 risks overall, ranked, each with the scope cut or mitigation you recommend.

Keep it under 900 words. Acceptance: `.handoff/stress-test-findings.md` exists and answers all 7.
