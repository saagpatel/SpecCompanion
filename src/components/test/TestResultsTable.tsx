import { Fragment, useState } from "react";
import type { TestResult } from "../../lib/types";

interface Props {
  results: TestResult[];
}

const statusColors: Record<string, string> = {
  passed: "text-success",
  failed: "text-danger",
  error: "text-danger",
  timed_out: "text-danger",
  runtime_unavailable: "text-warning",
  blocked: "text-warning",
  unsupported: "text-text-muted",
};

const statusIcons: Record<string, string> = {
  passed: "\u2713",
  failed: "\u2717",
  error: "\u26A0",
  timed_out: "\u23F1",
  runtime_unavailable: "?",
  blocked: "\u26A0",
  unsupported: "\u2014",
};

export function TestResultsTable({ results }: Props) {
  const [expanded, setExpanded] = useState<string | null>(null);

  if (results.length === 0) {
    return <p className="text-text-muted text-sm">No test results yet.</p>;
  }

  return (
    <div className="border-border overflow-hidden rounded-lg border">
      <table className="w-full text-sm">
        <caption className="sr-only">Bounded test execution results</caption>
        <thead>
          <tr className="bg-surface-alt border-border border-b">
            <th scope="col" className="text-text-muted px-4 py-2 text-left font-medium">
              Status
            </th>
            <th scope="col" className="text-text-muted px-4 py-2 text-left font-medium">
              Test ID
            </th>
            <th scope="col" className="text-text-muted px-4 py-2 text-right font-medium">
              Time
            </th>
            <th scope="col" className="text-text-muted px-4 py-2 text-left font-medium">
              Executed
            </th>
          </tr>
        </thead>
        <tbody>
          {results.map((result) => (
            <Fragment key={result.id}>
              <tr className="border-border hover:bg-surface-hover border-b">
                <td className={`px-4 py-2 font-mono ${statusColors[result.status]}`}>
                  <button
                    type="button"
                    aria-expanded={expanded === result.id}
                    aria-controls={`result-output-${result.id}`}
                    onClick={() => setExpanded(expanded === result.id ? null : result.id)}
                    className="rounded focus-visible:outline-2 focus-visible:outline-offset-2"
                  >
                    {statusIcons[result.status] ?? "?"} {result.status}
                  </button>
                </td>
                <td className="text-text-muted px-4 py-2 font-mono text-xs">
                  {result.generated_test_id.slice(0, 8)}
                </td>
                <td className="text-text-muted px-4 py-2 text-right">
                  {result.execution_time_ms}ms
                </td>
                <td className="text-text-muted px-4 py-2">
                  {new Date(result.executed_at).toLocaleString()}
                </td>
              </tr>
              {expanded === result.id && (
                <tr key={`${result.id}-detail`}>
                  <td
                    id={`result-output-${result.id}`}
                    colSpan={4}
                    className="bg-surface px-4 py-3"
                  >
                    {result.stdout && (
                      <div className="mb-2">
                        <span className="text-text-muted text-xs font-medium">stdout:</span>
                        <pre className="text-text bg-surface-alt mt-1 max-h-48 overflow-x-auto rounded p-2 text-xs">
                          {result.stdout}
                        </pre>
                      </div>
                    )}
                    {result.stderr && (
                      <div>
                        <span className="text-text-muted text-xs font-medium">stderr:</span>
                        <pre className="text-danger bg-surface-alt mt-1 max-h-48 overflow-x-auto rounded p-2 text-xs">
                          {result.stderr}
                        </pre>
                      </div>
                    )}
                    <div className="mt-2">
                      <span className="text-text-muted text-xs font-medium">Applied controls:</span>
                      <p className="text-text-muted mt-1 text-xs">
                        Provenance: {result.provenance_status || "unavailable"}
                        {result.provenance_digest ? ` (${result.provenance_digest})` : ""}
                      </p>
                      <p className="text-text-muted mt-1 font-mono text-xs">
                        {result.execution_controls.profile
                          ? `platform=${result.execution_controls.platform || "unknown"}; backend=${result.execution_controls.isolation_backend || "unknown"}; profile=${result.execution_controls.profile}; network=${result.execution_controls.network}; filesystem_write=${result.execution_controls.filesystem_write}; child_process=${result.execution_controls.child_process}`
                          : "Control evidence unavailable for this legacy result."}
                      </p>
                    </div>
                    {!result.stdout && !result.stderr && (
                      <p className="text-text-muted text-xs">No output captured.</p>
                    )}
                  </td>
                </tr>
              )}
            </Fragment>
          ))}
        </tbody>
      </table>
    </div>
  );
}
