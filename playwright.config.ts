import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/ui",
  fullyParallel: false,
  retries: process.env.CI ? 2 : 0,
  reporter: [["html"], ["github"]],
  snapshotPathTemplate: "{testDir}/{testFilePath}-snapshots/{arg}-{projectName}{ext}",
  webServer: {
    command: process.env.PLAYWRIGHT_WEB_SERVER_CMD ?? "pnpm dev --host 127.0.0.1 --port 4105",
    url: process.env.PLAYWRIGHT_BASE_URL || "http://127.0.0.1:4105",
    reuseExistingServer: !process.env.CI,
    timeout: 120 * 1000,
  },
  use: {
    baseURL: process.env.PLAYWRIGHT_BASE_URL || "http://127.0.0.1:4105",
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    video: "off",
    timezoneId: "UTC",
    locale: "en-US",
    colorScheme: "light",
  },
  expect: {
    toHaveScreenshot: { maxDiffPixelRatio: 0.002, threshold: 0.2 },
  },
  projects: [
    {
      name: "desktop",
      use: {
        ...devices["Desktop Chrome"],
        viewport: { width: 1440, height: 900 },
      },
    },
    {
      name: "mobile",
      use: {
        ...devices["Pixel 7"],
        viewport: { width: 390, height: 844 },
      },
    },
  ],
});
