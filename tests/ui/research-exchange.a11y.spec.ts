import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

test("@a11y research exchange preserves the desktop proof boundary", async ({ page }) => {
  await page.goto("/research");

  await expect(page.getByRole("heading", { name: "Inspect a research package" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Verify and re-evaluate" })).toBeDisabled();
  await expect(page.getByText(/Cryptographic qualification runs only in the desktop runtime/)).toBeVisible();

  const results = await new AxeBuilder({ page }).analyze();
  const blocking = results.violations.filter(
    (violation) => violation.impact === "critical" || violation.impact === "serious",
  );
  expect(blocking).toEqual([]);
});
