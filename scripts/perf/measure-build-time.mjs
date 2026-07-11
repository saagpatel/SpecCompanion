import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";

const npmExecPath = process.env.npm_execpath;
if (!npmExecPath) {
  console.error("npm_execpath is not set; run this script through pnpm, npm, or yarn.");
  process.exit(1);
}

// A single wall-clock sample is dominated by scheduler noise on shared CI and
// developer machines. Use an odd-sized median so one slow process launch cannot
// turn an otherwise unchanged build into a false regression.
const sampleCount = 3;
const samplesMs = [];
let failedStatus = 0;

for (let sample = 0; sample < sampleCount; sample += 1) {
  const start = performance.now();
  const result = spawnSync(process.execPath, [npmExecPath, "run", "build"], {
    stdio: "inherit",
  });
  samplesMs.push(Math.round(performance.now() - start));

  if (result.status !== 0) {
    failedStatus = result.status ?? 1;
    break;
  }
}

const sortedSamples = [...samplesMs].sort((a, b) => a - b);
const buildMs = sortedSamples[Math.floor(sortedSamples.length / 2)];

mkdirSync(".perf-results", { recursive: true });
writeFileSync(
  ".perf-results/build-time.json",
  JSON.stringify(
    {
      buildMs,
      samplesMs,
      aggregation: "median",
      capturedAt: new Date().toISOString(),
      command: "npm_execpath run build",
    },
    null,
    2,
  ),
);

if (failedStatus !== 0) {
  process.exit(failedStatus);
}
