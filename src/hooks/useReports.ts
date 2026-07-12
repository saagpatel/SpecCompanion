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

export function useVerifyEvidenceBundle(projectId?: string) {
  return useMutation({
    mutationFn: (bundleJson: string) => api.verifyEvidenceBundle(bundleJson, projectId),
  });
}

export function useSetSignerTrust(projectId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      fingerprint,
      identity,
      status,
      provenance,
    }: {
      fingerprint: string;
      identity: string;
      status: "trusted" | "revoked";
      provenance: string;
    }) => api.setSignerTrust(projectId, fingerprint, identity, status, provenance),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["signer-trust", projectId] });
      queryClient.invalidateQueries({ queryKey: ["signer-trust-history", projectId] });
      queryClient.invalidateQueries({ queryKey: ["signer-trust-history-integrity", projectId] });
    },
  });
}

export function useSignerTrust(projectId: string | undefined) {
  return useQuery({
    queryKey: ["signer-trust", projectId],
    queryFn: () => api.listSignerTrust(projectId!),
    enabled: !!projectId,
  });
}

export function useSignerTrustHistory(projectId: string | undefined) {
  return useQuery({
    queryKey: ["signer-trust-history", projectId],
    queryFn: () => api.listSignerTrustHistory(projectId!),
    enabled: !!projectId,
  });
}

export function useSignerTrustHistoryIntegrity(projectId: string | undefined) {
  return useQuery({
    queryKey: ["signer-trust-history-integrity", projectId],
    queryFn: () => api.getSignerTrustHistoryIntegrity(projectId!),
    enabled: !!projectId,
  });
}

export function useRotateSignerTrust(projectId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      previousFingerprint,
      newFingerprint,
      newIdentity,
      provenance,
    }: {
      previousFingerprint: string;
      newFingerprint: string;
      newIdentity: string;
      provenance: string;
    }) =>
      api.rotateSignerTrust(
        projectId,
        previousFingerprint,
        newFingerprint,
        newIdentity,
        provenance,
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["signer-trust", projectId] });
      queryClient.invalidateQueries({ queryKey: ["signer-trust-history", projectId] });
      queryClient.invalidateQueries({ queryKey: ["signer-trust-history-integrity", projectId] });
    },
  });
}

export function useExportSignerTrustPolicy() {
  return useMutation({
    mutationFn: ({ projectId, identity }: { projectId: string; identity: string }) =>
      api.exportSignerTrustPolicy(projectId, identity),
  });
}

export function useVerifySignerTrustPolicy(projectId: string) {
  return useMutation({
    mutationFn: (bundleJson: string) => api.verifySignerTrustPolicy(projectId, bundleJson),
  });
}

export function useImportSignerTrustPolicy(projectId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      bundleJson,
      fingerprint,
      payloadSha256,
      provenance,
    }: {
      bundleJson: string;
      fingerprint: string;
      payloadSha256: string;
      provenance: string;
    }) =>
      api.importSignerTrustPolicy(projectId, bundleJson, fingerprint, payloadSha256, provenance),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["signer-trust", projectId] });
      queryClient.invalidateQueries({ queryKey: ["signer-trust-history", projectId] });
      queryClient.invalidateQueries({ queryKey: ["signer-trust-history-integrity", projectId] });
    },
  });
}

export function useAdvanceTrustAnchorWitness(projectId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      bundleJson,
      fingerprint,
      payloadSha256,
      provenance,
    }: {
      bundleJson: string;
      fingerprint: string;
      payloadSha256: string;
      provenance: string;
    }) =>
      api.advanceTrustAnchorWitness(projectId, bundleJson, fingerprint, payloadSha256, provenance),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["trust-anchor-advancements", projectId] });
    },
  });
}

export function useTrustAnchorAdvancements(projectId: string | undefined) {
  return useQuery({
    queryKey: ["trust-anchor-advancements", projectId],
    queryFn: () => api.listTrustAnchorAdvancements(projectId!),
    enabled: !!projectId,
  });
}

export function useExportTrustAnchorAdvancements() {
  return useMutation({
    mutationFn: (projectId: string) => api.exportTrustAnchorAdvancements(projectId),
  });
}

export function useCreateSigningIdentity() {
  return useMutation({ mutationFn: (identity: string) => api.createSigningIdentity(identity) });
}

export function useExportSignedEvidenceBundle() {
  return useMutation({
    mutationFn: ({ reportId, identity }: { reportId: string; identity: string }) =>
      api.exportSignedEvidenceBundle(reportId, identity),
  });
}
