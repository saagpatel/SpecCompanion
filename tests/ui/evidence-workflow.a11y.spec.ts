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
  await expect(page.getByText(/Bundle status: Unsupported evidence contract/i)).toBeVisible();
  await expect(page.getByText(/never added to this project/i)).toBeVisible();
  await page.getByLabel("Choose bundle JSON").setInputFiles({
    name: "signed-evidence-bundle.json",
    mimeType: "application/json",
    buffer: Buffer.from("preview-signed"),
  });
  await expect(
    page.getByText(/Bundle status: Signature valid; fingerprint not trusted/i),
  ).toBeVisible();
  await expect(page.getByLabel("Trust decision provenance")).toBeVisible();
  await expect(page.getByRole("button", { name: "Mark trusted" })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Mark revoked" })).toBeDisabled();
  await page.getByLabel("Trust decision provenance").fill("Verified in release ceremony SEC-42");
  await page.getByRole("button", { name: "Mark trusted" }).click();
  await expect(page.getByText(/Project trust policy updated: trusted/i)).toBeVisible();
  await expect(page.getByRole("list", { name: "Current signer trust policies" })).toContainText(
    "Preview signer",
  );
  await page.getByText(/Decision history \(1\)/).click();
  await expect(page.getByText(/Verified in release ceremony SEC-42/).last()).toBeVisible();
  await page.getByLabel("Choose bundle JSON").setInputFiles({
    name: "replacement-signed-evidence-bundle.json",
    mimeType: "application/json",
    buffer: Buffer.from("preview-signed-b"),
  });
  await expect(
    page.getByRole("heading", { name: "Rotate to the verified fingerprint" }),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: "Rotate trust atomically" })).toBeDisabled();
  await page.getByLabel("Currently trusted key").selectOption({ index: 1 });
  await page.getByRole("button", { name: "Rotate trust atomically" }).click();
  await expect(page.getByText(/Rotation recorded/i)).toBeVisible();
  await expect(page.getByRole("list", { name: "Current signer trust policies" })).toContainText(
    "Preview replacement signer",
  );
  await expect(page.getByRole("button", { name: "Export signed trust policy" })).toBeDisabled();
  await page.getByLabel("Keychain signing identity").fill("Preview recovery signer");
  await expect(page.getByRole("button", { name: "Export signed trust policy" })).toBeEnabled();
  await page.getByLabel("Verify recovery policy JSON").setInputFiles({
    name: "unauthorized-signer-trust-policy.json",
    mimeType: "application/json",
    buffer: Buffer.from("preview-signed-trust-policy"),
  });
  await expect(page.getByText(/Destination recovery authorization: unknown/i)).toBeVisible();
  await page.getByLabel("Confirm package signer fingerprint").fill("c".repeat(64));
  await page.getByLabel(/Confirm add Recovered signer as trusted/).check();
  await page.getByLabel("Recovery verification provenance").fill("Copied from the package itself");
  await expect(
    page.getByRole("button", { name: "Apply authorized signed policy changes" }),
  ).toBeDisabled();
  await page.getByLabel("Independently verified recovery fingerprint").fill("c".repeat(64));
  await page.getByLabel("Operator label for this authority").fill("Disaster recovery authority");
  await page
    .getByLabel("Independent authority provenance")
    .fill("Approved recovery runbook SEC-41");
  await page.getByRole("button", { name: "Enroll recovery authority" }).click();
  await expect(
    page.getByText(/authority enrolled for this destination project only/i),
  ).toBeVisible();
  await page.getByLabel("Verify recovery policy JSON").setInputFiles({
    name: "signer-trust-policy.json",
    mimeType: "application/json",
    buffer: Buffer.from("preview-signed-trust-policy"),
  });
  await expect(
    page.getByText(/Recovery policy: Signature valid; recovery authority not yet proven/i),
  ).toBeVisible();
  await expect(page.getByText(/not recovery authority/i)).toBeVisible();
  await expect(page.getByText(/Payload digest:/i)).toBeVisible();
  await expect(page.getByText(/Proof checkpoint: genesis/i)).toBeVisible();
  await expect(page.getByRole("list", { name: "Recovery policy changes" })).toContainText(
    "add Recovered signer: absent → trusted",
  );
  await expect(
    page.getByRole("button", { name: "Apply authorized signed policy changes" }),
  ).toBeDisabled();
  await page.getByLabel(/Confirm add Recovered signer as trusted/).check();
  await page.getByLabel("Confirm package signer fingerprint").fill("c".repeat(64));
  await page
    .getByLabel("Recovery verification provenance")
    .fill("Matched printed disaster recovery record");
  await page.getByRole("button", { name: "Apply authorized signed policy changes" }).click();
  await expect(page.getByText(/Applied 1 authorized signer-policy changes/i)).toBeVisible();
  await page.getByLabel("Verify recovery policy JSON").setInputFiles({
    name: "signer-trust-policy-repeat.json",
    mimeType: "application/json",
    buffer: Buffer.from("preview-signed-trust-policy"),
  });
  await expect(
    page.getByText(/Witnessed-anchor assessment: Forward ancestry proven/i),
  ).toBeVisible();
  await expect(page.getByText(/contains the witnessed head/i)).toBeVisible();
  await expect(page.getByRole("list", { name: "Recovery policy changes" })).toContainText(
    "replace Recovered signer: trusted → trusted",
  );
  await page.getByLabel("Confirm package signer fingerprint").fill("c".repeat(64));
  await page.getByLabel("Recovery verification provenance").fill("Bridge package SEC-43");
  await page.getByRole("button", { name: "Record as bridge checkpoint" }).click();
  await expect(page.getByText(/no signer policy was imported/i)).toBeVisible();
  await expect(page.getByRole("list", { name: "Checkpoint advancement receipts" })).toContainText(
    "2 → 3 decisions",
  );
  await expect(page.getByText(/deterministic and unsigned/i)).toBeVisible();
  await expect(page.getByText(/Receipt-chain local consistency: verified/i)).toBeVisible();
  await expect(page.getByText(/does not establish signer authority/i)).toBeVisible();
  await expect(page.getByRole("button", { name: "Export unsigned receipts" })).toBeEnabled();
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

  await expect(
    page.getByText(/Protected checkpoint: Not configured; local consistency only/i),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Seal reviewed state in macOS Keychain" }),
  ).toBeDisabled();
  await page
    .getByLabel("Protected checkpoint review note")
    .fill("Reviewed recovery runbook SEC-51 and current trust decisions");
  await page
    .getByLabel(/I reviewed the current trust decisions, recovery authorities, and receipt state/i)
    .check();
  await page.getByRole("button", { name: "Seal reviewed state in macOS Keychain" }).click();
  await expect(page.getByText(/Protected checkpoint: Protected state matches/i)).toBeVisible();
  await expect(
    page.getByRole("status", { name: /Protected checkpoint: Protected state matches/i }),
  ).toBeVisible();
  await expect(page.getByText(/sealed in macOS Keychain/i)).toBeVisible();
  await expect(page.getByText(/does not prove package authorship/i)).toBeVisible();

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
