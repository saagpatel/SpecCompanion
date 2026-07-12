import { useState } from "react";
import { useParams, Link } from "react-router-dom";
import { useProject } from "../hooks/useProjects";
import { useTestResults, useExecuteTests } from "../hooks/useTestExecution";
import { TestResultsTable } from "../components/test/TestResultsTable";
import { ExecutionProgress } from "../components/test/ExecutionProgress";
import { getAllGeneratedTests } from "../lib/api";
import { useQuery } from "@tanstack/react-query";

export function TestExecution() {
  const { projectId } = useParams<{ projectId: string }>();
  const { data: project } = useProject(projectId);
  const { data: results, isError: resultsError } = useTestResults(projectId);
  const executeTests = useExecuteTests(projectId ?? "");

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
