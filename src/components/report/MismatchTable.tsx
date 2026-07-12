import { Fragment, useState } from "react";
import type { RequirementAlignment } from "../../lib/types";

interface Props {
  alignments: RequirementAlignment[];
}

const badge: Record<string, string> = {
  VERIFIED: "bg-success/20 text-success",
  PARTIAL: "bg-warning/20 text-warning",
  FAILED: "bg-danger/20 text-danger",
  UNKNOWN: "bg-primary/20 text-primary-light",
};

export function EvidenceTable({ alignments }: Props) {
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  if (alignments.length === 0) {
    return (
      <div className="border-border bg-surface-alt text-text-muted rounded-lg border p-4 text-sm">
        No requirements were available to classify.
      </div>
    );
  }

  const toggle = (id: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  return (
    <div className="border-border overflow-x-auto rounded-lg border">
      <table className="w-full min-w-[760px] text-sm">
        <caption className="sr-only">Requirement classifications and exact evidence</caption>
        <thead>
          <tr className="bg-surface-alt border-border border-b">
            <th scope="col" className="text-text-muted px-4 py-2 text-left font-medium">
              Requirement
            </th>
            <th scope="col" className="text-text-muted px-4 py-2 text-left font-medium">
              Classification
            </th>
            <th scope="col" className="text-text-muted px-4 py-2 text-left font-medium">
              Why
            </th>
            <th scope="col" className="text-text-muted px-4 py-2 text-right font-medium">
              Evidence
            </th>
          </tr>
        </thead>
        <tbody>
          {alignments.map((alignment) => {
            const isExpanded = expanded.has(alignment.requirement_id);
            return (
              <Fragment key={alignment.requirement_id}>
                <tr className="border-border border-b align-top">
                  <td className="text-text px-4 py-3">
                    <p>{alignment.description}</p>
                    <p className="text-text-muted mt-1 text-xs">
                      {alignment.section}, line {alignment.source_line_start}
                    </p>
                  </td>
                  <td className="px-4 py-3">
                    <span
                      className={`rounded px-2 py-0.5 text-xs font-semibold ${badge[alignment.classification]}`}
                    >
                      {alignment.classification}
                    </span>
                  </td>
                  <td className="text-text px-4 py-3">
                    <p>{alignment.summary}</p>
                    <p className="text-text-muted mt-1 font-mono text-xs">{alignment.reason}</p>
                  </td>
                  <td className="px-4 py-3 text-right">
                    <button
                      type="button"
                      aria-expanded={isExpanded}
                      aria-controls={`evidence-${alignment.requirement_id}`}
                      onClick={() => toggle(alignment.requirement_id)}
                      className="text-primary-light hover:text-primary rounded px-2 py-1 focus-visible:outline-2 focus-visible:outline-offset-2"
                    >
                      {isExpanded ? "Hide" : "Show"} {alignment.evidence.length}
                    </button>
                  </td>
                </tr>
                {isExpanded && (
                  <tr>
                    <td colSpan={4} className="bg-surface-alt px-4 py-3">
                      <section
                        aria-labelledby={`policy-${alignment.requirement_id}`}
                        className="border-border bg-surface mb-3 rounded border p-3"
                      >
                        <div className="flex flex-wrap items-center gap-2">
                          <h3
                            id={`policy-${alignment.requirement_id}`}
                            className="text-text text-xs font-semibold uppercase"
                          >
                            Verification policy
                          </h3>
                          <span className="bg-surface-alt text-text-muted rounded px-1.5 py-0.5 font-mono text-xs">
                            {alignment.verification_policy.status}
                          </span>
                        </div>
                        <p className="text-text mt-1 text-xs">
                          {alignment.verification_policy.summary}
                        </p>
                        <p className="text-text-muted mt-1 font-mono text-xs">
                          {alignment.verification_policy.policy_id}
                        </p>
                        {alignment.verification_policy.required_controls.length > 0 && (
                          <p className="text-text-muted mt-2 text-xs">
                            Required: {alignment.verification_policy.required_controls.join(", ")}
                          </p>
                        )}
                        {alignment.verification_policy.observations.map((observation) => (
                          <p key={observation.test_id} className="text-text-muted mt-2 text-xs">
                            Observed for {observation.test_id}: platform=
                            {observation.controls.platform || "unknown"}, backend=
                            {observation.controls.isolation_backend || "unknown"}, profile=
                            {observation.controls.profile}, network={observation.controls.network},
                            filesystem_write=
                            {observation.controls.filesystem_write}, timeout=
                            {observation.controls.timeout}, output_limit=
                            {observation.controls.output_limit}, process_tree_kill=
                            {observation.controls.process_tree_kill}
                          </p>
                        ))}
                      </section>
                      <ul id={`evidence-${alignment.requirement_id}`} className="space-y-2">
                        {alignment.evidence.map((item) => (
                          <li key={item.id} className="border-border rounded border p-3">
                            <div className="flex flex-wrap items-center gap-2">
                              <span className="text-text text-xs font-semibold uppercase">
                                {item.kind}
                              </span>
                              <span className="bg-surface text-text-muted rounded px-1.5 py-0.5 font-mono text-xs">
                                {item.status}
                              </span>
                              {item.path && (
                                <span className="text-text-muted font-mono text-xs break-all">
                                  {item.path}
                                  {item.line_start ? `:${item.line_start}` : ""}
                                </span>
                              )}
                            </div>
                            <p className="text-text mt-1 text-xs">{item.summary}</p>
                          </li>
                        ))}
                      </ul>
                    </td>
                  </tr>
                )}
              </Fragment>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
