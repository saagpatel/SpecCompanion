import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

test("@a11y executable evidence workflow keeps placeholder tests UNKNOWN", async ({
  page,
}, testInfo) => {
  await page.goto("/");

  const newProject = page.getByRole("button", { name: "New Project" });
  await newProject.click();
  await expect(page.getByRole("dialog", { name: "New Project" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(newProject).toBeFocused();

  await newProject.click();
  await page.getByLabel("Project Name").fill("Evidence Workflow");
  await page.getByLabel("Codebase Path").fill("/preview/javascript-fixture");
  await page.getByRole("button", { name: "Create", exact: true }).click();
  await page.getByRole("link", { name: /Evidence Workflow/ }).click();

  await page.getByRole("button", { name: "Upload Spec" }).click();
  await expect(page.getByText("qa-spec.md")).toBeVisible();
  await page.getByRole("link", { name: "Generate Tests" }).click();
  await page.getByRole("button", { name: "Select All" }).click();
  await page.getByRole("button", { name: /Generate \(4\)/ }).click();
  await expect(page.getByText(/Offline templates are editable scaffolds/)).toBeVisible();

  await page.getByRole("link", { name: "Evidence Workflow" }).click();
  await page.getByRole("link", { name: "Run Tests" }).click();
  await page.getByRole("button", { name: "Select All" }).click();
  await page.getByRole("button", { name: /Run \(4\)/ }).click();
  await page
    .getByRole("button", { name: /passed/ })
    .first()
    .click();
  await expect(page.getByText("Mock browser preview execution passed").first()).toBeVisible();

  await page.getByRole("link", { name: "Evidence Workflow" }).click();
  await page.getByRole("link", { name: "Reports" }).click();
  await page.getByRole("button", { name: "Generate Report" }).click();
  await expect(page.getByRole("heading", { name: "Verify an evidence bundle" })).toBeVisible();
  await page.getByLabel("Choose bundle JSON").setInputFiles({
    name: "evidence-bundle.json",
    mimeType: "application/json",
    buffer: Buffer.from("{}"),
  });
  await expect(page.getByText(/Bundle status: unsupported/i)).toBeVisible();
  await expect(page.getByText(/never added to this project/i)).toBeVisible();
  await page.getByLabel("Choose bundle JSON").setInputFiles({
    name: "signed-evidence-bundle.json",
    mimeType: "application/json",
    buffer: Buffer.from("preview-signed"),
  });
  await expect(page.getByText(/Bundle status: signed_untrusted/i)).toBeVisible();
  await expect(page.getByLabel("Trust decision provenance")).toBeVisible();
  await expect(page.getByRole("button", { name: "Mark trusted" })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Mark revoked" })).toBeDisabled();
  await expect(page.getByText(/Report integrity: not checked/i)).toBeVisible();
  await expect(page.getByText(/cannot be treated as tamper-evident/i)).toBeVisible();
  await expect(page.getByRole("button", { name: "Evidence bundle" })).toBeVisible();
  await expect(page.getByText(/unsigned and do not prove authorship/i)).toBeVisible();
  await expect(page.getByRole("heading", { name: "Optional signed bundle" })).toBeVisible();
  await expect(page.getByText(/identity label remains untrusted/i)).toBeVisible();
  await expect(page.getByLabel("Signer identity label")).toBeVisible();
  await expect(page.getByRole("button", { name: "Create Keychain identity" })).toBeDisabled();
  await expect(page.getByText("0/4 requirements verified")).toBeVisible();
  await expect(page.getByText("UNKNOWN").first()).toBeVisible();
  await expect(page.getByText(/placeholder assertion is not evidence/i).first()).toBeVisible();
  await page
    .getByRole("button", { name: /Show 3/ })
    .first()
    .click();
  await expect(page.getByRole("heading", { name: "Verification policy" }).first()).toBeVisible();
  await expect(
    page.getByText(/browser preview cannot evaluate native execution enforcement/i).first(),
  ).toBeVisible();
  await expect(page.getByText(/platform=macos/i).first()).toBeVisible();
  await expect(page.getByText(/tautology is non-probative/i).first()).toBeVisible();

  if (testInfo.project.name === "mobile") {
    const layout = await page.evaluate(() => {
      const main = document.querySelector("main");
      return {
        viewport: document.documentElement.clientWidth,
        documentWidth: document.documentElement.scrollWidth,
        mainWidth: main?.clientWidth ?? 0,
        mainScrollWidth: main?.scrollWidth ?? 0,
      };
    });
    expect(layout.viewport).toBe(390);
    expect(layout.documentWidth).toBe(390);
    expect(layout.mainWidth).toBeGreaterThan(300);
    expect(layout.mainScrollWidth).toBe(layout.mainWidth);
  }

  const accessibility = await new AxeBuilder({ page }).analyze();
  const blocking = accessibility.violations.filter(
    (violation) => violation.impact === "critical" || violation.impact === "serious",
  );
  expect(blocking).toEqual([]);
});
