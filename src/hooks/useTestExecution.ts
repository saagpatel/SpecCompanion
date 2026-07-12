import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import * as api from "../lib/api";

export function useTestResults(projectId: string | undefined) {
  return useQuery({
    queryKey: ["test-results", projectId],
    queryFn: () => api.getTestResults(projectId!),
    enabled: !!projectId,
  });
}

export function useTestResult(id: string | undefined) {
  return useQuery({
    queryKey: ["test-result", id],
    queryFn: () => api.getTestResult(id!),
    enabled: !!id,
  });
}

export function useExecuteTests(projectId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (testIds: string[]) => api.executeTests(projectId, testIds),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["test-results", projectId] });
      queryClient.invalidateQueries({ queryKey: ["projects"] });
      queryClient.invalidateQueries({ queryKey: ["project", projectId] });
      queryClient.invalidateQueries({ queryKey: ["reports", projectId] });
    },
  });
}

export function usePythonRuntime(projectId: string) {
  return useQuery({
    queryKey: ["python-runtime", projectId],
    queryFn: () => api.getProjectPythonRuntimeStatus(projectId),
    enabled: Boolean(projectId),
  });
}

export function useConfigurePythonRuntime(projectId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ root, profile }: { root: string; profile: "bounded" | "macos_isolated" }) =>
      api.configureProjectPythonRuntime(projectId, root, profile),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["python-runtime", projectId] }),
  });
}

export function useClearPythonRuntime(projectId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => api.clearProjectPythonRuntime(projectId),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["python-runtime", projectId] }),
  });
}
