import { useState, useEffect } from "react";
import { useParams, Link } from "react-router-dom";
import { useProject } from "../hooks/useProjects";
import {
  useReports,
  useAlignmentReport,
  useGenerateAlignmentReport,
  useExportReport,
  useVerifyEvidenceBundle,
  useCreateSigningIdentity,
  useExportSignedEvidenceBundle,
  useSetSignerTrust,
  useSignerTrust,
  useSignerTrustHistory,
  useSignerTrustHistoryIntegrity,
  useRotateSignerTrust,
  useExportSignerTrustPolicy,
  useVerifySignerTrustPolicy,
  useImportSignerTrustPolicy,
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
  const verifyBundle = useVerifyEvidenceBundle(projectId);
  const createSigner = useCreateSigningIdentity();
  const exportSigned = useExportSignedEvidenceBundle();
  const [signerIdentity, setSignerIdentity] = useState("");
  const [trustProvenance, setTrustProvenance] = useState("");
  const signerTrust = useSetSignerTrust(projectId ?? "");
  const trustPolicies = useSignerTrust(projectId);
  const trustHistory = useSignerTrustHistory(projectId);
  const trustHistoryIntegrity = useSignerTrustHistoryIntegrity(projectId);
  const rotateTrust = useRotateSignerTrust(projectId ?? "");
  const [previousFingerprint, setPreviousFingerprint] = useState("");
  const [policySignerIdentity, setPolicySignerIdentity] = useState("");
  const [recoveryBundle, setRecoveryBundle] = useState("");
  const [recoveryFingerprint, setRecoveryFingerprint] = useState("");
  const [recoveryProvenance, setRecoveryProvenance] = useState("");
  const [recoveryFileError, setRecoveryFileError] = useState("");
  const exportTrustPolicy = useExportSignerTrustPolicy();
  const verifyTrustPolicy = useVerifySignerTrustPolicy(projectId ?? "");
  const importTrustPolicy = useImportSignerTrustPolicy(projectId ?? "");
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
            <p>Signer trust: {verifyBundle.data.trust_status}.</p>
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
            {verifyBundle.data.key_fingerprint && verifyBundle.data.signer_identity && (
              <div className="border-border mt-3 border-t pt-3">
                <p className="text-xs break-all">
                  Fingerprint: {verifyBundle.data.key_fingerprint}
                </p>
                <label htmlFor="trust-provenance" className="mt-2 block text-xs">
                  Trust decision provenance
                </label>
                <input
                  id="trust-provenance"
                  value={trustProvenance}
                  onChange={(event) => setTrustProvenance(event.target.value)}
                  placeholder="How was this fingerprint verified?"
                  className="bg-surface border-border mt-1 w-full rounded border px-3 py-2 text-sm"
                />
                <div className="mt-2 flex gap-3">
                  {(["trusted", "revoked"] as const).map((status) => (
                    <button
                      key={status}
                      type="button"
                      disabled={!trustProvenance.trim() || signerTrust.isPending}
                      onClick={() =>
                        signerTrust.mutate({
                          fingerprint: verifyBundle.data!.key_fingerprint!,
                          identity: verifyBundle.data!.signer_identity!,
                          status,
                          provenance: trustProvenance,
                        })
                      }
                      className="text-primary-light text-sm capitalize hover:underline disabled:opacity-50"
                    >
                      Mark {status}
                    </button>
                  ))}
                </div>
                {signerTrust.data && (
                  <p role="status" className="mt-2 text-xs">
                    Project trust policy updated: {signerTrust.data.status}. Re-verify the bundle to
                    apply it.
                  </p>
                )}
              </div>
            )}
          </div>
        )}
        {verifyBundle.isError && (
          <p role="alert" className="text-danger mt-3 text-sm">
            The selected file could not be verified. No evidence was imported.
          </p>
        )}
      </section>

      <section
        aria-labelledby="signer-trust-heading"
        className="border-border bg-surface-alt mb-6 rounded-xl border p-4"
      >
        <h3 id="signer-trust-heading" className="text-sm font-semibold">
          Project signer trust
        </h3>
        <p className="text-text-muted mt-1 text-sm">
          Trust applies only to this project and exact fingerprints. Every decision is linked by a
          tamper-evident digest chain.
        </p>
        {(trustPolicies.isLoading || trustHistory.isLoading || trustHistoryIntegrity.isLoading) && (
          <p role="status" className="text-text-muted mt-3 text-sm">
            Loading signer trust…
          </p>
        )}
        {(trustPolicies.isError || trustHistory.isError || trustHistoryIntegrity.isError) && (
          <p role="alert" className="text-danger mt-3 text-sm">
            Signer trust records are unavailable. No signer should be assumed trusted.
          </p>
        )}
        {trustPolicies.data && trustPolicies.data.length === 0 && (
          <p className="text-text-muted mt-3 text-sm">
            No fingerprints have been trusted or revoked.
          </p>
        )}
        {trustPolicies.data && trustPolicies.data.length > 0 && (
          <ul aria-label="Current signer trust policies" className="mt-3 space-y-2">
            {trustPolicies.data.map((policy) => (
              <li key={policy.key_fingerprint} className="border-border rounded border p-3 text-sm">
                <p>
                  <strong>{policy.signer_identity}</strong> — {policy.status}
                </p>
                <p className="text-text-muted text-xs break-all">{policy.key_fingerprint}</p>
                <p className="text-text-muted text-xs">{policy.provenance}</p>
              </li>
            ))}
          </ul>
        )}
        {verifyBundle.data?.key_fingerprint &&
          verifyBundle.data.signer_identity &&
          trustPolicies.data?.some(
            (policy) =>
              policy.status === "trusted" &&
              policy.key_fingerprint !== verifyBundle.data!.key_fingerprint,
          ) && (
            <div className="border-border mt-4 border-t pt-4">
              <h4 className="text-sm font-medium">Rotate to the verified fingerprint</h4>
              <p className="text-text-muted mt-1 text-xs">
                This atomically revokes the selected old key and trusts the verified new key.
              </p>
              <label htmlFor="previous-fingerprint" className="mt-2 block text-xs">
                Currently trusted key
              </label>
              <select
                id="previous-fingerprint"
                value={previousFingerprint}
                onChange={(event) => setPreviousFingerprint(event.target.value)}
                className="bg-surface border-border mt-1 w-full rounded border px-3 py-2 text-sm"
              >
                <option value="">Select the key being replaced</option>
                {trustPolicies.data
                  .filter((policy) => policy.status === "trusted")
                  .map((policy) => (
                    <option key={policy.key_fingerprint} value={policy.key_fingerprint}>
                      {policy.signer_identity} — {policy.key_fingerprint.slice(0, 12)}…
                    </option>
                  ))}
              </select>
              <button
                type="button"
                disabled={!previousFingerprint || !trustProvenance.trim() || rotateTrust.isPending}
                onClick={() =>
                  rotateTrust.mutate({
                    previousFingerprint,
                    newFingerprint: verifyBundle.data!.key_fingerprint!,
                    newIdentity: verifyBundle.data!.signer_identity!,
                    provenance: trustProvenance,
                  })
                }
                className="text-primary-light mt-2 text-sm hover:underline disabled:opacity-50"
              >
                Rotate trust atomically
              </button>
            </div>
          )}
        {rotateTrust.isError && (
          <p role="alert" className="text-danger mt-3 text-xs">
            Rotation failed; the previous trust policy remains in force.
          </p>
        )}
        {rotateTrust.isSuccess && (
          <p role="status" className="mt-3 text-xs">
            Rotation recorded. Re-verify the bundle to apply the new policy.
          </p>
        )}
        <div className="border-border mt-4 border-t pt-4">
          <h4 className="text-sm font-medium">Portable recovery policy</h4>
          <p className="text-text-muted mt-1 text-xs">
            Exports are signed. Imports verify integrity first, but recovery authority still
            requires an out-of-band fingerprint match and recorded provenance.
          </p>
          <label htmlFor="policy-signer-identity" className="mt-2 block text-xs">
            Keychain signing identity
          </label>
          <input
            id="policy-signer-identity"
            value={policySignerIdentity}
            onChange={(event) => setPolicySignerIdentity(event.target.value)}
            className="bg-surface border-border mt-1 w-full rounded border px-3 py-2 text-sm"
          />
          <button
            type="button"
            disabled={
              !policySignerIdentity.trim() ||
              !trustPolicies.data?.length ||
              exportTrustPolicy.isPending
            }
            onClick={() =>
              exportTrustPolicy.mutate(
                { projectId: projectId!, identity: policySignerIdentity },
                {
                  onSuccess: (content) => {
                    const url = URL.createObjectURL(
                      new Blob([content], { type: "application/json" }),
                    );
                    const anchor = document.createElement("a");
                    anchor.href = url;
                    anchor.download = "speccompanion-signer-trust-policy.json";
                    anchor.click();
                    URL.revokeObjectURL(url);
                  },
                },
              )
            }
            className="text-primary-light mt-2 text-sm hover:underline disabled:opacity-50"
          >
            Export signed trust policy
          </button>
          {exportTrustPolicy.isError && (
            <p role="alert" className="text-danger mt-2 text-xs">
              Signed policy export failed. No recovery package was created.
            </p>
          )}

          <label className="text-primary-light mt-4 block cursor-pointer text-sm hover:underline">
            Verify recovery policy JSON
            <input
              type="file"
              accept="application/json,.json"
              className="sr-only"
              onChange={async (event) => {
                const file = event.target.files?.[0];
                if (!file) return;
                if (file.size > 1_048_576) {
                  setRecoveryFileError("Recovery policy exceeds the 1 MiB size limit.");
                  setRecoveryBundle("");
                  event.target.value = "";
                  return;
                }
                setRecoveryFileError("");
                const content = await file.text();
                setRecoveryBundle(content);
                setRecoveryFingerprint("");
                verifyTrustPolicy.mutate(content);
                event.target.value = "";
              }}
            />
          </label>
          {recoveryFileError && (
            <p role="alert" className="text-danger mt-2 text-xs">
              {recoveryFileError}
            </p>
          )}
          {verifyTrustPolicy.data && (
            <div role="status" className="border-border mt-2 rounded border p-3 text-xs">
              <p>
                <strong>Recovery policy: {verifyTrustPolicy.data.status}</strong>
              </p>
              <p>
                Source: {verifyTrustPolicy.data.source_project_name ?? "unknown"}. Policies:{" "}
                {verifyTrustPolicy.data.policy_count}.
              </p>
              {verifyTrustPolicy.data.key_fingerprint && (
                <p className="break-all">
                  Package signer: {verifyTrustPolicy.data.key_fingerprint}
                </p>
              )}
              {verifyTrustPolicy.data.payload_sha256 && (
                <p className="break-all">Payload digest: {verifyTrustPolicy.data.payload_sha256}</p>
              )}
              {verifyTrustPolicy.data.source_history_head_digest && (
                <p className="break-all">
                  Signed history anchor: {verifyTrustPolicy.data.source_history_head_digest} after{" "}
                  {verifyTrustPolicy.data.source_history_event_count} decisions.
                </p>
              )}
              {verifyTrustPolicy.data.diagnostics.map((diagnostic) => (
                <p key={diagnostic}>{diagnostic}</p>
              ))}
              {verifyTrustPolicy.data.conflicts.length > 0 && (
                <ul aria-label="Recovery policy changes" className="mt-2 space-y-1">
                  {verifyTrustPolicy.data.conflicts.map((conflict) => (
                    <li key={conflict.key_fingerprint}>
                      <strong>{conflict.action}</strong> {conflict.signer_identity}:{" "}
                      {conflict.current_status ?? "absent"} → {conflict.incoming_status}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
          {verifyTrustPolicy.data?.status === "valid_untrusted" &&
            verifyTrustPolicy.data.key_fingerprint && (
              <div className="mt-3">
                <label htmlFor="recovery-fingerprint" className="block text-xs">
                  Confirm package signer fingerprint
                </label>
                <input
                  id="recovery-fingerprint"
                  value={recoveryFingerprint}
                  onChange={(event) => setRecoveryFingerprint(event.target.value)}
                  className="bg-surface border-border mt-1 w-full rounded border px-3 py-2 font-mono text-xs"
                />
                <label htmlFor="recovery-provenance" className="mt-2 block text-xs">
                  Recovery verification provenance
                </label>
                <input
                  id="recovery-provenance"
                  value={recoveryProvenance}
                  onChange={(event) => setRecoveryProvenance(event.target.value)}
                  placeholder="Where was the expected fingerprint obtained?"
                  className="bg-surface border-border mt-1 w-full rounded border px-3 py-2 text-sm"
                />
                <button
                  type="button"
                  disabled={
                    recoveryFingerprint.trim().toLowerCase() !==
                      verifyTrustPolicy.data.key_fingerprint.toLowerCase() ||
                    !recoveryProvenance.trim() ||
                    importTrustPolicy.isPending
                  }
                  onClick={() =>
                    importTrustPolicy.mutate({
                      bundleJson: recoveryBundle,
                      fingerprint: recoveryFingerprint,
                      payloadSha256: verifyTrustPolicy.data!.payload_sha256!,
                      provenance: recoveryProvenance,
                    })
                  }
                  className="text-primary-light mt-2 text-sm hover:underline disabled:opacity-50"
                >
                  Recover verified policy
                </button>
              </div>
            )}
          {importTrustPolicy.isError && (
            <p role="alert" className="text-danger mt-2 text-xs">
              Recovery failed. Existing project trust remains unchanged.
            </p>
          )}
          {importTrustPolicy.isSuccess && (
            <p role="status" className="mt-2 text-xs">
              Recovered {importTrustPolicy.data.length} signer policies with immutable history.
            </p>
          )}
        </div>
        {trustHistoryIntegrity.data && (
          <div
            role="status"
            className={`mt-4 rounded border p-3 text-xs ${trustHistoryIntegrity.data.status === "verified" ? "border-success/30 bg-success/5 text-success" : "border-warning/30 bg-warning/5 text-warning"}`}
          >
            Trust history integrity: <strong>{trustHistoryIntegrity.data.status}</strong> ·{" "}
            {trustHistoryIntegrity.data.event_count} decisions checked.
            {trustHistoryIntegrity.data.status !== "verified" &&
              " Stored trust is ignored and recovery is blocked until integrity can be established."}
          </div>
        )}
        <details className="mt-4">
          <summary className="text-primary-light cursor-pointer text-sm">
            Decision history ({trustHistory.data?.length ?? 0})
          </summary>
          {trustHistory.data && trustHistory.data.length === 0 ? (
            <p className="text-text-muted mt-2 text-sm">No trust decisions recorded.</p>
          ) : (
            <ol className="mt-2 space-y-2">
              {trustHistory.data?.map((entry) => (
                <li key={entry.id} className="border-border border-l-2 pl-3 text-xs">
                  <p>
                    <strong>{entry.status}</strong> {entry.signer_identity}
                  </p>
                  <p className="text-text-muted break-all">{entry.key_fingerprint}</p>
                  <p className="text-text-muted">
                    {entry.provenance} · {new Date(entry.recorded_at).toLocaleString()}
                  </p>
                </li>
              ))}
            </ol>
          )}
        </details>
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
          <section
            aria-labelledby="signed-export-heading"
            className="border-border rounded-lg border p-4"
          >
            <h3 id="signed-export-heading" className="text-sm font-semibold">
              Optional signed bundle
            </h3>
            <p className="text-text-muted mt-1 text-xs">
              The private Ed25519 key stays in the OS Keychain. A valid signature proves key
              possession; the identity label remains untrusted until its fingerprint is verified
              separately.
            </p>
            <label className="text-text-muted mt-3 block text-xs" htmlFor="signer-identity">
              Signer identity label
            </label>
            <input
              id="signer-identity"
              value={signerIdentity}
              onChange={(event) => setSignerIdentity(event.target.value)}
              maxLength={120}
              className="bg-surface border-border mt-1 w-full rounded border px-3 py-2 text-sm"
            />
            <div className="mt-3 flex flex-wrap gap-2">
              <button
                type="button"
                disabled={!signerIdentity.trim() || createSigner.isPending}
                onClick={() => createSigner.mutate(signerIdentity)}
                className="text-primary-light text-sm hover:underline disabled:opacity-50"
              >
                Create Keychain identity
              </button>
              <button
                type="button"
                disabled={!selectedReportId || !signerIdentity.trim() || exportSigned.isPending}
                onClick={() =>
                  exportSigned.mutate(
                    { reportId: selectedReportId!, identity: signerIdentity },
                    {
                      onSuccess: (content) => {
                        const url = URL.createObjectURL(
                          new Blob([content], { type: "application/json" }),
                        );
                        const anchor = document.createElement("a");
                        anchor.href = url;
                        anchor.download = "signed-alignment-evidence-bundle.json";
                        anchor.click();
                        URL.revokeObjectURL(url);
                      },
                    },
                  )
                }
                className="text-primary-light text-sm hover:underline disabled:opacity-50"
              >
                Export signed bundle
              </button>
            </div>
            {createSigner.data && (
              <p role="status" className="text-success mt-3 text-xs break-all">
                Keychain identity created. Fingerprint: {createSigner.data.key_fingerprint}
              </p>
            )}
            {(createSigner.isError || exportSigned.isError) && (
              <p role="alert" className="text-danger mt-3 text-xs">
                The signing operation failed. No unsigned fallback was substituted.
              </p>
            )}
          </section>

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
