import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, Cell } from "recharts";
import type { Mismatch } from "../../lib/types";

interface Props {
  mismatches: Mismatch[];
  totalRequirements: number;
  coveredRequirements: number;
}

export function AlignmentChart({ mismatches, totalRequirements, coveredRequirements }: Props) {
  // Count by mismatch type
  const counts: Record<string, number> = {};
  for (const m of mismatches) {
    counts[m.mismatch_type] = (counts[m.mismatch_type] || 0) + 1;
  }

  const data = [
    { name: "Covered", value: coveredRequirements, color: "#22c55e" },
    { name: "No Test", value: counts.no_test_generated || 0, color: "#eab308" },
    { name: "Failing", value: counts.test_failing || 0, color: "#ef4444" },
    { name: "Not Run", value: counts.not_implemented || 0, color: "#6366f1" },
    { name: "Partial", value: counts.partial_coverage || 0, color: "#f97316" },
  ].filter((d) => d.value > 0);

  if (totalRequirements === 0) {
    return <p className="text-text-muted text-sm">No requirements to analyze.</p>;
  }

  return (
    <div className="h-48 min-w-0">
      <ResponsiveContainer width="100%" height={192} minWidth={240}>
        <BarChart data={data} layout="vertical" margin={{ left: 60 }}>
          <XAxis type="number" tick={{ fill: "#9393a8", fontSize: 12 }} />
          <YAxis
            type="category"
            dataKey="name"
            tick={{ fill: "#9393a8", fontSize: 12 }}
            width={60}
          />
          <Tooltip
            contentStyle={{
              backgroundColor: "#252538",
              border: "1px solid #333348",
              borderRadius: "8px",
              color: "#e4e4f0",
              fontSize: "12px",
            }}
          />
          <Bar dataKey="value" radius={[0, 4, 4, 0]}>
            {data.map((entry) => (
              <Cell key={entry.name} fill={entry.color} />
            ))}
          </Bar>
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}
