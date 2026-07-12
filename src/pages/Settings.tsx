import { useState, useEffect, useRef } from "react";
import { useSettings, useSaveSettings } from "../hooks/useTestGeneration";
import type { AppSettings } from "../lib/types";

export function Settings() {
  const { data: settings, isLoading } = useSettings();
  const saveSettings = useSaveSettings();
  const [form, setForm] = useState<AppSettings>({
    api_key: "",
    default_framework: "jest",
    default_mode: "template",
    scan_exclusions: [],
    python_environment_root: "",
  });
  const [exclusionInput, setExclusionInput] = useState("");
  const [showSaved, setShowSaved] = useState(false);
  const savedTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => {
    if (settings) {
      setForm(settings);
      setExclusionInput(settings.scan_exclusions.join(", "));
    }
  }, [settings]);

  // Clean up timer on unmount
  useEffect(() => {
    return () => {
      if (savedTimerRef.current) clearTimeout(savedTimerRef.current);
    };
  }, []);

  const handleSave = () => {
    const exclusions = exclusionInput
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    saveSettings.mutate(
      { ...form, scan_exclusions: exclusions },
      {
        onSuccess: () => {
          setShowSaved(true);
          if (savedTimerRef.current) clearTimeout(savedTimerRef.current);
          savedTimerRef.current = setTimeout(() => setShowSaved(false), 3000);
        },
      },
    );
  };

  if (isLoading)
    return (
      <p role="status" className="text-text-muted">
        Loading settings...
      </p>
    );

  return (
    <div>
      <h2 className="mb-6 text-2xl font-bold">Settings</h2>
      <div className="max-w-lg space-y-6">
        {/* API Key */}
        <div>
          <label htmlFor="claude-api-key" className="text-text-muted mb-1 block text-sm">
            Claude API Key (optional)
          </label>
          <input
            id="claude-api-key"
            type="password"
            value={form.api_key}
            onChange={(e) => setForm({ ...form, api_key: e.target.value })}
            placeholder="sk-ant-..."
            className="bg-surface border-border text-text focus:border-primary w-full rounded-lg border px-3 py-2 text-sm focus:outline-none"
          />
          <p className="text-text-muted mt-1 text-xs">
            Only used when you explicitly choose Claude-assisted generation. Offline evidence
            analysis does not need it.
          </p>
        </div>

        {/* Default Framework */}
        <div>
          <label htmlFor="default-framework" className="text-text-muted mb-1 block text-sm">
            Default Framework
          </label>
          <select
            id="default-framework"
            value={form.default_framework}
            onChange={(e) =>
              setForm({ ...form, default_framework: e.target.value as "jest" | "pytest" })
            }
            className="bg-surface border-border text-text rounded-lg border px-3 py-2 text-sm"
          >
            <option value="jest">Jest</option>
            <option value="pytest">PyTest</option>
          </select>
        </div>

        {/* Default Mode */}
        <div>
          <label htmlFor="default-mode" className="text-text-muted mb-1 block text-sm">
            Default Generation Mode
          </label>
          <select
            id="default-mode"
            value={form.default_mode}
            onChange={(e) =>
              setForm({ ...form, default_mode: e.target.value as "template" | "llm" })
            }
            className="bg-surface border-border text-text rounded-lg border px-3 py-2 text-sm"
          >
            <option value="template">Template (offline)</option>
            <option value="llm">LLM (Claude API)</option>
          </select>
        </div>

        {/* Scan Exclusions */}
        <div>
          <label htmlFor="scan-exclusions" className="text-text-muted mb-1 block text-sm">
            Scan Exclusion Patterns
          </label>
          <input
            id="scan-exclusions"
            type="text"
            value={exclusionInput}
            onChange={(e) => setExclusionInput(e.target.value)}
            placeholder="dist, build, .cache"
            className="bg-surface border-border text-text focus:border-primary w-full rounded-lg border px-3 py-2 text-sm focus:outline-none"
          />
          <p className="text-text-muted mt-1 text-xs">
            Comma-separated directory names to skip during codebase scanning.
          </p>
        </div>

        <div>
          <label htmlFor="python-environment-root" className="text-text-muted mb-1 block text-sm">
            Trusted Python environment (optional)
          </label>
          <input
            id="python-environment-root"
            type="text"
            value={form.python_environment_root}
            onChange={(e) => setForm({ ...form, python_environment_root: e.target.value })}
            placeholder="/Users/you/.virtualenvs/project-tests"
            autoComplete="off"
            spellCheck={false}
            className="bg-surface border-border text-text focus:border-primary w-full rounded-lg border px-3 py-2 font-mono text-sm focus:outline-none"
          />
          <p className="text-text-muted mt-1 text-xs">
            Used only for Python tests. SpecCompanion never installs packages. The environment must
            be an absolute, non-symlink directory outside the target project and is validated again
            before every run. Saving it explicitly trusts code already installed there.
          </p>
        </div>

        <button
          onClick={handleSave}
          disabled={saveSettings.isPending}
          className="bg-primary hover:bg-primary-dark rounded-lg px-6 py-2 text-sm text-white transition-colors disabled:opacity-50"
        >
          {saveSettings.isPending ? "Saving..." : "Save Settings"}
        </button>

        {showSaved && (
          <p role="status" className="text-success text-sm">
            Settings saved.
          </p>
        )}
        {saveSettings.isError && (
          <p role="alert" className="text-danger text-sm">
            Failed to save settings.
          </p>
        )}
      </div>
    </div>
  );
}
