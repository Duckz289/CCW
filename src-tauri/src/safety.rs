use crate::models::{CleanupErrorCategory, SafetyLevel};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct KnownRoot {
    pub path: PathBuf,
    pub label: String,
    pub safety: SafetyLevel,
    pub default_cleanup: bool,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationPurpose {
    CleanupSafeOnly,
    CleanupAllowCaution,
    Inspect,
}

#[derive(Debug, Clone)]
pub struct ValidatedTarget {
    pub canonical: PathBuf,
    pub safety: SafetyLevel,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct ValidationFailure {
    pub category: CleanupErrorCategory,
    pub reason: String,
}

pub fn claude_roots() -> Vec<KnownRoot> {
    match std::env::consts::OS {
        "macos" => dirs::home_dir()
            .map(|home| {
                vec![
                    protected_root(
                        home.join("Library/Application Support/Claude/vm_bundles"),
                        "Claude workspace bundles",
                        "Workspace VM bundle container. Active workspace and session data must not be removed.",
                    ),
                    safe_root(
                        home.join("Library/Application Support/Claude/Cache"),
                        "Renderer cache",
                    ),
                    safe_root(
                        home.join("Library/Application Support/Claude/Code Cache"),
                        "Code cache",
                    ),
                    protected_root(
                        home.join("Library/Application Support/Claude/claude-code-vm"),
                        "Claude Code VM data",
                        "May contain workspace or session state.",
                    ),
                    protected_root(
                        home.join("Library/Application Support/Claude/claude-code"),
                        "Claude Code data",
                        "May contain Claude Code project or session state.",
                    ),
                    safe_root(home.join("Library/Caches/Claude"), "Claude system cache"),
                ]
            })
            .unwrap_or_default(),
        "windows" => windows_roots_from_env(),
        _ => Vec::new(),
    }
}

fn windows_roots_from_env() -> Vec<KnownRoot> {
    let mut roots = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        roots.push(protected_root(
            PathBuf::from(appdata).join("Claude"),
            "Claude roaming data",
            "Top-level Claude data may contain identity, configuration, browser, project, or session state.",
        ));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let local_path = PathBuf::from(local);
        roots.push(protected_root(
            local_path.join("Claude"),
            "Claude local data",
            "Top-level Claude data may contain application or session state.",
        ));
        roots.push(protected_root(
            local_path.join("Claude-3p"),
            "Claude (3p channel) data",
            "Top-level Claude channel data may contain application or workspace state.",
        ));
        roots.push(safe_root(
            local_path.join("Temp").join("claude"),
            "Claude temp files",
        ));
        roots.extend(windows_store_package_roots(&local_path));
    }
    roots
}

pub fn windows_store_package_roots(local_path: &Path) -> Vec<KnownRoot> {
    let packages_path = local_path.join("Packages");
    let Ok(entries) = fs::read_dir(packages_path) else {
        return Vec::new();
    };
    let mut packages = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("claude_")
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    packages.sort();
    packages
        .into_iter()
        .flat_map(|package| windows_store_package_branch_roots(&package))
        .collect()
}

pub fn windows_store_package_branch_roots(package_root: &Path) -> Vec<KnownRoot> {
    vec![
        protected_root(
            package_root.join("LocalCache"),
            "Claude Store package cache",
            "Contains cache plus Claude workspace, session, identity, browser, and app-state data.",
        ),
        caution_root(
            package_root.join("TempState"),
            "Claude Store package temp state",
        ),
        protected_root(
            package_root.join("LocalState"),
            "Claude Store package local state",
            "May contain Store app state or sessions.",
        ),
        protected_root(
            package_root.join("RoamingState"),
            "Claude Store package roaming state",
            "May contain roaming identity, configuration, or session state.",
        ),
        protected_root(
            package_root.join("Settings"),
            "Claude Store package settings",
            "Contains Store application settings.",
        ),
        protected_root(
            package_root.join("AC"),
            "Claude Store package app container",
            "May contain browser storage and app-container state.",
        ),
        protected_root(
            package_root.join("SystemAppData"),
            "Claude Store package system app data",
            "Contains protected Windows application state.",
        ),
    ]
}

fn safe_root(path: PathBuf, label: &str) -> KnownRoot {
    KnownRoot {
        path,
        label: label.to_string(),
        safety: SafetyLevel::Safe,
        default_cleanup: true,
        description: "Cache data that Claude can rebuild after cleanup.".to_string(),
    }
}

fn protected_root(path: PathBuf, label: &str, description: &str) -> KnownRoot {
    KnownRoot {
        path,
        label: label.to_string(),
        safety: SafetyLevel::NotRecommended,
        default_cleanup: false,
        description: description.to_string(),
    }
}

fn caution_root(path: PathBuf, label: &str) -> KnownRoot {
    KnownRoot {
        path,
        label: label.to_string(),
        safety: SafetyLevel::Caution,
        default_cleanup: false,
        description: "Temporary Claude Store state that is excluded from automatic cleanup and requires quarantine review.".to_string(),
    }
}

pub fn classify_path(path: &Path) -> (SafetyLevel, String) {
    let normalized = normalized_path(path);
    if is_rebuildable_vm_bundle_leaf(path) || is_rebuildable_vm_bundle_artifact(path) {
        return (
            SafetyLevel::Safe,
            "Rebuildable Claude VM runtime artifact. Claude can recreate it after cleanup."
                .to_string(),
        );
    }
    if is_path_leaf(path, "LocalCache") || has_sensitive_state_segment(path) {
        return (
            SafetyLevel::NotRecommended,
            "Protected Claude workspace, session, identity, configuration, browser, project, or application-state data.".to_string(),
        );
    }
    if is_path_leaf(path, "TempState") {
        return (
            SafetyLevel::Caution,
            "Temporary Claude Store state. It is not selected automatically and requires quarantine review.".to_string(),
        );
    }
    if normalized.ends_with("\\temp\\claude")
        || normalized.ends_with("/temp/claude")
        || is_rebuildable_cache_leaf(path)
        || normalized.ends_with("\\library\\caches\\claude")
        || normalized.ends_with("/library/caches/claude")
    {
        return (
            SafetyLevel::Safe,
            "Cache data that Claude can rebuild after cleanup.".to_string(),
        );
    }
    if is_path_leaf(path, "Claude")
        || normalized.ends_with("/claude")
        || normalized.ends_with("\\claude")
    {
        return (
            SafetyLevel::NotRecommended,
            "Top-level Claude data may contain settings, identity, projects, sessions, or application state.".to_string(),
        );
    }
    (
        SafetyLevel::Caution,
        "Unrecognized Claude subtree. It is excluded from normal cleanup and may only be moved to quarantine after review.".to_string(),
    )
}

pub fn validate_existing_target(
    path: &Path,
    purpose: ValidationPurpose,
) -> Result<ValidatedTarget, ValidationFailure> {
    validate_existing_target_with_roots(path, &claude_roots(), purpose)
}

pub fn validate_existing_target_with_roots(
    path: &Path,
    roots: &[KnownRoot],
    purpose: ValidationPurpose,
) -> Result<ValidatedTarget, ValidationFailure> {
    let metadata = fs::symlink_metadata(path).map_err(|error| ValidationFailure {
        category: if error.kind() == io::ErrorKind::NotFound {
            CleanupErrorCategory::PathNotFound
        } else {
            CleanupErrorCategory::ReadFailed
        },
        reason: format!("Cannot inspect target: {error}"),
    })?;
    if metadata.file_type().is_symlink() || metadata_is_reparse_point(&metadata) {
        return Err(ValidationFailure {
            category: CleanupErrorCategory::SymlinkRejected,
            reason: "Symbolic links, junctions, and reparse-point targets are never accepted."
                .to_string(),
        });
    }
    let inspection_file = purpose == ValidationPurpose::Inspect && metadata.is_file();
    if !(metadata.is_dir()
        || inspection_file
        || metadata.is_file() && is_rebuildable_vm_bundle_artifact(path))
    {
        return Err(ValidationFailure {
            category: CleanupErrorCategory::InvalidTarget,
            reason: "Only approved directories or known rebuildable VM artifacts can be managed."
                .to_string(),
        });
    }

    let canonical = path.canonicalize().map_err(|error| ValidationFailure {
        category: CleanupErrorCategory::InvalidTarget,
        reason: format!("Target could not be canonicalized: {error}"),
    })?;

    let mut matching_roots = roots
        .iter()
        .filter_map(|root| root.path.canonicalize().ok().map(|known| (root, known)))
        .filter(|(_, known)| canonical == *known || canonical.starts_with(known))
        .collect::<Vec<_>>();
    matching_roots.sort_by_key(|(_, root)| std::cmp::Reverse(root.components().count()));
    let Some((known_root, known_canonical)) = matching_roots.first() else {
        return Err(ValidationFailure {
            category: CleanupErrorCategory::InvalidTarget,
            reason: "Target is outside the known Claude roots.".to_string(),
        });
    };

    if contains_unsafe_link_component(path, &known_root.path) {
        return Err(ValidationFailure {
            category: CleanupErrorCategory::SymlinkRejected,
            reason: "A path component is a symbolic link, junction, or reparse point.".to_string(),
        });
    }

    let (mut safety, mut reason) = classify_path(&canonical);
    if canonical == *known_canonical {
        safety = known_root.safety;
        reason = known_root.description.clone();
    }

    if purpose == ValidationPurpose::Inspect {
        return Ok(ValidatedTarget {
            canonical,
            safety,
            reason,
        });
    }
    if safety == SafetyLevel::NotRecommended {
        return Err(ValidationFailure {
            category: CleanupErrorCategory::ProtectedTarget,
            reason,
        });
    }
    if safety == SafetyLevel::Caution {
        if purpose != ValidationPurpose::CleanupAllowCaution {
            return Err(ValidationFailure {
                category: CleanupErrorCategory::ProtectedTarget,
                reason: "Caution targets are excluded from direct cleanup and require quarantine."
                    .to_string(),
            });
        }
        if contains_protected_descendant(&canonical) {
            return Err(ValidationFailure {
                category: CleanupErrorCategory::ProtectedTarget,
                reason: "This Caution container includes protected Claude state and cannot be quarantined as a whole.".to_string(),
            });
        }
    }
    if safety == SafetyLevel::Safe
        && metadata.is_dir()
        && !is_rebuildable_vm_bundle_leaf(&canonical)
        && contains_protected_descendant(&canonical)
    {
        return Err(ValidationFailure {
            category: CleanupErrorCategory::ProtectedTarget,
            reason: "This Safe cache container currently includes a protected Claude state item, so whole-container cleanup is refused.".to_string(),
        });
    }

    Ok(ValidatedTarget {
        canonical,
        safety,
        reason,
    })
}

pub fn validate_restore_destination_with_roots(
    path: &Path,
    roots: &[KnownRoot],
) -> Result<(), ValidationFailure> {
    if path.exists() {
        return Err(ValidationFailure {
            category: CleanupErrorCategory::RestoreConflict,
            reason:
                "The original destination already exists; restore will not overwrite or merge it."
                    .to_string(),
        });
    }
    let parent = path.parent().ok_or_else(|| ValidationFailure {
        category: CleanupErrorCategory::InvalidTarget,
        reason: "Restore destination has no parent.".to_string(),
    })?;
    let canonical_parent = parent.canonicalize().map_err(|error| ValidationFailure {
        category: CleanupErrorCategory::InvalidTarget,
        reason: format!("Restore parent could not be canonicalized: {error}"),
    })?;
    let leaf = path.file_name().ok_or_else(|| ValidationFailure {
        category: CleanupErrorCategory::InvalidTarget,
        reason: "Restore destination has no leaf name.".to_string(),
    })?;
    let reconstructed = canonical_parent.join(leaf);
    let belongs = roots.iter().any(|root| {
        root.path
            .canonicalize()
            .map(|known| reconstructed.starts_with(known))
            .unwrap_or(false)
    });
    if !belongs {
        return Err(ValidationFailure {
            category: CleanupErrorCategory::InvalidTarget,
            reason: "Restore destination is outside known Claude roots.".to_string(),
        });
    }
    if classify_path(&reconstructed).0 != SafetyLevel::Caution {
        return Err(ValidationFailure {
            category: CleanupErrorCategory::ProtectedTarget,
            reason: "Only a previously approved Caution target can be restored.".to_string(),
        });
    }
    Ok(())
}

pub fn is_cleanup_safe_target(path: &Path) -> bool {
    validate_existing_target(path, ValidationPurpose::CleanupSafeOnly)
        .map(|target| target.safety == SafetyLevel::Safe)
        .unwrap_or(false)
}

pub fn is_rebuildable_vm_bundle_leaf(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    is_path_leaf(parent, "vm_bundles")
        && path
            .file_name()
            .map(|value| {
                matches!(
                    value.to_string_lossy().to_ascii_lowercase().as_str(),
                    "claudevm.bundle" | "warm"
                )
            })
            .unwrap_or(false)
}

pub fn is_claude_vm_bundle_leaf(path: &Path) -> bool {
    path.parent()
        .map(|parent| is_path_leaf(parent, "vm_bundles"))
        .unwrap_or(false)
        && is_path_leaf(path, "claudevm.bundle")
}

pub fn is_rebuildable_vm_bundle_artifact(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    if !is_rebuildable_vm_bundle_leaf(parent) {
        return false;
    }
    path.file_name()
        .map(|value| {
            matches!(
                value.to_string_lossy().to_ascii_lowercase().as_str(),
                "rootfs.vhdx"
                    | "rootfs.vhdx.zst"
                    | "initrd"
                    | "initrd.zst"
                    | "vmlinuz"
                    | "vmlinuz.zst"
                    | "smol-bin.vhdx"
            )
        })
        .unwrap_or(false)
}

pub fn is_path_leaf(path: &Path, expected: &str) -> bool {
    path.file_name()
        .map(|value| value.to_string_lossy().eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

fn is_rebuildable_cache_leaf(path: &Path) -> bool {
    [
        "Cache",
        "Code Cache",
        "GPUCache",
        "DawnGraphiteCache",
        "DawnWebGPUCache",
    ]
    .iter()
    .any(|leaf| is_path_leaf(path, leaf))
}

pub fn has_sensitive_state_segment(path: &Path) -> bool {
    path.components().any(|component| {
        let segment = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        matches!(
            segment.as_str(),
            "indexeddb"
                | "local storage"
                | "session storage"
                | "file system"
                | "service worker"
                | "network"
                | "partitions"
                | "cookies"
                | "local-agent-mode-sessions"
                | "vm_bundles"
                | "claude-code"
                | "claude-code-vm"
                | "settings"
                | "localstate"
                | "roamingstate"
                | "systemappdata"
                | "ac"
                | "config"
                | "preferences"
                | "local state"
                | "config.json"
                | "claude_desktop_config.json"
                | "git-worktrees.json"
                | "buddy-tokens.json"
                | "cowork-enabled-cli-ops.json"
                | "ant-did"
                | "dips"
                | "dips-wal"
                | "sharedstorage"
                | "sharedstorage-wal"
                | "shared dictionary"
                | "webstorage"
                | "projects"
                | "project"
                | "sessions"
                | "session"
        )
    })
}

fn contains_protected_descendant(path: &Path) -> bool {
    let Ok(entries) = fs::read_dir(path) else {
        return true;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            return true;
        };
        let child = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&child) else {
            return true;
        };
        if metadata.file_type().is_symlink() || metadata_is_reparse_point(&metadata) {
            return true;
        }
        if classify_path(&child).0 == SafetyLevel::NotRecommended {
            return true;
        }
        if metadata.is_dir() && contains_protected_descendant(&child) {
            return true;
        }
    }
    false
}

fn contains_unsafe_link_component(path: &Path, root: &Path) -> bool {
    let mut cursor = Some(path);
    while let Some(current) = cursor {
        if let Ok(metadata) = fs::symlink_metadata(current) {
            if metadata.file_type().is_symlink() || metadata_is_reparse_point(&metadata) {
                return true;
            }
        } else {
            return true;
        }
        if current == root {
            break;
        }
        cursor = current.parent();
    }
    false
}

#[cfg(target_os = "windows")]
fn metadata_is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(target_os = "windows"))]
fn metadata_is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().to_ascii_lowercase()
}

