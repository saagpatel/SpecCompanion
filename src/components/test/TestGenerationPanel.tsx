import { useState, useEffect, useRef } from "react";
import { Highlight, themes } from "prism-react-renderer";
import type { Requirement, GeneratedTest } from "../../lib/types";
import { RequirementsList } from "../spec/RequirementsList";
import {
  useGenerateTests,
  useLinkRepositoryTest,
  useRepositoryTests,
  useSettings,
} from "../../hooks/useTestGeneration";

interface Props {
  projectId: string;
  requirements: Requirement[];
}

export function TestGenerationPanel({ projectId, requirements }: Props) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [framework, setFramework] = useState<"jest" | "pytest">("jest");
  const [mode, setMode] = useState<"template" | "llm">("template");
  const [results, setResults] = useState<GeneratedTest[]>([]);
  const [linkRequirementId, setLinkRequirementId] = useState("");
  const [linkPath, setLinkPath] = useState("");
  const generateTests = useGenerateTests();
  const repositoryTests = useRepositoryTests(projectId);
  const linkRepositoryTest = useLinkRepositoryTest();
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

  const handleLinkRepositoryTest = () => {
    linkRepositoryTest.mutate(
      {
        project_id: projectId,
        requirement_id: linkRequirementId,
        path: linkPath,
      },
      {
        onSuccess: (test) => {
          setResults((current) =>
            current.some((candidate) => candidate.id === test.id) ? current : [test, ...current],
          );
        },
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

      <fieldset className="border-border bg-surface-alt space-y-4 rounded-xl border p-4">
        <legend className="px-1 text-base font-semibold">Existing repository evidence</legend>
        <p className="text-text-muted text-sm">
          Link a scanned test to a requirement without changing the target repository. The link is
          recorded as an explicit user decision—not inferred semantic equivalence—and the assertion
          and execution still have to be meaningful before the report can verify anything.
        </p>

        {repositoryTests.isLoading && (
          <p className="text-text-muted text-sm" role="status">
            Scanning contained repository tests…
          </p>
        )}

        {repositoryTests.isError && (
          <p className="text-danger text-sm" role="alert">
            Repository tests could not be scanned. The generated-test workflow is still available.
          </p>
        )}

        {repositoryTests.data?.length === 0 && (
          <p className="text-text-muted text-sm">No supported repository tests were found.</p>
        )}

        {repositoryTests.data && repositoryTests.data.length > 0 && (
          <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] md:items-end">
            <div>
              <label
                htmlFor="repository-requirement"
                className="text-text-muted mb-1 block text-sm"
              >
                Requirement
              </label>
              <select
                id="repository-requirement"
                value={linkRequirementId}
                onChange={(event) => setLinkRequirementId(event.target.value)}
                className="bg-surface border-border text-text w-full rounded-lg border px-3 py-2 text-sm"
              >
                <option value="">Select a requirement</option>
                {requirements.map((requirement) => (
                  <option key={requirement.id} value={requirement.id}>
                    {requirement.description}
                  </option>
                ))}
              </select>
            </div>

            <div>
              <label htmlFor="repository-test" className="text-text-muted mb-1 block text-sm">
                Repository test
              </label>
              <select
                id="repository-test"
                value={linkPath}
                onChange={(event) => setLinkPath(event.target.value)}
                className="bg-surface border-border text-text w-full rounded-lg border px-3 py-2 text-sm"
              >
                <option value="">Select a contained test</option>
                {repositoryTests.data.map((test) => (
                  <option key={test.path} value={test.path}>
                    {test.path} — {test.framework} — {test.assertion_status}
                  </option>
                ))}
              </select>
            </div>

            <button
              type="button"
              onClick={handleLinkRepositoryTest}
              disabled={!linkRequirementId || !linkPath || linkRepositoryTest.isPending}
              className="bg-primary hover:bg-primary-dark rounded-lg px-4 py-2 text-sm text-white transition-colors disabled:opacity-50"
            >
              {linkRepositoryTest.isPending ? "Linking…" : "Link test evidence"}
            </button>
          </div>
        )}

        {linkRepositoryTest.isSuccess && (
          <p className="text-success text-sm" role="status">
            Linked {linkRepositoryTest.data.file_path} as {linkRepositoryTest.data.framework} test
            evidence. Run it before generating a new report.
          </p>
        )}

        {linkRepositoryTest.isError && (
          <p className="text-danger text-sm" role="alert">
            {linkRepositoryTest.error instanceof Error
              ? linkRepositoryTest.error.message
              : String(linkRepositoryTest.error)}
          </p>
        )}
      </fieldset>

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
                  {test.framework} | {test.generation_mode.replace("_", " ")}
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
