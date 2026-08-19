use license_trace::model::SourceDisclosureLevel;
use license_trace::policy::{CompatibilityReport, CompatibilityStatus};
use license_trace::resolver;
use std::path::PathBuf;

#[test]
fn test_sample_project_fixture_resolution_and_gpl_detection() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_dir = manifest_dir
        .join("tests")
        .join("fixtures")
        .join("sample_project");
    assert!(fixture_dir.exists(), "Fixture dir must exist");

    let graph =
        resolver::resolve_auto(&fixture_dir).expect("Should resolve sample project fixture");

    // Check package count (root + left-pad + gpl-sample-lib + nested-unknown-tool + jest)
    let packages = graph.all_packages();
    assert!(
        packages.len() >= 4,
        "Should have resolved at least 4 packages"
    );

    // Evaluate compatibility against MIT (prod only)
    let report_prod = CompatibilityReport::evaluate("MIT", &graph, true);

    // gpl-sample-lib is a direct production dependency in sample_project -> must be Incompatible
    assert_eq!(report_prod.status, CompatibilityStatus::Incompatible);
    assert_eq!(
        report_prod.obligations.worst_source_disclosure,
        SourceDisclosureLevel::ProjectLevel
    );
    assert!(!report_prod.obligations.problematic_packages.is_empty());
    assert!(report_prod.obligations.unknown_license_count >= 1);
}

#[test]
fn test_sample_project_why_dependency_path() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_dir = manifest_dir
        .join("tests")
        .join("fixtures")
        .join("sample_project");

    let graph =
        resolver::resolve_auto(&fixture_dir).expect("Should resolve sample project fixture");

    let paths = graph.find_all_paths_to("nested-unknown-tool");
    assert!(!paths.is_empty(), "Should find path to nested-unknown-tool");
    assert_eq!(
        paths[0].len(),
        3,
        "Path should be: root -> gpl-sample-lib -> nested-unknown-tool"
    );
}
