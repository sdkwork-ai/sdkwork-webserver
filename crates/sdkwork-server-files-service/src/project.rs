//! Project-type classification for deployment directories.
//!
//! The classifier mirrors the frontend `project-detection.ts`: a directory is
//! typed by the presence of known manifest files (and, at the top level, by
//! conventional monorepo folder shapes). The backend owns the authoritative
//! list so that security decisions (which commands may run) are derived from a
//! single, server-side source of truth.

use std::path::Path;

/// Stable classification shared with the frontend `ServerProjectType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectType {
    H5App,
    PcApp,
    FlutterApp,
    RustBackend,
    NodeBackend,
    SdkworkWorkspace,
    Generic,
}

impl ProjectType {
    pub fn label(self) -> &'static str {
        match self {
            ProjectType::H5App => "H5 App",
            ProjectType::PcApp => "PC App",
            ProjectType::FlutterApp => "Flutter App",
            ProjectType::RustBackend => "Rust Backend",
            ProjectType::NodeBackend => "Node Backend",
            ProjectType::SdkworkWorkspace => "SDKWork Workspace",
            ProjectType::Generic => "Directory",
        }
    }
}

/// Classification result for a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectClassification {
    pub project_type: ProjectType,
    /// True when this directory owns a project manifest (not merely nests one).
    pub is_project_root: bool,
}

/// Manifest markers per project type. Earlier entries win on collision.
const MARKERS: &[(ProjectType, &[&str])] = &[
    (ProjectType::FlutterApp, &["pubspec.yaml"]),
    (ProjectType::RustBackend, &["Cargo.toml", "Cargo.lock"]),
    (
        ProjectType::SdkworkWorkspace,
        &["sdkwork.app.config.json", "sdkwork.workflow.json"],
    ),
    (
        ProjectType::H5App,
        &["uni.scss", "project.config.json"],
    ),
    (
        ProjectType::PcApp,
        &["webpack.config.js"],
    ),
    // Vite / tsconfig are shared between H5 and PC; decide by the presence of
    // an H5-typical manifest, otherwise treat as a generic web app.
    (ProjectType::H5App, &["vite.config.ts", "vite.config.js", "manifest.json"]),
    (ProjectType::PcApp, &["vite.config.ts", "vite.config.js", "tsconfig.json"]),
    (ProjectType::NodeBackend, &["package.json"]),
];

/// Conventional monorepo sub-folder shapes (no manifest required).
const DIRECTORY_SHAPES: &[(ProjectType, &[&str])] = &[
    (ProjectType::RustBackend, &["crates"]),
    (
        ProjectType::Generic,
        &["apps", "packages", "sdks", "database", "deployments"],
    ),
];

/// Classify a directory given the names of the entries it directly contains.
pub fn classify_entry_names(entry_names: &[String]) -> ProjectClassification {
    let has_any = |candidates: &[&str]| {
        candidates
            .iter()
            .any(|name| entry_names.iter().any(|entry| entry == name))
    };

    for (kind, markers) in MARKERS {
        if has_any(markers) {
            return ProjectClassification {
                project_type: *kind,
                is_project_root: true,
            };
        }
    }

    // Fall back to conventional SDKWork monorepo folder shapes. These indicate
    // a nested workspace but not a standalone project root.
    for (kind, folder_names) in DIRECTORY_SHAPES {
        if has_any(folder_names) {
            return ProjectClassification {
                project_type: *kind,
                is_project_root: false,
            };
        }
    }

    ProjectClassification {
        project_type: ProjectType::Generic,
        is_project_root: false,
    }
}

/// Classify a directory on disk by reading its immediate entry names.
///
/// Returns `None` when the directory cannot be listed (permission, I/O), so
/// callers can degrade gracefully instead of failing the whole browse.
pub fn classify_directory(path: &Path) -> Option<ProjectClassification> {
    let entries: Vec<String> = std::fs::read_dir(path)
        .ok()?
        .filter_map(|result| result.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    Some(classify_entry_names(&entries))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_flutter() {
        let result = classify_entry_names(&["pubspec.yaml".to_string()]);
        assert_eq!(result.project_type, ProjectType::FlutterApp);
        assert!(result.is_project_root);
    }

    #[test]
    fn detects_rust_backend() {
        let result = classify_entry_names(&["Cargo.toml".to_string(), "src".to_string()]);
        assert_eq!(result.project_type, ProjectType::RustBackend);
        assert!(result.is_project_root);
    }

    #[test]
    fn detects_sdkwork_workspace() {
        let result = classify_entry_names(&["sdkwork.app.config.json".to_string()]);
        assert_eq!(result.project_type, ProjectType::SdkworkWorkspace);
        assert!(result.is_project_root);
    }

    #[test]
    fn falls_back_to_monorepo_shape() {
        let result = classify_entry_names(&["crates".to_string(), "apps".to_string()]);
        assert_eq!(result.project_type, ProjectType::RustBackend);
        assert!(!result.is_project_root);
    }

    #[test]
    fn unknown_is_generic() {
        let result = classify_entry_names(&["README.md".to_string()]);
        assert_eq!(result.project_type, ProjectType::Generic);
        assert!(!result.is_project_root);
    }
}
