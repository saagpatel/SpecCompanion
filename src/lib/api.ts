import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type {
  Project,
  CreateProjectRequest,
  ProjectWithStats,
  Spec,
  Requirement,
  ParsedSpec,
  GeneratedTest,
  GenerateTestsRequest,
  TestResult,
  AlignmentReport,
  AlignmentReportWithEvidence,
  AppSettings,
  EvidenceRecord,
  LinkRepositoryTestRequest,
  RepositoryTestCandidate,
} from "./types";

type InvokeArgs = Record<string, unknown>;

interface MockState {
  projects: Project[];
  specs: Spec[];
  requirements: Requirement[];
  generatedTests: GeneratedTest[];
  testResults: TestResult[];
  reports: AlignmentReportWithEvidence[];
  settings: AppSettings;
}

const mockState: MockState = {
  projects: [],
  specs: [],
  requirements: [],
  generatedTests: [],
  testResults: [],
  reports: [],
  settings: {
    api_key: "",
    default_framework: "jest",
    default_mode: "template",
    scan_exclusions: ["node_modules", "dist", ".git"],
    python_environment_root: "",
  },
};

let mockId = 0;

export const isTauriRuntime = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function now() {
  return new Date().toISOString();
}

function nextId(prefix: string) {
  mockId += 1;
  return `mock-${prefix}-${mockId}`;
}

function withStats(project: Project): ProjectWithStats {
  const specs = mockState.specs.filter((spec) => spec.project_id === project.id);
  const reports = mockState.reports.filter((report) => report.project_id === project.id);
  const latestReport = reports[reports.length - 1];
  const latestResult = mockState.testResults[mockState.testResults.length - 1];

  return {
    ...project,
    spec_count: specs.length,
    coverage_percent: latestReport?.coverage_percent ?? null,
    last_run_at: latestResult?.executed_at ?? null,
  };
}

