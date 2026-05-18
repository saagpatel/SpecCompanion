import { expect, test } from "@playwright/test";

test("@visual error boundary recovery screen", async ({ page }, testInfo) => {
  await page.goto("/__error-boundary");
  await page.addStyleTag({
    content:
      "*,*::before,*::after{animation:none!important;transition:none!important;caret-color:transparent!important;}",
  });
  const maxDiffPixelRatio = testInfo.project.name === "mobile" ? 0.12 : 0.05;
  await expect(page.locator("body")).toHaveScreenshot("error-boundary-body.png", {
    animations: "disabled",
    maxDiffPixelRatio,
  });
});
