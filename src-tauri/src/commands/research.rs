use crate::models::research::{
    export_research_package, import_research_package, ResearchPackageImport,
};

#[tauri::command]
pub fn inspect_research_package(raw: String) -> Result<ResearchPackageImport, String> {
    import_research_package(&raw)
}

#[tauri::command]
pub fn export_canonical_research_package(raw: String) -> Result<String, String> {
    let imported = import_research_package(&raw)?;
    export_research_package(&imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str =
        include_str!("../../../fixtures/evidence-centered-research/qualified-package-v3.json");

    #[test]
    fn command_boundary_inspects_and_exports_v2_package() {
        let inspected = inspect_research_package(FIXTURE.into()).expect("inspect fixture");
        assert_eq!(
            inspected.schema_version,
            "evidence-centered.research-package.v2"
        );
        assert!(inspected
            .source_lifecycle
            .iter()
            .any(|item| item.source_id == "source-unknown-authority"));
        let exported = export_canonical_research_package(FIXTURE.into()).expect("export fixture");
        assert_eq!(
            crate::models::research::research_package_digest(&inspected.canonical_package).unwrap(),
            crate::models::research::research_package_digest(
                &serde_json::from_str(&exported).expect("parse exported fixture")
            )
            .unwrap()
        );
    }
}
