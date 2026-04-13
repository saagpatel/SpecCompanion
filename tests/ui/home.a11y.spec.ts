import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

function formatBlockingViolations(
  violations: Array<{ id: string; impact?: string | null; nodes: Array<{ target: string[] }> }>,
): string {
  if (violations.length === 0) return "No blocking axe violations.";
  return violations
    .map((violation) => {
      const firstTarget = violation.nodes[0]?.target?.join(" > ") ?? "unknown target";
      return `${violation.id} (${violation.impact ?? "unknown"}) at ${firstTarget}`;
    })
    .join("\n");
}

test("@a11y home page", async ({ page }) => {
  await page.goto("/");
  const results = await new AxeBuilder({ page }).analyze();
  const blockingViolations = results.violations.filter(
    (violation) => violation.impact === "critical" || violation.impact === "serious",
  );
  expect(blockingViolations, formatBlockingViolations(blockingViolations)).toEqual([]);
});
