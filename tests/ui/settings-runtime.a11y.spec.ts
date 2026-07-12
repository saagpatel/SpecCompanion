import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

test("@a11y trusted Python runtime is an explicit, explained setting", async ({ page }) => {
  await page.goto("/settings");

  const runtime = page.getByLabel("Trusted Python environment (optional)");
  await expect(runtime).toBeVisible();
  await expect(page.getByText(/never installs packages/i)).toBeVisible();
  await expect(page.getByText(/validated again before every run/i)).toBeVisible();

  await runtime.fill("/Users/example/.virtualenvs/requirements");
  await runtime.press("Tab");
  await page.getByRole("button", { name: "Save Settings" }).press("Enter");
  await expect(page.getByRole("status")).toHaveText("Settings saved.");

  const accessibility = await new AxeBuilder({ page }).analyze();
  const blocking = accessibility.violations.filter(
    (violation) => violation.impact === "critical" || violation.impact === "serious",
  );
  expect(blocking).toEqual([]);
});
