import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import * as api from "../lib/api";

export function useReports(projectId: string | undefined) {
  return useQuery({
    queryKey: ["reports", projectId],
    queryFn: () => api.listReports(projectId!),
    enabled: !!projectId,
  });
}

export function useAlignmentReport(id: string | undefined) {
  return useQuery({
    queryKey: ["alignment-report", id],
    queryFn: () => api.getAlignmentReport(id!),
    enabled: !!id,
  });
}

export function useGenerateAlignmentReport(projectId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => api.generateAlignmentReport(projectId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["reports", projectId] });
      queryClient.invalidateQueries({ queryKey: ["projects"] });
    },
  });
}

export function useExportReport() {
  return useMutation({
    mutationFn: ({
      reportId,
      format,
    }: {
      reportId: string;
      format: "json" | "html" | "csv" | "bundle";
    }) => api.exportReport(reportId, format),
  });
}
