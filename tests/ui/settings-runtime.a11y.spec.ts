import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

test("@a11y Python runtime trust is project-scoped, explicit, and explained", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "New Project" }).click();
  await page.getByLabel("Project Name").fill("Python Runtime Trust");
  await page.getByLabel("Codebase Path").fill("/preview/python-fixture");
  await page.getByRole("button", { name: "Create", exact: true }).click();
  await page.getByRole("link", { name: /Python Runtime Trust/ }).click();
  await page.getByRole("link", { name: "Run Tests" }).click();

  const runtime = page.getByLabel("External environment root");
  await expect(runtime).toBeVisible();
  await expect(page.getByText(/never installs packages/i)).toBeVisible();
  await expect(page.getByText(/scoped to this project/i)).toBeVisible();
  await expect(page.getByText(/package inventory changes/i)).toBeVisible();
  await expect(page.getByText(/platform-bound isolation receipt/i)).toBeVisible();
  await expect(page.getByText(/only macOS sandbox-exec is recognized/i)).toBeVisible();

  await runtime.fill("/Users/example/.virtualenvs/requirements");
  await runtime.press("Tab");
  await page.getByRole("button", { name: "Trust runtime" }).press("Enter");
  await expect(page.getByText("Attestation matches", { exact: true })).toBeVisible();
  await expect(page.getByRole("status")).toContainText("attestation matches");

  const accessibility = await new AxeBuilder({ page }).analyze();
  const blocking = accessibility.violations.filter(
    (violation) => violation.impact === "critical" || violation.impact === "serious",
  );
  expect(blocking).toEqual([]);
});
