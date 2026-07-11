import { useRef, useState } from "react";
import { useProjects } from "../hooks/useProjects";
import { ProjectList } from "../components/project/ProjectList";
import { CreateProjectDialog } from "../components/project/CreateProjectDialog";

export function Dashboard() {
  const [showCreate, setShowCreate] = useState(false);
  const { data: projects, isLoading, error } = useProjects();
  const newProjectButton = useRef<HTMLButtonElement>(null);
  const closeCreate = () => {
    setShowCreate(false);
    requestAnimationFrame(() => newProjectButton.current?.focus());
  };

  return (
    <div>
      <div className="mb-6 flex items-center justify-between">
        <h2 className="text-2xl font-bold">Dashboard</h2>
        <button
          ref={newProjectButton}
          onClick={() => setShowCreate(true)}
          className="bg-primary hover:bg-primary-dark rounded-lg px-4 py-2 text-sm text-white transition-colors"
        >
          New Project
        </button>
      </div>

      {isLoading && <p className="text-text-muted">Loading projects...</p>}
      {error && (
        <div className="bg-danger/10 border-danger/30 text-danger rounded-lg border p-3 text-sm">
          {String(error)}
        </div>
      )}
      {projects && <ProjectList projects={projects} />}
      {showCreate && <CreateProjectDialog onClose={closeCreate} />}
    </div>
  );
}
