import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { isTauriRuntime } from "../../lib/api";
import type { TestProgress } from "../../lib/types";

export function ExecutionProgress() {
  const [progress, setProgress] = useState<TestProgress | null>(null);

  useEffect(() => {
    if (!isTauriRuntime()) return;

    const unlisten = listen<TestProgress>("test-progress", (event) => {
      setProgress(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, []);

  if (!progress || progress.status === "completed") return null;

  const isError = progress.status === "error";
  const percent = progress.total > 0 ? (progress.completed / progress.total) * 100 : 0;

  return (
    <div
      className={`mb-4 rounded-lg border p-4 ${isError ? "border-danger/30 bg-danger/5" : "border-border bg-surface-alt"}`}
    >
      <div className="mb-2 flex items-center justify-between">
        <span className={`text-sm ${isError ? "text-danger" : "text-text"}`}>
          {isError
            ? `Test execution error at ${progress.completed}/${progress.total}`
            : `Running tests... (${progress.completed}/${progress.total})`}
        </span>
        <span className="text-text-muted text-xs">{percent.toFixed(0)}%</span>
      </div>
      <div className="bg-surface h-2 w-full rounded-full">
        <div
          className={`h-2 rounded-full transition-all duration-300 ${isError ? "bg-danger" : "bg-primary"}`}
          style={{ width: `${percent}%` }}
        />
      </div>
    </div>
  );
}
