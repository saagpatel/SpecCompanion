import { BrowserRouter, Routes, Route, Link } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { AppLayout } from "./components/layout/AppLayout";
import { ErrorBoundary } from "./components/layout/ErrorBoundary";
import { Dashboard } from "./pages/Dashboard";
import { Settings } from "./pages/Settings";
import { ProjectView } from "./pages/ProjectView";
import { SpecView } from "./pages/SpecView";
import { TestGeneration } from "./pages/TestGeneration";
import { TestExecution } from "./pages/TestExecution";
import { Reports } from "./pages/Reports";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      staleTime: 30_000,
      refetchOnWindowFocus: false,
    },
  },
});

function App() {
  return (
    <ErrorBoundary queryClient={queryClient}>
      <QueryClientProvider client={queryClient}>
        <BrowserRouter>
          <Routes>
            <Route element={<AppLayout />}>
              <Route path="/" element={<Dashboard />} />
              <Route path="/settings" element={<Settings />} />
              <Route path="/project/:projectId" element={<ProjectView />} />
              <Route path="/project/:projectId/spec/:specId" element={<SpecView />} />
              <Route path="/project/:projectId/generate" element={<TestGeneration />} />
              <Route path="/project/:projectId/execute" element={<TestExecution />} />
              <Route path="/project/:projectId/reports" element={<Reports />} />
              {import.meta.env.DEV && (
                <Route path="/__error-boundary" element={<ErrorBoundaryProbe />} />
              )}
              <Route
                path="*"
                element={
                  <div className="py-16 text-center">
                    <h2 className="mb-2 text-2xl font-bold">Page Not Found</h2>
                    <p className="text-text-muted mb-4">
                      The page you're looking for doesn't exist.
                    </p>
                    <Link to="/" className="text-primary-light hover:underline">
                      Back to Dashboard
                    </Link>
                  </div>
                }
              />
            </Route>
          </Routes>
        </BrowserRouter>
      </QueryClientProvider>
    </ErrorBoundary>
  );
}

function ErrorBoundaryProbe() {
  throw new Error("SpecCompanion error-boundary test probe");
  return null;
}

export default App;
