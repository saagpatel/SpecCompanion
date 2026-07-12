import { useState } from "react";
import { useParams, Link } from "react-router-dom";
import { useProject } from "../hooks/useProjects";
import {
  useTestResults,
  useExecuteTests,
  usePythonRuntime,
  useConfigurePythonRuntime,
  useClearPythonRuntime,
} from "../hooks/useTestExecution";
import { TestResultsTable } from "../components/test/TestResultsTable";
import { ExecutionProgress } from "../components/test/ExecutionProgress";
import { getAllGeneratedTests } from "../lib/api";
import { useQuery } from "@tanstack/react-query";

export function TestExecution() {
  const { projectId } = useParams<{ projectId: string }>();
  const { data: project } = useProject(projectId);
  const { data: results, isError: resultsError } = useTestResults(projectId);
  const executeTests = useExecuteTests(projectId ?? "");
  const runtime = usePythonRuntime(projectId ?? "");
  const configureRuntime = useConfigurePythonRuntime(projectId ?? "");
  const clearRuntime = useClearPythonRuntime(projectId ?? "");
  const [runtimeRoot, setRuntimeRoot] = useState("");
  const [capabilityProfile, setCapabilityProfile] = useState<"bounded" | "macos_isolated">(
    "bounded",
  );

  const {
    data: allTests,
    isLoading: testsLoading,
    isError: testsError,
  } = useQuery({
    queryKey: ["all-generated-tests", projectId],
    queryFn: () => getAllGeneratedTests(projectId!),
    enabled: !!projectId,
  });

  const [selectedTests, setSelectedTests] = useState<Set<string>>(new Set());

  const toggleTest = (id: string) => {
    setSelectedTests((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const selectAll = () => {
    if (!allTests) return;
    if (selectedTests.size === allTests.length) {
      setSelectedTests(new Set());
    } else {
      setSelectedTests(new Set(allTests.map((t) => t.id)));
    }
  };

  const handleExecute = () => {
    const ids = Array.from(selectedTests);
    executeTests.mutate(ids);
  };

  return (
    <div>
      <div className="text-text-muted mb-1 flex items-center gap-2 text-sm">
        <Link to="/" className="hover:text-text transition-colors">
          Dashboard
        </Link>
        <span>/</span>
        <Link to={`/project/${projectId}`} className="hover:text-text transition-colors">
          {project?.name ?? "Project"}
        </Link>
        <span>/</span>
        <span className="text-text">Execute Tests</span>
      </div>
      <h2 className="mb-6 text-2xl font-bold">Test Execution</h2>

      <ExecutionProgress />

      <section
        aria-labelledby="python-runtime-heading"
        className="border-border bg-surface-alt mb-6 rounded-xl border p-4"
      >
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h3 id="python-runtime-heading" className="font-semibold">
              Project Python runtime
            </h3>
            <p className="text-text-muted mt-1 text-sm">
              Optional for dependencyful Python tests. SpecCompanion never installs packages. Trust
              is scoped to this project and execution stops as UNKNOWN if the interpreter or package
              inventory changes.
            </p>
            <p className="text-text-muted mt-1 text-xs">
              VERIFIED also requires a platform-bound isolation receipt. Today, only macOS
              sandbox-exec is recognized; bounded or unavailable platform policies remain PARTIAL.
            </p>
          </div>
          {runtime.data?.configured && (
            <span
              className={`rounded-full px-2 py-1 text-xs ${runtime.data.valid ? "bg-success/10 text-success" : "bg-warning/10 text-warning"}`}
            >
              {runtime.data.valid ? "Attestation matches" : "Trust expired"}
            </span>
          )}
        </div>
        <label htmlFor="project-python-runtime" className="text-text-muted mt-4 block text-sm">
          External environment root
        </label>
        <div className="mt-1 flex flex-col gap-2 sm:flex-row">
          <input
            id="project-python-runtime"
            value={runtimeRoot}
            onChange={(event) => setRuntimeRoot(event.target.value)}
            placeholder={runtime.data?.root || "/Users/you/.virtualenvs/project-tests"}
            autoComplete="off"
            spellCheck={false}
            className="bg-surface border-border text-text focus:border-primary min-w-0 flex-1 rounded-lg border px-3 py-2 font-mono text-sm focus:outline-none"
          />
          <select
            aria-label="Execution capability profile"
            value={capabilityProfile}
            onChange={(event) =>
              setCapabilityProfile(event.target.value as "bounded" | "macos_isolated")
            }
            className="bg-surface border-border rounded-lg border px-3 py-2 text-sm"
          >
            <option value="bounded">Bounded (no OS isolation)</option>
            <option value="macos_isolated">macOS isolated (fail closed)</option>
          </select>
          <button
            onClick={() =>
              configureRuntime.mutate({ root: runtimeRoot, profile: capabilityProfile })
            }
            disabled={!runtimeRoot.trim() || configureRuntime.isPending}
            className="bg-primary hover:bg-primary-dark rounded-lg px-4 py-2 text-sm text-white disabled:opacity-50"
          >
            {configureRuntime.isPending
              ? "Attesting..."
              : runtime.data?.configured
                ? "Trust again"
                : "Trust runtime"}
          </button>
          {runtime.data?.configured && (
            <button
              onClick={() => clearRuntime.mutate()}
              className="border-border rounded-lg border px-4 py-2 text-sm"
            >
              Clear trust
            </button>
          )}
        </div>
        {runtime.data && (
          <p role="status" className="text-text-muted mt-2 text-xs">
            {runtime.data.reason}
          </p>
        )}
        {(configureRuntime.isError || clearRuntime.isError || runtime.isError) && (
          <p role="alert" className="text-danger mt-2 text-sm">
            {configureRuntime.isError
              ? String(configureRuntime.error)
              : "Python runtime trust could not be updated."}
          </p>
        )}
      </section>

      {(testsError || resultsError || executeTests.isError) && (
        <div className="border-danger/30 bg-danger/5 text-danger mb-4 rounded-lg border p-4 text-sm">
          {executeTests.isError
            ? `Execution failed: ${String(executeTests.error)}`
            : "Failed to load test data. Please try again."}
        </div>
      )}

      {testsLoading && <p className="text-text-muted mb-4">Loading generated tests...</p>}

      {/* Test selection */}
      {allTests && allTests.length > 0 && (
        <div className="mb-6">
          <div className="mb-3 flex items-center justify-between">
            <h3 className="text-lg font-semibold">Executable evidence ({allTests.length})</h3>
            <div className="flex gap-2">
              <button
                onClick={selectAll}
                className="text-primary-light hover:text-primary text-sm transition-colors"
              >
                {selectedTests.size === allTests.length ? "Deselect All" : "Select All"}
              </button>
              <button
                onClick={handleExecute}
                disabled={selectedTests.size === 0 || executeTests.isPending}
                className="bg-primary hover:bg-primary-dark rounded-lg px-4 py-2 text-sm text-white transition-colors disabled:opacity-50"
              >
                {executeTests.isPending ? "Running..." : `Run (${selectedTests.size})`}
              </button>
            </div>
          </div>
          <div className="max-h-64 space-y-1 overflow-y-auto">
            {allTests.map((test) => (
              <div
                key={test.id}
                className={`flex cursor-pointer items-center gap-3 rounded-lg border p-3 transition-colors ${
                  selectedTests.has(test.id)
                    ? "border-primary bg-primary/5"
                    : "border-border bg-surface-alt hover:bg-surface-hover"
                }`}
                onClick={() => toggleTest(test.id)}
                role="checkbox"
                aria-checked={selectedTests.has(test.id)}
                tabIndex={0}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    toggleTest(test.id);
                  }
                }}
              >
                <input
                  type="checkbox"
                  checked={selectedTests.has(test.id)}
                  readOnly
                  aria-hidden="true"
                  tabIndex={-1}
                  className="accent-primary"
                />
                <span className="text-text flex-1 truncate text-sm">
                  {test.id.slice(0, 8)} — {test.framework} ({test.generation_mode.replace("_", " ")}
                  )
                </span>
                <span className="text-text-muted text-xs">
                  {new Date(test.created_at).toLocaleDateString()}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

      {allTests && allTests.length === 0 && (
        <div className="border-border bg-surface-alt mb-6 rounded-xl border p-8 text-center">
          <p className="text-text-muted">No generated or linked tests are ready to run.</p>
          <Link
            to={`/project/${projectId}/generate`}
            className="text-primary-light mt-2 inline-block text-sm hover:underline"
          >
            Generate or link test evidence
          </Link>
        </div>
      )}

      {/* Results */}
      {results && results.length > 0 && (
        <div>
          <h3 className="mb-3 text-lg font-semibold">Results</h3>
          <TestResultsTable results={results} />
        </div>
      )}
    </div>
  );
}