pub fn sanitize_path(path: &Path) -> String {
    let raw = path.to_string_lossy().to_string();
    if cfg!(target_os = "windows") {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            if raw
                .to_ascii_lowercase()
                .starts_with(&profile.to_ascii_lowercase())
            {
                return format!("%USERPROFILE%{}", &raw[profile.len()..]);
            }
        }
    } else if let Some(home) = dirs::home_dir() {
        let home = home.to_string_lossy();
        if raw.starts_with(home.as_ref()) {
            return format!("~{}", &raw[home.len()..]);
        }
    }
    raw
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(path: PathBuf, safety: SafetyLevel) -> KnownRoot {
        KnownRoot {
            path,
            label: "test".to_string(),
            safety,
            default_cleanup: safety == SafetyLevel::Safe,
            description: "test root".to_string(),
        }
    }

    #[test]
    fn protected_windows_branches_have_no_override() {
        let package = PathBuf::from(r"C:\Users\Test\AppData\Local\Packages\Claude_test");
        let roots = windows_store_package_branch_roots(&package);
        for leaf in [
            "LocalCache",
            "LocalState",
            "RoamingState",
            "Settings",
            "AC",
            "SystemAppData",
        ] {
            let root = roots
                .iter()
                .find(|root| is_path_leaf(&root.path, leaf))
                .unwrap();
            assert_eq!(root.safety, SafetyLevel::NotRecommended, "{leaf}");
            assert!(!root.default_cleanup, "{leaf}");
        }
    }

    #[test]
    fn safe_cache_child_validates_but_manipulated_path_does_not() {
        let base = std::env::temp_dir().join(format!("ccw-safety-{}", std::process::id()));
        let cache = base.join("Cache");
        let outside = std::env::temp_dir().join(format!("ccw-outside-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&cache).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let roots = vec![test_root(base.clone(), SafetyLevel::NotRecommended)];

        let approved =
            validate_existing_target_with_roots(&cache, &roots, ValidationPurpose::CleanupSafeOnly)
                .unwrap();
        assert_eq!(approved.safety, SafetyLevel::Safe);
        assert!(validate_existing_target_with_roots(
            &outside,
            &roots,
            ValidationPurpose::CleanupSafeOnly
        )
        .is_err());

        let _ = fs::remove_dir_all(base);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn rebuildable_vm_artifact_is_safe() {
        let root = PathBuf::from("/tmp/Claude/vm_bundles/claudevm.bundle/rootfs.vhdx");
        assert!(is_rebuildable_vm_bundle_artifact(&root));
        assert_eq!(classify_path(&root).0, SafetyLevel::Safe);
    }

    #[test]
    fn protected_targets_remain_blocked_even_when_caution_is_allowed() {
        let base = std::env::temp_dir().join(format!("ccw-protected-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        for leaf in [
            "LocalCache",
            "LocalState",
            "RoamingState",
            "Settings",
            "SystemAppData",
        ] {
            let target = base.join(leaf);
            fs::create_dir_all(&target).unwrap();
            let roots = vec![test_root(target.clone(), SafetyLevel::NotRecommended)];
            let failure = validate_existing_target_with_roots(
                &target,
                &roots,
                ValidationPurpose::CleanupAllowCaution,
            )
            .unwrap_err();
            assert_eq!(
                failure.category,
                CleanupErrorCategory::ProtectedTarget,
                "{leaf}"
            );
        }
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn safe_vm_artifact_can_be_validated_as_a_file() {
        let base = std::env::temp_dir().join(format!("ccw-vm-artifact-{}", std::process::id()));
        let bundle = base.join("vm_bundles").join("claudevm.bundle");
        let artifact = bundle.join("rootfs.vhdx");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&bundle).unwrap();
        fs::write(&artifact, b"runtime").unwrap();
        let roots = vec![test_root(base.clone(), SafetyLevel::NotRecommended)];
        let approved = validate_existing_target_with_roots(
            &artifact,
            &roots,
            ValidationPurpose::CleanupSafeOnly,
        )
        .unwrap();
        assert_eq!(approved.safety, SafetyLevel::Safe);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn inspection_allows_regular_files_but_cleanup_does_not() {
        let base = std::env::temp_dir().join(format!("ccw-inspect-file-{}", std::process::id()));
        let file = base.join("Cache").join("data.bin");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"cache").unwrap();
        let roots = vec![test_root(base.clone(), SafetyLevel::NotRecommended)];
        assert!(
            validate_existing_target_with_roots(&file, &roots, ValidationPurpose::Inspect).is_ok()
        );
        assert!(validate_existing_target_with_roots(
            &file,
            &roots,
            ValidationPurpose::CleanupSafeOnly
        )
        .is_err());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn safe_cache_container_is_refused_if_protected_state_appears_inside() {
        let base = std::env::temp_dir().join(format!("ccw-mixed-cache-{}", std::process::id()));
        let cache = base.join("Cache");
        let protected = cache.join("Session Storage");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&protected).unwrap();
        fs::write(protected.join("state.bin"), b"state").unwrap();
        let roots = vec![test_root(base.clone(), SafetyLevel::NotRecommended)];
        let failure =
            validate_existing_target_with_roots(&cache, &roots, ValidationPurpose::CleanupSafeOnly)
                .unwrap_err();
        assert_eq!(failure.category, CleanupErrorCategory::ProtectedTarget);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn symlink_target_is_rejected_when_platform_can_create_it() {
        let base = std::env::temp_dir().join(format!("ccw-link-{}", std::process::id()));
        let cache = base.join("Cache");
        let outside = base.join("outside");
        let link = cache.join("link");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&cache).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let linked = create_test_dir_symlink(&outside, &link);
        if linked {
            let roots = vec![test_root(base.clone(), SafetyLevel::NotRecommended)];
            let failure = validate_existing_target_with_roots(
                &link,
                &roots,
                ValidationPurpose::CleanupSafeOnly,
            )
            .unwrap_err();
            assert_eq!(failure.category, CleanupErrorCategory::SymlinkRejected);
        }
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn diagnostic_paths_are_sanitized_by_default() {
        if cfg!(target_os = "windows") {
            let profile = std::env::var("USERPROFILE").unwrap();
            let value = sanitize_path(
                &PathBuf::from(profile)
                    .join("AppData")
                    .join("Local")
                    .join("Claude"),
            );
            assert!(value.starts_with("%USERPROFILE%"));
        } else if let Some(home) = dirs::home_dir() {
            let value = sanitize_path(&home.join("Library").join("Caches").join("Claude"));
            assert!(value.starts_with('~'));
        }
    }

    #[cfg(target_os = "windows")]
    fn create_test_dir_symlink(target: &Path, link: &Path) -> bool {
        std::os::windows::fs::symlink_dir(target, link).is_ok()
    }

    #[cfg(not(target_os = "windows"))]
    fn create_test_dir_symlink(target: &Path, link: &Path) -> bool {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
}
