import { useState, useEffect, useRef } from "react";
import { Highlight, themes } from "prism-react-renderer";
import type { Requirement, GeneratedTest } from "../../lib/types";
import { RequirementsList } from "../spec/RequirementsList";
import { useGenerateTests, useSettings } from "../../hooks/useTestGeneration";

interface Props {
  projectId: string;
  requirements: Requirement[];
}

export function TestGenerationPanel({ projectId, requirements }: Props) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [framework, setFramework] = useState<"jest" | "pytest">("jest");
  const [mode, setMode] = useState<"template" | "llm">("template");
  const [results, setResults] = useState<GeneratedTest[]>([]);
  const generateTests = useGenerateTests();
  const { data: settings } = useSettings();
  const defaultsApplied = useRef(false);

  // Only apply settings defaults once on initial load, not on background refetches
  useEffect(() => {
    if (settings && !defaultsApplied.current) {
      defaultsApplied.current = true;
      if (settings.default_framework === "jest" || settings.default_framework === "pytest") {
        setFramework(settings.default_framework);
      }
      if (settings.default_mode === "template" || settings.default_mode === "llm") {
        setMode(settings.default_mode);
      }
    }
  }, [settings]);

  const toggleRequirement = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const selectAll = () => {
    if (selected.size === requirements.length) {
      setSelected(new Set());
    } else {
      setSelected(new Set(requirements.map((r) => r.id)));
    }
  };

  const handleGenerate = () => {
    generateTests.mutate(
      {
        requirement_ids: Array.from(selected),
        framework,
        mode,
        project_id: projectId,
      },
      {
        onSuccess: (data) => setResults(data),
      },
    );
  };

  return (
    <div className="space-y-6">
      {/* Controls */}
      <div className="flex flex-wrap items-center gap-4">
        <div className="flex items-center gap-2">
          <label className="text-text-muted text-sm">Mode:</label>
          <select
            value={mode}
            onChange={(e) => setMode(e.target.value as "template" | "llm")}
            className="bg-surface border-border text-text rounded-lg border px-3 py-1.5 text-sm"
          >
            <option value="template">Template</option>
            <option value="llm">LLM (Claude)</option>
          </select>
        </div>
        <div className="flex items-center gap-2">
          <label className="text-text-muted text-sm">Framework:</label>
          <select
            value={framework}
            onChange={(e) => setFramework(e.target.value as "jest" | "pytest")}
            className="bg-surface border-border text-text rounded-lg border px-3 py-1.5 text-sm"
          >
            <option value="jest">Jest</option>
            <option value="pytest">PyTest</option>
          </select>
        </div>
        <button
          onClick={selectAll}
          className="text-primary-light hover:text-primary text-sm transition-colors"
        >
          {selected.size === requirements.length ? "Deselect All" : "Select All"}
        </button>
        <button
          onClick={handleGenerate}
          disabled={selected.size === 0 || generateTests.isPending}
          className="bg-primary hover:bg-primary-dark ml-auto rounded-lg px-4 py-2 text-sm text-white transition-colors disabled:opacity-50"
        >
          {generateTests.isPending ? "Generating..." : `Generate (${selected.size})`}
        </button>
      </div>

      <div
        role="note"
        className="border-border bg-surface-alt text-text-muted rounded-lg border p-3 text-sm"
      >
        Offline templates are editable scaffolds, not verification. Placeholder assertions remain
        <span className="text-primary-light font-semibold"> UNKNOWN </span>
        even if their test process exits successfully. Claude-assisted generation is optional and is
        judged by the same evidence rules.
      </div>

      {/* Requirements */}
      <RequirementsList
        requirements={requirements}
        selectable
        selected={selected}
        onToggle={toggleRequirement}
      />

      {/* Results */}
      {results.length > 0 && (
        <div className="space-y-4">
          <h3 className="text-lg font-semibold">Generated Tests</h3>
          {results.map((test) => (
            <div key={test.id} className="border-border overflow-hidden rounded-lg border">
              <div className="bg-surface-alt border-border flex items-center justify-between border-b px-4 py-2">
                <span className="text-text-muted text-xs">
                  {test.framework} | {test.generation_mode}
                </span>
              </div>
              <Highlight
                theme={themes.vsDark}
                code={test.code}
                language={test.framework === "pytest" ? "python" : "javascript"}
              >
                {({ style, tokens, getLineProps, getTokenProps }) => (
                  <pre
                    style={{ ...style, margin: 0, padding: "1rem" }}
                    className="overflow-x-auto text-xs"
                  >
                    {tokens.map((line, i) => (
                      <div key={i} {...getLineProps({ line })}>
                        {line.map((token, key) => (
                          <span key={key} {...getTokenProps({ token })} />
                        ))}
                      </div>
                    ))}
                  </pre>
                )}
              </Highlight>
            </div>
          ))}
        </div>
      )}

      {generateTests.isError && (
        <div className="bg-danger/10 border-danger/30 text-danger rounded-lg border p-3 text-sm">
          {generateTests.error instanceof Error
            ? generateTests.error.message
            : String(generateTests.error)}
        </div>
      )}
    </div>
  );
}
