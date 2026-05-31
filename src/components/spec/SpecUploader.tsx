import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { isTauriRuntime, readFileContent } from "../../lib/api";
import { useUploadSpec } from "../../hooks/useSpecs";

interface Props {
  projectId: string;
}

export function SpecUploader({ projectId }: Props) {
  const uploadSpec = useUploadSpec(projectId);
  const [browserFilename, setBrowserFilename] = useState("qa-spec.md");
  const [browserContent, setBrowserContent] = useState(`# Checkout Flow Spec

## Requirements

- The system shall let a user create a project from a local codebase path
- The system shall parse uploaded markdown specifications into requirements
- As a user, I want generated tests for selected requirements

## Non-Functional Requirements

- The system shall keep the dashboard responsive during preview QA
`);
  const canUseNativeDialog = isTauriRuntime();

  const handleUpload = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "Markdown", extensions: ["md", "txt", "markdown"] }],
      });
      if (!selected || typeof selected !== "string") return;

      const filename = selected.split(/[/\\]/).pop() || selected;
      const content = await readFileContent(selected);
      uploadSpec.mutate({ filename, content });
    } catch {
      // Dialog cancelled or file read failed — mutation error state handles display
    }
  };

  const handleBrowserUpload = () => {
    if (!browserFilename.trim() || !browserContent.trim()) return;

    uploadSpec.mutate({
      filename: browserFilename.trim(),
      content: browserContent,
    });
  };

  if (!canUseNativeDialog) {
    return (
      <div className="flex flex-col gap-2 sm:w-96">
        {uploadSpec.isError && <span className="text-danger text-xs">Upload failed</span>}
        <input
          type="text"
          value={browserFilename}
          onChange={(event) => setBrowserFilename(event.target.value)}
          className="bg-surface border-border text-text focus:border-primary w-full rounded-lg border px-3 py-2 text-sm focus:outline-none"
          aria-label="Spec filename"
        />
        <textarea
          value={browserContent}
          onChange={(event) => setBrowserContent(event.target.value)}
          rows={5}
          className="bg-surface border-border text-text focus:border-primary w-full rounded-lg border px-3 py-2 text-sm focus:outline-none"
          aria-label="Spec content"
        />
        <button
          onClick={handleBrowserUpload}
          disabled={!browserFilename.trim() || !browserContent.trim() || uploadSpec.isPending}
          className="bg-primary hover:bg-primary-dark self-end rounded-lg px-4 py-2 text-sm text-white transition-colors disabled:opacity-50"
        >
          {uploadSpec.isPending ? "Uploading..." : "Upload Spec"}
        </button>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-2">
      {uploadSpec.isError && <span className="text-danger text-xs">Upload failed</span>}
      <button
        onClick={handleUpload}
        disabled={uploadSpec.isPending}
        className="bg-primary hover:bg-primary-dark rounded-lg px-4 py-2 text-sm text-white transition-colors disabled:opacity-50"
      >
        {uploadSpec.isPending ? "Uploading..." : "Upload Spec"}
      </button>
    </div>
  );
}
