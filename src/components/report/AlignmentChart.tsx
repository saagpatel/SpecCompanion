import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, Cell } from "recharts";

interface Props {
  verified: number;
  partial: number;
  failed: number;
  unknown: number;
  totalRequirements: number;
}

export function AlignmentChart({ verified, partial, failed, unknown, totalRequirements }: Props) {
  const data = [
    { name: "Verified", value: verified, color: "#22c55e" },
    { name: "Partial", value: partial, color: "#eab308" },
    { name: "Failed", value: failed, color: "#ef4444" },
    { name: "Unknown", value: unknown, color: "#6366f1" },
  ].filter((item) => item.value > 0);

  if (totalRequirements === 0)
    return <p className="text-text-muted text-sm">No requirements to analyze.</p>;
  return (
    <div className="h-48 min-w-0" role="img" aria-label="Requirement classification breakdown">
      <ResponsiveContainer width="100%" height={192} minWidth={240}>
        <BarChart data={data} layout="vertical" margin={{ left: 60 }}>
          <XAxis type="number" allowDecimals={false} tick={{ fill: "#9393a8", fontSize: 12 }} />
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
