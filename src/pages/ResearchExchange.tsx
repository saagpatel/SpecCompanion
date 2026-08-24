import { useState } from "react";
import {
  exportCanonicalResearchPackage,
  inspectResearchPackage,
  isTauriRuntime,
} from "../lib/api";
import type { ResearchPackageInspection } from "../lib/types";

const MAX_PACKAGE_BYTES = 5 * 1024 * 1024;

function downloadJson(contents: string, packageId: string, revisionId: string) {
  const blob = new Blob([contents], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `${packageId}-${revisionId}.research.json`;
  anchor.click();
  URL.revokeObjectURL(url);
}

export function ResearchExchange() {
  const [raw, setRaw] = useState("");
  const [inspection, setInspection] = useState<ResearchPackageInspection | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const inspect = async () => {
    setBusy(true);
    setError("");
    setInspection(null);
    try {
      setInspection(await inspectResearchPackage(raw));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const exportCanonical = async () => {
    if (!inspection) return;
    setBusy(true);
    setError("");
    try {
      const canonical = await exportCanonicalResearchPackage(raw);
      downloadJson(canonical, inspection.package_id, inspection.revision_id);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <header>
        <p className="text-primary-light text-sm font-medium">Evidence-centered exchange</p>
        <h2 className="text-text mt-1 text-2xl font-bold">Inspect a research package</h2>
        <p className="text-text-muted mt-2 max-w-3xl text-sm">
          Import reviewed P0/P1 JSON, verify source-lifecycle attestations, re-evaluate claims and
          conclusions, and export the retained canonical package. Research support never becomes
          native VERIFIED evidence without SpecCompanion&apos;s own requirement and test proof.
        </p>
      </header>

      {!isTauriRuntime() && (
        <div className="border-warning/40 bg-warning/10 text-text rounded-lg border px-4 py-3 text-sm">
          Browser preview is inspect-only UI. Cryptographic qualification runs only in the desktop
          runtime and remains unknown here.
        </div>
      )}

      <section className="bg-surface border-border rounded-xl border p-5">
        <label className="text-text block text-sm font-medium" htmlFor="research-package">
          Research package JSON
        </label>
        <label
          className="text-text-muted mt-3 block text-sm"
          htmlFor="research-package-file"
        >
          Load from a JSON file
        </label>
        <input
          id="research-package-file"
          className="text-text-muted mt-2 block w-full text-sm"
          type="file"
          accept="application/json,.json"
          onChange={async (event) => {
            const file = event.target.files?.[0];
            if (!file) return;
            if (file.size > MAX_PACKAGE_BYTES) {
              setError("Research package exceeds the 5 MiB local inspection limit.");
              return;
            }
            setRaw(await file.text());
            setInspection(null);
            setError("");
          }}
        />
        <textarea
          id="research-package"
          className="border-border bg-surface-alt text-text mt-3 min-h-64 w-full rounded-lg border p-3 font-mono text-xs outline-none focus:border-primary"
          value={raw}
          onChange={(event) => {
            setRaw(event.target.value);
            setInspection(null);
          }}
          placeholder='{"schema_version":"evidence-centered.research-package.v2", ...}'
          spellCheck={false}
        />
        <div className="mt-4 flex flex-wrap gap-3">
          <button
            type="button"
            className="bg-primary text-primary-foreground rounded-lg px-4 py-2 text-sm font-medium disabled:opacity-50"
            disabled={!raw.trim() || busy || !isTauriRuntime()}
            onClick={inspect}
          >
            {busy ? "Inspecting…" : "Verify and re-evaluate"}
          </button>
          <button
            type="button"
            className="border-border text-text rounded-lg border px-4 py-2 text-sm disabled:opacity-50"
            disabled={!inspection || busy}
            onClick={exportCanonical}
          >
            Export canonical JSON
          </button>
          <button
            type="button"
            className="text-text-muted rounded-lg px-3 py-2 text-sm"
            onClick={() => {
              setRaw("");
              setInspection(null);
              setError("");
            }}
          >
            Clear
          </button>
        </div>
        {error && (
          <p className="text-danger mt-3 text-sm" role="alert">
            {error}
          </p>
        )}
      </section>

      {inspection && (
        <div className="space-y-6" aria-live="polite">
          <section className="bg-surface border-border rounded-xl border p-5">
            <h3 className="text-text text-lg font-semibold">Package binding</h3>
            <dl className="mt-4 grid gap-4 text-sm md:grid-cols-2">
              {[
                ["Package", inspection.package_id],
                ["Revision", inspection.revision_id],
                ["Schema", inspection.schema_version],
                ["Schema digest", inspection.schema_digest],
                ["Package digest", inspection.package_digest],
                ["Projection losses", String(inspection.losses.length)],
              ].map(([label, value]) => (
                <div key={label}>
                  <dt className="text-text-muted">{label}</dt>
                  <dd className="text-text mt-1 break-all font-mono text-xs">{value}</dd>
                </div>
              ))}
            </dl>
          </section>

          <section className="bg-surface border-border rounded-xl border p-5">
            <h3 className="text-text text-lg font-semibold">Qualification</h3>
            <div className="mt-4 overflow-x-auto">
              <table className="w-full text-left text-sm">
                <thead className="text-text-muted border-border border-b">
                  <tr>
                    <th className="py-2 pr-4 font-medium">Claim</th>
                    <th className="py-2 pr-4 font-medium">Research state</th>
                    <th className="py-2 font-medium">Native alignment</th>
                  </tr>
                </thead>
                <tbody>
                  {inspection.qualification.map((claim) => (
                    <tr className="border-border border-b last:border-0" key={claim.claim_id}>
                      <td className="text-text py-2 pr-4 font-mono text-xs">{claim.claim_id}</td>
                      <td className="text-text py-2 pr-4">{claim.state}</td>
                      <td className="text-text py-2">
                        {inspection.alignment_projection[claim.claim_id]}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>

          <section className="bg-surface border-border rounded-xl border p-5">
            <h3 className="text-text text-lg font-semibold">Source lifecycle</h3>
            <ul className="mt-4 space-y-2 text-sm">
              {inspection.source_lifecycle.map((source) => (
                <li className="border-border flex flex-wrap justify-between gap-2 border-b pb-2" key={source.source_id}>
                  <span className="text-text font-mono text-xs">{source.source_id}</span>
                  <span className="text-text-muted">{source.state}</span>
                </li>
              ))}
            </ul>
          </section>

          <section className="bg-surface border-border rounded-xl border p-5">
            <h3 className="text-text text-lg font-semibold">Explicit compatibility losses</h3>
            <ul className="mt-4 space-y-3 text-sm">
              {inspection.losses.map((loss) => (
                <li className="border-border border-b pb-3 last:border-0" key={loss.path}>
                  <p className="text-text font-mono text-xs">{loss.path}</p>
                  <p className="text-text-muted mt-1">{loss.reason}</p>
                </li>
              ))}
            </ul>
          </section>
        </div>
      )}
    </div>
  );
}
