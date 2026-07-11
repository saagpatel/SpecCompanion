import type { Requirement } from "../../lib/types";

interface Props {
  requirements: Requirement[];
  selectable?: boolean;
  selected?: Set<string>;
  onToggle?: (id: string) => void;
}

const typeBadgeColors: Record<string, string> = {
  functional: "bg-primary/20 text-primary-light",
  non_functional: "bg-warning/20 text-warning",
  constraint: "bg-danger/20 text-danger",
};

const priorityBadgeColors: Record<string, string> = {
  high: "text-danger",
  medium: "text-warning",
  low: "text-text-muted",
};

export function RequirementsList({ requirements, selectable, selected, onToggle }: Props) {
  // Group by section
  const grouped = requirements.reduce<Record<string, Requirement[]>>((acc, req) => {
    if (!acc[req.section]) acc[req.section] = [];
    acc[req.section].push(req);
    return acc;
  }, {});

  if (requirements.length === 0) {
    return <p className="text-text-muted text-sm">No requirements found.</p>;
  }

  return (
    <div className="space-y-4">
      {Object.entries(grouped).map(([section, reqs]) => (
        <div key={section}>
          <h4 className="text-text-muted mb-2 text-sm font-medium">{section}</h4>
          <div className="space-y-1">
            {reqs.map((req) => (
              <div
                key={req.id}
                className={`border-border bg-surface flex items-start gap-3 rounded-lg border p-3 ${
                  selectable ? "hover:bg-surface-hover cursor-pointer" : ""
                } ${selected?.has(req.id) ? "border-primary bg-primary/5" : ""}`}
                onClick={() => selectable && onToggle?.(req.id)}
                role={selectable ? "checkbox" : undefined}
                aria-checked={selectable ? (selected?.has(req.id) ?? false) : undefined}
                tabIndex={selectable ? 0 : undefined}
                onKeyDown={(event) => {
                  if (selectable && (event.key === "Enter" || event.key === " ")) {
                    event.preventDefault();
                    onToggle?.(req.id);
                  }
                }}
              >
                {selectable && (
                  <input
                    type="checkbox"
                    checked={selected?.has(req.id) ?? false}
                    readOnly
                    aria-hidden="true"
                    tabIndex={-1}
                    className="accent-primary mt-1"
                  />
                )}
                <div className="min-w-0 flex-1">
                  <p className="text-text text-sm">{req.description}</p>
                  <div className="mt-1 flex gap-2">
                    <span className="text-text-muted text-xs">line {req.source_line_start}</span>
                    <span
                      className={`rounded px-1.5 py-0.5 text-xs ${typeBadgeColors[req.req_type] ?? "bg-border text-text-muted"}`}
                    >
                      {req.req_type.replace("_", "-")}
                    </span>
                    <span
                      className={`text-xs ${priorityBadgeColors[req.priority] ?? "text-text-muted"}`}
                    >
                      {req.priority}
                    </span>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
