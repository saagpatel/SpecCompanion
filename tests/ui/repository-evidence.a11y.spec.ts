import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

test("@a11y explicitly links contained repository evidence without overclaiming preview verification", async ({
  page,
}) => {
  await page.goto("/");

  await page.getByRole("button", { name: "New Project" }).click();
  await page.getByLabel("Project Name").fill("Repository Evidence");
  await page.getByLabel("Codebase Path").fill("/preview/javascript-fixture");
  await page.getByRole("button", { name: "Create", exact: true }).click();
  await page.getByRole("link", { name: /Repository Evidence/ }).click();

  await page.getByRole("button", { name: "Upload Spec" }).click();
  await page.getByRole("link", { name: "Generate Tests" }).click();
  await expect(page.getByText("Existing repository evidence")).toBeVisible();

  await page.getByLabel("Requirement").selectOption({ index: 1 });
  await page.getByLabel("Repository test").selectOption("tests/model.test.ts");
  await page.getByRole("button", { name: "Link test evidence" }).click();
  await expect(
    page.getByRole("status").filter({ hasText: /Linked .*model\.test\.ts/ }),
  ).toBeVisible();
  await expect(page.getByText(/vitest \| repository link/)).toBeVisible();

  await page.getByRole("link", { name: "Repository Evidence" }).click();
  await page.getByRole("link", { name: "Run Tests" }).click();
  await expect(page.getByRole("heading", { name: /Executable evidence \(1\)/ })).toBeVisible();
  await page.getByRole("button", { name: "Select All" }).click();
  await page.getByRole("button", { name: "Run (1)" }).click();
  await expect(page.getByRole("button", { name: /passed/ })).toBeVisible();

  await page.getByRole("link", { name: "Repository Evidence" }).click();
  await page.getByRole("link", { name: "Reports" }).click();
  await page.getByRole("button", { name: "Generate Report" }).click();
  await expect(page.getByText("UNKNOWN").first()).toBeVisible();
  await expect(
    page.getByText(/browser preview cannot scan implementation evidence/i),
  ).toBeVisible();

  const accessibility = await new AxeBuilder({ page }).analyze();
  const blocking = accessibility.violations.filter(
    (violation) => violation.impact === "critical" || violation.impact === "serious",
  );
  expect(blocking).toEqual([]);
});