function parseRequirements(specId: string, content: string): Requirement[] {
  const requirements: Requirement[] = [];
  let section = "General";
  let inRequirementSection = false;

  for (const [lineIndex, line] of content.split(/\r?\n/).entries()) {
    const heading = line.match(/^#{1,3}\s+(.+)$/);
    if (heading) {
      section = heading[1].trim();
      const lower = section.toLowerCase();
      inRequirementSection =
        lower.includes("requirement") ||
        lower.includes("user stor") ||
        lower.includes("feature") ||
        lower.includes("functional") ||
        lower.includes("specification") ||
        lower.includes("capability") ||
        lower.includes("constraint") ||
        lower.includes("acceptance criteria") ||
        lower.includes("use case");
      continue;
    }

    const item = line.match(/^\s*[-*]\s+(.+)$/);
    if (!item) continue;

    const description = item[1].trim();
    const lower = description.toLowerCase();
    const looksLikeRequirement =
      lower.startsWith("as a ") ||
      lower.startsWith("the system shall ") ||
      lower.startsWith("the system must ") ||
      lower.startsWith("the application shall ") ||
      lower.startsWith("the application must ") ||
      lower.startsWith("shall ") ||
      lower.startsWith("must ") ||
      lower.includes("**shall**") ||
      lower.includes("**must**");

    if (!inRequirementSection && !looksLikeRequirement) continue;

    const lowerSection = section.toLowerCase();
    const reqType =
      lowerSection.includes("non-functional") ||
      lowerSection.includes("performance") ||
      lowerSection.includes("security") ||
      lower.includes("performance") ||
      lower.includes("latency") ||
      lower.includes("availability")
        ? "non_functional"
        : lowerSection.includes("constraint") ||
            lower.includes("constraint") ||
            lower.includes("limitation")
          ? "constraint"
          : "functional";
    const priority =
      lower.includes("critical") || lower.includes("must have") || lower.includes("**must**")
        ? "high"
        : lower.includes("nice to have") || lower.includes("optional") || lower.includes("could")
          ? "low"
          : "medium";

    requirements.push({
      id: nextId("req"),
      spec_id: specId,
      section,
      description,
      req_type: reqType,
      priority,
      content_fingerprint: `${section.toLowerCase()}::${description.toLowerCase()}::${requirements.length}`,
      source_line_start: lineIndex + 1,
      source_line_end: lineIndex + 1,
    });
  }

  return requirements;
}

function generateMockTest(requirement: Requirement, req: GenerateTestsRequest): GeneratedTest {
  const testName = requirement.description
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim()
    .split(" ")
    .slice(0, 8)
    .join("_");
  const code =
    req.framework === "pytest"
      ? `# Requirement-ID: ${requirement.id}\ndef test_${testName || "requirement"}():\n    \"\"\"${requirement.description}\"\"\"\n    assert True\n`
      : `// Requirement-ID: ${requirement.id}\ndescribe("${requirement.section}", () => {\n  it("${requirement.description}", () => {\n    expect(true).toBe(true);\n  });\n});\n`;

  return {
    id: nextId("test"),
    requirement_id: requirement.id,
    framework: req.framework,
    code,
    generation_mode: req.mode,
    file_path: null,
    created_at: now(),
  };
}

async function mockInvoke<T>(command: string, args: InvokeArgs = {}): Promise<T> {
  switch (command) {
    case "create_project": {
      const request = args.request as CreateProjectRequest;
      const createdAt = now();
      const project: Project = {
        id: nextId("project"),
        name: request.name,
        codebase_path: request.codebase_path,
        created_at: createdAt,
        updated_at: createdAt,
      };
      mockState.projects.push(project);
      return project as T;
    }
    case "list_projects":
      return mockState.projects.map(withStats) as T;
    case "get_project": {
      const project = mockState.projects.find((item) => item.id === args.id);
      if (!project) throw new Error("Project not found");
      return withStats(project) as T;
    }
    case "delete_project": {
      const id = args.id as string;
      mockState.projects = mockState.projects.filter((item) => item.id !== id);
      mockState.specs = mockState.specs.filter((item) => item.project_id !== id);
      mockState.reports = mockState.reports.filter((item) => item.project_id !== id);
      return undefined as T;
    }
    case "validate_path":
      return Boolean(args.path) as T;
    case "upload_spec": {
      const spec: Spec = {
        id: nextId("spec"),
        project_id: args.project_id as string,
        filename: args.filename as string,
        content: args.content as string,
        parsed_at: now(),
        created_at: now(),
      };
      const requirements = parseRequirements(spec.id, spec.content);
      mockState.specs.push(spec);
      mockState.requirements.push(...requirements);
      return { spec, requirements } as T;
    }
    case "get_spec": {
      const spec = mockState.specs.find((item) => item.id === args.id);
      if (!spec) throw new Error("Spec not found");
      return {
        spec,
        requirements: mockState.requirements.filter((item) => item.spec_id === spec.id),
      } as T;
    }
    case "list_specs":
      return mockState.specs.filter((item) => item.project_id === args.project_id) as T;
    case "delete_spec": {
      const id = args.id as string;
      mockState.specs = mockState.specs.filter((item) => item.id !== id);
      mockState.requirements = mockState.requirements.filter((item) => item.spec_id !== id);
      return undefined as T;
    }
    case "reparse_spec": {
      const spec = mockState.specs.find((item) => item.id === args.id);
      if (!spec) throw new Error("Spec not found");
      mockState.requirements = mockState.requirements.filter((item) => item.spec_id !== spec.id);
      const requirements = parseRequirements(spec.id, spec.content);
      mockState.requirements.push(...requirements);
      spec.parsed_at = now();
      return requirements as T;
    }
    case "read_file_content":
      throw new Error("Use the browser file picker in web preview");
    case "generate_tests": {
      const request = args.request as GenerateTestsRequest;
      const tests = request.requirement_ids
        .map((id) => mockState.requirements.find((requirement) => requirement.id === id))
        .filter((requirement): requirement is Requirement => Boolean(requirement))
        .map((requirement) => generateMockTest(requirement, request));
      mockState.generatedTests.push(...tests);
      return tests as T;
    }
    case "get_generated_tests":
      return mockState.generatedTests.filter(
        (item) => item.requirement_id === args.requirement_id,
      ) as T;
    case "get_all_generated_tests": {
      const projectSpecIds = new Set(
        mockState.specs
          .filter((spec) => spec.project_id === args.project_id)
          .map((spec) => spec.id),
      );
      const requirementIds = new Set(
        mockState.requirements
          .filter((requirement) => projectSpecIds.has(requirement.spec_id))
          .map((requirement) => requirement.id),
      );
      return mockState.generatedTests.filter((test) =>
        requirementIds.has(test.requirement_id),
      ) as T;
    }
    case "list_repository_tests":
      return [
        {
          path: "tests/model.test.ts",
          language: "typescript",
          framework: "vitest",
          assertion_status: "meaningful",
          assertion_lines: [8],
        },
      ] as T;
    case "link_repository_test": {
      const request = args.request as LinkRepositoryTestRequest;
      const existing = mockState.generatedTests.find(
        (test) =>
          test.requirement_id === request.requirement_id &&
          test.generation_mode === "repository_link" &&
          test.file_path?.endsWith(request.path),
      );
      if (existing) return existing as T;
      const linked: GeneratedTest = {
        id: nextId("test"),
        requirement_id: request.requirement_id,
        framework: "vitest",
        generation_mode: "repository_link",
        file_path: `/preview/javascript-fixture/${request.path}`,
        code: "import { expect, it } from 'vitest';\nimport { simulate } from '../src/model';\nit('exercises linked behavior', () => { expect(simulate({ faults: [], rollback: false }).id).toBeDefined(); });\n",
        created_at: now(),
      };
      mockState.generatedTests.push(linked);
      return linked as T;
    }
    case "save_test_to_disk":
      return (args.path ?? "") as T;
    case "save_settings":
      mockState.settings = args.settings as AppSettings;
      return undefined as T;
    case "load_settings":
      return mockState.settings as T;
    case "execute_tests": {
      const testIds = args.test_ids as string[];
      const results = testIds.map<TestResult>((testId) => ({
        id: nextId("result"),
        generated_test_id: testId,
        status: "passed",
        execution_time_ms: 12,
        stdout: "Mock browser preview execution passed",
        stderr: "",
        executed_at: now(),
      }));
      mockState.testResults.push(...results);
      return results as T;
    }
    case "get_test_results":
      return mockState.testResults as T;
    case "get_test_result": {
      const result = mockState.testResults.find((item) => item.id === args.id);
      if (!result) throw new Error("Test result not found");
      return result as T;
    }
    case "generate_alignment_report": {
      const projectId = args.project_id as string;
      const specs = mockState.specs.filter((spec) => spec.project_id === projectId);
      const requirements = mockState.requirements.filter((requirement) =>
        specs.some((spec) => spec.id === requirement.spec_id),
      );
      const alignments = requirements.map((requirement) => {
        const generated = mockState.generatedTests.find(
          (test) => test.requirement_id === requirement.id,
        );
        const result = generated
          ? mockState.testResults.find((item) => item.generated_test_id === generated.id)
          : undefined;
        const failed = ["failed", "timed_out", "error"].includes(result?.status ?? "");
        const repositoryLink = generated?.generation_mode === "repository_link";
        const evidence: EvidenceRecord[] = [
          {
            id: `${requirement.id}-requirement`,
            kind: "requirement",
            path: null,
            line_start: requirement.source_line_start,
            line_end: requirement.source_line_end,
            symbol: null,
            status: "parsed",
            summary: "Requirement parsed in browser preview.",
          },
        ];
        if (generated) {
          if (repositoryLink) {
            evidence.push({
              id: `${requirement.id}-test`,
              kind: "test",
              path: generated.file_path,
              line_start: 1,
              line_end: generated.code.split(/\r?\n/).length,
              symbol: generated.id,
              status: "explicitly_linked",
              summary: "The user explicitly linked this contained repository test.",
            });
          }
          evidence.push({
            id: `${requirement.id}-assertion`,
            kind: "assertion",
            path: `generated:/${requirement.id}/${generated.id}`,
            line_start: 4,
            line_end: 4,
            symbol: null,
            status: repositoryLink ? "meaningful" : "placeholder",
            summary: repositoryLink
              ? "The repository test has a non-placeholder assertion; browser preview cannot scan implementation evidence."
              : "This tautology is non-probative and cannot verify the requirement.",
          });
        }
        if (result) {
          evidence.push({
            id: `${requirement.id}-execution`,
            kind: "execution",
            path: null,
            line_start: null,
            line_end: null,
            symbol: generated?.id ?? null,
            status: result.status,
            summary: repositoryLink
              ? "Browser preview simulates execution but cannot scan the selected local implementation."
              : "Browser preview simulates process execution; placeholder assertions remain non-probative.",
          });
        }
        return {
          requirement_id: requirement.id,
          classification: (failed ? "FAILED" : "UNKNOWN") as "FAILED" | "UNKNOWN",
          reason: failed
            ? result?.status === "timed_out"
              ? "test_timed_out"
              : "test_failed"
            : repositoryLink
              ? "evidence_unavailable"
              : generated
                ? "test_non_probative"
                : "evidence_unavailable",
          description: requirement.description,
          section: requirement.section,
          source_line_start: requirement.source_line_start,
          source_line_end: requirement.source_line_end,
          summary: failed
            ? "The associated test process failed."
            : repositoryLink
              ? "The repository test is explicitly linked, but browser preview cannot scan implementation evidence."
              : generated
                ? "A test ran, but its placeholder assertion is not evidence."
                : "Browser preview cannot scan the selected local project.",
          evidence,
        };
      });
      const verified = 0;
      const partial = 0;
      const failed = alignments.filter((item) => item.classification === "FAILED").length;
      const unknown = alignments.length - failed;
      const report: AlignmentReportWithEvidence = {
        id: nextId("report"),
        project_id: projectId,
        coverage_percent: 0,
        total_requirements: requirements.length,
        covered_requirements: verified,
        verified_requirements: verified,
        partial_requirements: partial,
        failed_requirements: failed,
        unknown_requirements: unknown,
        evidence_digest: `preview-${requirements.length}-${failed}-${unknown}`,
        checked_languages: [],
        skipped_languages: [],
        diagnostics: ["Browser preview cannot scan or execute a local target repository."],
        generated_at: now(),
        alignments,
      };
      mockState.reports.push(report);
      return report as T;
    }
    case "get_alignment_report": {
      const report = mockState.reports.find((item) => item.id === args.id);
      if (!report) throw new Error("Report not found");
      return report as T;
    }
    case "list_reports":
      return mockState.reports
        .filter((item) => item.project_id === args.project_id)
        .map(({ alignments: _alignments, ...report }) => report) as T;
    case "export_report":
      return JSON.stringify(
        mockState.reports.find((item) => item.id === args.report_id) ?? null,
        null,
        2,
      ) as T;
    default:
      throw new Error(`Unsupported browser preview command: ${command}`);
  }
}

function invoke<T>(command: string, args?: InvokeArgs): Promise<T> {
  return isTauriRuntime() ? tauriInvoke<T>(command, args) : mockInvoke<T>(command, args);
}

// Project commands
export const createProject = (req: CreateProjectRequest) =>
  invoke<Project>("create_project", { request: req });

export const listProjects = () => invoke<ProjectWithStats[]>("list_projects");

export const getProject = (id: string) => invoke<ProjectWithStats>("get_project", { id });

export const deleteProject = (id: string) => invoke<void>("delete_project", { id });

export const validatePath = (path: string) => invoke<boolean>("validate_path", { path });

// Spec commands
export const uploadSpec = (projectId: string, filename: string, content: string) =>
  invoke<ParsedSpec>("upload_spec", { project_id: projectId, filename, content });

export const getSpec = (id: string) => invoke<ParsedSpec>("get_spec", { id });

export const listSpecs = (projectId: string) =>
  invoke<Spec[]>("list_specs", { project_id: projectId });

export const deleteSpec = (id: string) => invoke<void>("delete_spec", { id });

export const reparseSpec = (id: string) => invoke<Requirement[]>("reparse_spec", { id });

export const readFileContent = (path: string) => invoke<string>("read_file_content", { path });

// Test generation commands
export const generateTests = (req: GenerateTestsRequest) =>
  invoke<GeneratedTest[]>("generate_tests", { request: req });

export const getGeneratedTests = (requirementId: string) =>
  invoke<GeneratedTest[]>("get_generated_tests", { requirement_id: requirementId });

export const getAllGeneratedTests = (projectId: string) =>
  invoke<GeneratedTest[]>("get_all_generated_tests", { project_id: projectId });

export const listRepositoryTests = (projectId: string) =>
  invoke<RepositoryTestCandidate[]>("list_repository_tests", { project_id: projectId });

export const linkRepositoryTest = (request: LinkRepositoryTestRequest) =>
  invoke<GeneratedTest>("link_repository_test", { request });

export const saveTestToDisk = (testId: string, path: string) =>
  invoke<string>("save_test_to_disk", { test_id: testId, path });

// Settings commands
export const saveSettings = (settings: AppSettings) => invoke<void>("save_settings", { settings });

export const loadSettings = () => invoke<AppSettings>("load_settings");

// Test execution commands
export const executeTests = (projectId: string, testIds: string[]) =>
  invoke<TestResult[]>("execute_tests", { project_id: projectId, test_ids: testIds });

export const getTestResults = (projectId: string) =>
  invoke<TestResult[]>("get_test_results", { project_id: projectId });

export const getTestResult = (id: string) => invoke<TestResult>("get_test_result", { id });

// Report commands
export const generateAlignmentReport = (projectId: string) =>
  invoke<AlignmentReportWithEvidence>("generate_alignment_report", { project_id: projectId });

export const getAlignmentReport = (id: string) =>
  invoke<AlignmentReportWithEvidence>("get_alignment_report", { id });

export const listReports = (projectId: string) =>
  invoke<AlignmentReport[]>("list_reports", { project_id: projectId });

export const exportReport = (reportId: string, format: "json" | "html" | "csv") =>
  invoke<string>("export_report", { report_id: reportId, format });
