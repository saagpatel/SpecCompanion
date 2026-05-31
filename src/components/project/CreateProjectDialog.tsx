import { useState, useEffect } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { isTauriRuntime } from "../../lib/api";
import { useCreateProject } from "../../hooks/useProjects";

interface Props {
  onClose: () => void;
}

export function CreateProjectDialog({ onClose }: Props) {
  const [name, setName] = useState("");
  const [codebasePath, setCodebasePath] = useState("");
  const createProject = useCreateProject();
  const canBrowse = isTauriRuntime();

  const handleSelectFolder = async () => {
    if (!canBrowse) return;
    const selected = await open({ directory: true, multiple: false });
    if (selected && typeof selected === "string") {
      setCodebasePath(selected);
    }
  };

  // Close on Escape key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !createProject.isPending) onClose();
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onClose, createProject.isPending]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !codebasePath.trim()) return;
    createProject.mutate(
      { name: name.trim(), codebase_path: codebasePath.trim() },
      { onSuccess: onClose },
    );
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={() => !createProject.isPending && onClose()}
    >
      <div
        className="bg-surface-alt border-border w-full max-w-md rounded-xl border p-6"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="mb-4 text-lg font-semibold">New Project</h3>
        {createProject.isError && (
          <div className="border-danger/30 bg-danger/5 text-danger mb-4 rounded-lg border p-3 text-sm">
            {String(createProject.error)}
          </div>
        )}
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="text-text-muted mb-1 block text-sm">Project Name</label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="My Project"
              className="bg-surface border-border text-text focus:border-primary w-full rounded-lg border px-3 py-2 text-sm focus:outline-none"
              autoFocus
            />
          </div>
          <div>
            <label className="text-text-muted mb-1 block text-sm">Codebase Path</label>
            <div className="flex gap-2">
              <input
                type="text"
                value={codebasePath}
                onChange={(e) => setCodebasePath(e.target.value)}
                placeholder="/path/to/project"
                className="bg-surface border-border text-text focus:border-primary flex-1 rounded-lg border px-3 py-2 text-sm focus:outline-none"
              />
              <button
                type="button"
                onClick={handleSelectFolder}
                disabled={!canBrowse}
                className="bg-surface border-border text-text-muted hover:bg-surface-hover rounded-lg border px-3 py-2 text-sm transition-colors"
              >
                Browse
              </button>
            </div>
          </div>
          <div className="flex justify-end gap-2 pt-2">
            <button
              type="button"
              onClick={onClose}
              className="text-text-muted hover:text-text px-4 py-2 text-sm transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={!name.trim() || !codebasePath.trim() || createProject.isPending}
              className="bg-primary hover:bg-primary-dark rounded-lg px-4 py-2 text-sm text-white transition-colors disabled:opacity-50"
            >
              {createProject.isPending ? "Creating..." : "Create"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
