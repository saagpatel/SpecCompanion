import { useState, useEffect } from "react";
import { useParams, Link } from "react-router-dom";
import { useProject } from "../hooks/useProjects";
import {
  useReports,
  useAlignmentReport,
  useGenerateAlignmentReport,
  useExportReport,
  useVerifyEvidenceBundle,
} from "../hooks/useReports";
import { CoverageGauge } from "../components/report/CoverageGauge";
import { AlignmentChart } from "../components/report/AlignmentChart";
import { EvidenceTable } from "../components/report/MismatchTable";

export function Reports() {
  const { projectId } = useParams<{ projectId: string }>();
  const { data: project } = useProject(projectId);
  const { data: reports, isError: reportsError } = useReports(projectId);
  const generateReport = useGenerateAlignmentReport(projectId ?? "");
  const exportReport = useExportReport();
  const verifyBundle = useVerifyEvidenceBundle();
  const [selectedReportId, setSelectedReportId] = useState<string | undefined>();

  const {
    data: report,
    isLoading: reportLoading,
    isError: reportError,
  } = useAlignmentReport(selectedReportId);

  // Auto-select latest report
  useEffect(() => {
    if (!selectedReportId && reports && reports.length > 0) {
      setSelectedReportId(reports[0].id);
    }
  }, [reports, selectedReportId]);

  const handleExport = (format: "json" | "html" | "csv" | "bundle") => {
    if (!selectedReportId) return;
    exportReport.mutate(
      { reportId: selectedReportId, format },
      {
        onSuccess: (content) => {
          const blob = new Blob([content], {
            type:
              format === "json" || format === "bundle"
                ? "application/json"
                : format === "html"
                  ? "text/html"
                  : "text/csv",
          });
          const url = URL.createObjectURL(blob);
          const a = document.createElement("a");
          a.href = url;
          a.download =
            format === "bundle" ? "alignment-evidence-bundle.json" : `alignment-report.${format}`;
          a.click();
          URL.revokeObjectURL(url);
        },
      },
    );
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
        <span className="text-text">Reports</span>
      </div>

      <div className="mb-6 flex items-center justify-between">
        <h2 className="text-2xl font-bold">Alignment Reports</h2>
        <div className="flex gap-2">
          <button
            onClick={() =>
              generateReport.mutate(undefined, {
                onSuccess: (data) => setSelectedReportId(data.id),
              })
            }
            disabled={generateReport.isPending}
            className="bg-primary hover:bg-primary-dark rounded-lg px-4 py-2 text-sm text-white transition-colors disabled:opacity-50"
          >
            {generateReport.isPending ? "Generating..." : "Generate Report"}
          </button>
        </div>
      </div>

      {(reportsError || generateReport.isError) && (
        <div className="border-danger/30 bg-danger/5 text-danger mb-4 rounded-lg border p-4 text-sm">
          {generateReport.isError
            ? `Failed to generate report: ${String(generateReport.error)}`
            : `Failed to load reports.`}
        </div>
      )}

      <section
        aria-labelledby="verify-bundle-heading"
        className="border-border bg-surface-alt mb-6 rounded-xl border p-4"
      >
        <h3 id="verify-bundle-heading" className="text-sm font-semibold">
          Verify an evidence bundle
        </h3>
        <p className="text-text-muted mt-1 text-sm">
          Verification is offline and read-only. Imported evidence is never added to this project.
        </p>
        <label className="text-primary-light mt-3 inline-block cursor-pointer text-sm hover:underline">
          Choose bundle JSON
          <input
            type="file"
            accept="application/json,.json"
            className="sr-only"
            onChange={async (event) => {
              const file = event.target.files?.[0];
              if (!file) return;
              verifyBundle.mutate(await file.text());
              event.target.value = "";
            }}
          />
        </label>
        {verifyBundle.isPending && (
          <p role="status" className="text-text-muted mt-3 text-sm">
            Verifying bundle...
          </p>
        )}
        {verifyBundle.data && (
          <div
            role="status"
            className={`mt-3 rounded-lg border p-3 text-sm ${verifyBundle.data.status === "verified" ? "border-success/30 bg-success/5 text-success" : "border-warning/30 bg-warning/5 text-warning"}`}
          >
            <p className="font-medium">Bundle status: {verifyBundle.data.status}.</p>
            <p>
              Integrity: payload {verifyBundle.data.payload_integrity}, bundle{" "}
              {verifyBundle.data.bundle_integrity}, report {verifyBundle.data.report_integrity}.
              Freshness: {verifyBundle.data.freshness_status}. Signature:{" "}
              {verifyBundle.data.signature_status}.
            </p>
            {verifyBundle.data.signature_status === "unsigned" && (
              <p>Integrity is not proof of authorship or trusted time.</p>
            )}
            {verifyBundle.data.diagnostics.length > 0 && (
              <ul className="mt-2 list-disc pl-5">
                {verifyBundle.data.diagnostics.map((diagnostic) => (
                  <li key={diagnostic}>{diagnostic}</li>
                ))}
              </ul>
            )}
          </div>
        )}
        {verifyBundle.isError && (
          <p role="alert" className="text-danger mt-3 text-sm">
            The selected file could not be verified. No evidence was imported.
          </p>
        )}
      </section>

      {reportError && (
        <div
          role="alert"
          className="border-danger/30 bg-danger/5 text-danger mb-4 rounded-lg border p-4 text-sm"
        >
          The selected report could not be loaded. Its evidence remains unknown.
        </div>
      )}

      {/* Report selector */}
      {reports && reports.length > 1 && (
        <div className="mb-4">
          <label className="text-text-muted mr-2 text-sm">Report:</label>
          <select
            value={selectedReportId ?? ""}
            onChange={(e) => setSelectedReportId(e.target.value)}
            className="bg-surface border-border text-text rounded-lg border px-3 py-1.5 text-sm"
          >
            {reports.map((r) => (
              <option key={r.id} value={r.id}>
                {new Date(r.generated_at).toLocaleString()} — {r.verified_requirements}/
                {r.total_requirements} verified
              </option>
            ))}
          </select>
        </div>
      )}

      {reportLoading ? (
        <p role="status" className="text-text-muted">
          Loading evidence report...
        </p>
      ) : report ? (
        <div className="space-y-6">
          <p
            role="status"
            className={`rounded-lg border p-3 text-sm ${report.integrity_status === "verified" ? "border-success/30 bg-success/5 text-success" : "border-warning/30 bg-warning/5 text-warning"}`}
          >
            Report integrity: {report.integrity_status.replace(/_/g, " ")}.
            {report.integrity_status !== "verified" &&
              " Persisted evidence cannot be treated as tamper-evident."}
          </p>
          <section
            aria-labelledby="scan-scope-heading"
            className="border-border bg-surface-alt rounded-xl border p-4"
          >
            <h3 id="scan-scope-heading" className="text-sm font-semibold">
              What was checked
            </h3>
            <p className="text-text-muted mt-1 text-sm">
              {report.checked_languages.length > 0
                ? `Deterministic source scan: ${report.checked_languages.join(", ")}.`
                : "No supported source language could be checked."}
              {report.skipped_languages.length > 0 &&
                ` Skipped as unsupported: ${report.skipped_languages.join(", ")}.`}
            </p>
            {report.diagnostics.length > 0 && (
              <ul className="text-text-muted mt-2 list-disc space-y-1 pl-5 text-xs">
                {report.diagnostics.map((diagnostic) => (
                  <li key={diagnostic}>{diagnostic}</li>
                ))}
              </ul>
            )}
          </section>
          {/* Coverage and chart */}
          <div className="grid grid-cols-1 gap-6 md:grid-cols-2">
            <div className="border-border bg-surface-alt rounded-xl border p-6">
              <h3 className="text-text-muted mb-4 text-sm">Coverage</h3>
              <CoverageGauge
                coveragePercent={report.coverage_percent}
                total={report.total_requirements}
                covered={report.covered_requirements}
              />
            </div>
            <div className="border-border bg-surface-alt rounded-xl border p-6">
              <h3 className="text-text-muted mb-4 text-sm">Breakdown</h3>
              <AlignmentChart
                verified={report.verified_requirements}
                partial={report.partial_requirements}
                failed={report.failed_requirements}
                unknown={report.unknown_requirements}
                totalRequirements={report.total_requirements}
              />
            </div>
          </div>

          {/* Export */}
          <div className="flex gap-2">
            <span className="text-text-muted pt-1 text-sm">Export:</span>
            <button
              onClick={() => handleExport("bundle")}
              className="text-primary-light text-sm hover:underline"
            >
              Evidence bundle
            </button>
            <button
              onClick={() => handleExport("json")}
              className="text-primary-light text-sm hover:underline"
            >
              JSON
            </button>
            <button
              onClick={() => handleExport("html")}
              className="text-primary-light text-sm hover:underline"
            >
              HTML
            </button>
            <button
              onClick={() => handleExport("csv")}
              className="text-primary-light text-sm hover:underline"
            >
              CSV
            </button>
          </div>
          <p className="text-text-muted text-xs">
            Evidence bundles are self-hashed and include export-time freshness. They are unsigned
            and do not prove authorship or external attestation.
          </p>

          {/* Evidence */}
          <div>
            <h3 className="mb-3 text-lg font-semibold">
              Requirement evidence ({report.alignments.length})
            </h3>
            <EvidenceTable alignments={report.alignments} />
          </div>
        </div>
      ) : reports && reports.length === 0 ? (
        <div className="border-border bg-surface-alt rounded-xl border p-8 text-center">
          <p className="text-text-muted">
            No reports generated yet. Generate one to see alignment analysis.
          </p>
        </div>
      ) : null}
    </div>
  );
}
