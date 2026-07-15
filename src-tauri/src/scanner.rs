use crate::{
    models::{
        CacheNode, CleanupError, CleanupErrorCategory, FileTypeBreakdownResult, FileTypeCategory,
        LargestItem, LargestItemType, LargestItemsResult, SafetyLevel, ScanResult,
    },
    process::claude_activity,
    safety::{
        classify_path, claude_roots, is_cleanup_safe_target, sanitize_path,
        validate_existing_target, ValidationPurpose,
    },
};
use chrono::{DateTime, Local};
use std::{cmp::Reverse, collections::BTreeMap, fs, io, path::Path, time::SystemTime};

const MAX_TREE_DEPTH: usize = 8;
const MAX_CHILDREN_PER_NODE: usize = 200;
const MAX_LARGEST_ITEMS: usize = 100;

#[derive(Debug, Default, Clone, Copy)]
pub struct TreeStats {
    pub bytes: u64,
    pub files: u64,
    pub directories: u64,
}

pub fn perform_scan(mut warnings: Vec<String>) -> Result<ScanResult, String> {
    let roots = claude_roots();
    if roots.is_empty() {
        warnings.push("No Claude cache root could be resolved for this platform.".to_string());
    }
    let mut scanned_roots = Vec::new();
    for root in roots {
        match scan_path(
            &root.path,
            root.label,
            root.safety,
            root.default_cleanup,
            root.description,
            0,
        ) {
            Ok(node) => scanned_roots.push(node),
            Err(error) => warnings.push(format!(
                "Could not scan {}: {error}",
                sanitize_path(&root.path)
            )),
        }
    }
    let total_bytes = scanned_roots.iter().map(|node| node.size_bytes).sum();
    let activity = claude_activity();
    Ok(ScanResult {
        platform: std::env::consts::OS.to_string(),
        scanned_at: Local::now().to_rfc3339(),
        total_bytes,
        roots: scanned_roots,
        claude_running: activity != crate::models::ClaudeActivity::NotDetected,
        claude_activity: activity,
        warnings,
    })
}

fn scan_path(
    path: &Path,
    label: String,
    safety: SafetyLevel,
    default_cleanup: bool,
    description: String,
    depth: usize,
) -> io::Result<CacheNode> {
    if !path.exists() {
        return Ok(CacheNode {
            label,
            path: path.to_string_lossy().to_string(),
            size_bytes: 0,
            file_count: 0,
            dir_count: 0,
            exists: false,
            safety,
            default_cleanup: false,
            description,
            children: Vec::new(),
        });
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "symlink root rejected",
        ));
    }
    if metadata.is_file() {
        return Ok(CacheNode {
            label,
            path: path.to_string_lossy().to_string(),
            size_bytes: metadata.len(),
            file_count: 1,
            dir_count: 0,
            exists: true,
            safety,
            default_cleanup,
            description,
            children: Vec::new(),
        });
    }

    let mut totals = TreeStats::default();
    let mut children = Vec::new();
    for entry in fs::read_dir(path)? {
        let Ok(entry) = entry else {
            continue;
        };
        let child_path = entry.path();
        let Ok(child_meta) = fs::symlink_metadata(&child_path) else {
            continue;
        };
        if child_meta.file_type().is_symlink() {
            continue;
        }
        if child_meta.is_dir() {
            if depth < MAX_TREE_DEPTH {
                let (child_safety, child_description) = classify_path(&child_path);
                let child = scan_path(
                    &child_path,
                    display_label(&child_path),
                    child_safety,
                    is_cleanup_safe_target(&child_path),
                    child_description,
                    depth + 1,
                )?;
                totals.bytes = totals.bytes.saturating_add(child.size_bytes);
                totals.files = totals.files.saturating_add(child.file_count);
                totals.directories = totals.directories.saturating_add(child.dir_count + 1);
                children.push(child);
            } else {
                let child_stats = inspect_tree(&child_path, &mut Vec::new());
                totals.bytes = totals.bytes.saturating_add(child_stats.bytes);
                totals.files = totals.files.saturating_add(child_stats.files);
                totals.directories = totals
                    .directories
                    .saturating_add(child_stats.directories + 1);
            }
        } else if child_meta.is_file() {
            totals.bytes = totals.bytes.saturating_add(child_meta.len());
            totals.files = totals.files.saturating_add(1);
        }
    }
    children.sort_by_key(|item| Reverse(item.size_bytes));
    children.truncate(MAX_CHILDREN_PER_NODE);
    Ok(CacheNode {
        label,
        path: path.to_string_lossy().to_string(),
        size_bytes: totals.bytes,
        file_count: totals.files,
        dir_count: totals.directories,
        exists: true,
        safety,
        default_cleanup,
        description,
        children,
    })
}

pub fn inspect_tree(path: &Path, errors: &mut Vec<CleanupError>) -> TreeStats {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) => {
            errors.push(io_error(path, &error, CleanupErrorCategory::ReadFailed));
            return TreeStats::default();
        }
    };
    if metadata.file_type().is_symlink() {
        errors.push(CleanupError {
            category: CleanupErrorCategory::SymlinkRejected,
            path: sanitize_path(path),
            message: "Symbolic link was not followed.".to_string(),
        });
        return TreeStats::default();
    }
    if metadata.is_file() {
        return TreeStats {
            bytes: metadata.len(),
            files: 1,
            directories: 0,
        };
    }
    let mut stats = TreeStats::default();
    let entries = match fs::read_dir(path) {
        Ok(value) => value,
        Err(error) => {
            errors.push(io_error(path, &error, CleanupErrorCategory::ReadFailed));
            return stats;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(value) => value,
            Err(error) => {
                errors.push(io_error(path, &error, CleanupErrorCategory::ReadFailed));
                continue;
            }
        };
        let child = entry.path();
        let child_stats = inspect_tree(&child, errors);
        stats.bytes = stats.bytes.saturating_add(child_stats.bytes);
        stats.files = stats.files.saturating_add(child_stats.files);
        stats.directories = stats.directories.saturating_add(child_stats.directories);
        if fs::symlink_metadata(&child)
            .map(|value| value.is_dir() && !value.file_type().is_symlink())
            .unwrap_or(false)
        {
            stats.directories = stats.directories.saturating_add(1);
        }
    }
    stats
}

pub fn get_largest_items(root: &str, requested_limit: usize) -> Result<LargestItemsResult, String> {
    let target = validate_existing_target(Path::new(root), ValidationPurpose::Inspect)
        .map_err(|failure| failure.reason)?;
    let limit = requested_limit.clamp(1, MAX_LARGEST_ITEMS);
    let mut files = Vec::new();
    let mut directories = Vec::new();
    let mut warnings = Vec::new();
    collect_largest(
        &target.canonical,
        &mut files,
        &mut directories,
        &mut warnings,
    );
    files.sort_by_key(|item| Reverse(item.size_bytes));
    directories.sort_by_key(|item| Reverse(item.size_bytes));
    files.truncate(limit);
    directories.truncate(limit);
    Ok(LargestItemsResult {
        root: sanitize_path(&target.canonical),
        files,
        directories,
        warnings,
        generated_at: Local::now().to_rfc3339(),
    })
}

fn collect_largest(
    path: &Path,
    files: &mut Vec<LargestItem>,
    directories: &mut Vec<LargestItem>,
    warnings: &mut Vec<CleanupError>,
) -> u64 {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) => {
            warnings.push(io_error(path, &error, CleanupErrorCategory::ReadFailed));
            return 0;
        }
    };
    if metadata.file_type().is_symlink() {
        warnings.push(CleanupError {
            category: CleanupErrorCategory::SymlinkRejected,
            path: sanitize_path(path),
            message: "Symbolic link was not followed.".to_string(),
        });
        return 0;
    }
    let (safety, _) = classify_path(path);
    let item = |size_bytes, item_type| LargestItem {
        name: display_label(path),
        display_path: sanitize_path(path),
        full_path: path.to_string_lossy().to_string(),
        size_bytes,
        item_type,
        modified_at: metadata.modified().ok().map(system_time_string),
        safety,
        cleanup_eligible: is_cleanup_safe_target(path),
        inaccessible: false,
    };
    if metadata.is_file() {
        files.push(item(metadata.len(), LargestItemType::File));
        return metadata.len();
    }
    let entries = match fs::read_dir(path) {
        Ok(value) => value,
        Err(error) => {
            warnings.push(io_error(path, &error, CleanupErrorCategory::ReadFailed));
            directories.push(LargestItem {
                inaccessible: true,
                ..item(0, LargestItemType::Directory)
            });
            return 0;
        }
    };
    let mut total = 0_u64;
    for entry in entries {
        match entry {
            Ok(entry) => {
                total = total.saturating_add(collect_largest(
                    &entry.path(),
                    files,
                    directories,
                    warnings,
                ))
            }
            Err(error) => warnings.push(io_error(path, &error, CleanupErrorCategory::ReadFailed)),
        }
    }
    directories.push(item(total, LargestItemType::Directory));
    total
}

pub fn get_file_type_breakdown(root: &str) -> Result<FileTypeBreakdownResult, String> {
    let target = validate_existing_target(Path::new(root), ValidationPurpose::Inspect)
        .map_err(|failure| failure.reason)?;
    let mut categories: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut warnings = Vec::new();
    collect_breakdown(&target.canonical, &mut categories, &mut warnings);
    let total_bytes = categories.values().map(|(bytes, _)| *bytes).sum::<u64>();
    let mut result = categories
        .into_iter()
        .map(|(category, (size_bytes, file_count))| FileTypeCategory {
            category,
            size_bytes,
            file_count,
            percentage: if total_bytes == 0 {
                0.0
            } else {
                size_bytes as f64 * 100.0 / total_bytes as f64
            },
        })
        .collect::<Vec<_>>();
    result.sort_by_key(|item| Reverse(item.size_bytes));
    Ok(FileTypeBreakdownResult {
        root: sanitize_path(&target.canonical),
        total_bytes,
        categories: result,
        warnings,
        generated_at: Local::now().to_rfc3339(),
    })
}

fn collect_breakdown(
    path: &Path,
    categories: &mut BTreeMap<String, (u64, u64)>,
    warnings: &mut Vec<CleanupError>,
) {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) => {
            warnings.push(io_error(path, &error, CleanupErrorCategory::ReadFailed));
            return;
        }
    };
    if metadata.file_type().is_symlink() {
        warnings.push(CleanupError {
            category: CleanupErrorCategory::SymlinkRejected,
            path: sanitize_path(path),
            message: "Symbolic link was not followed.".to_string(),
        });
        return;
    }
    if metadata.is_file() {
        let category = classify_file_type(path);
        let value = categories.entry(category).or_insert((0, 0));
        value.0 = value.0.saturating_add(metadata.len());
        value.1 = value.1.saturating_add(1);
        return;
    }
    match fs::read_dir(path) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(entry) => collect_breakdown(&entry.path(), categories, warnings),
                    Err(error) => {
                        warnings.push(io_error(path, &error, CleanupErrorCategory::ReadFailed))
                    }
                }
            }
        }
        Err(error) => warnings.push(io_error(path, &error, CleanupErrorCategory::ReadFailed)),
    }
}

fn classify_file_type(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if name.contains("cache") && matches!(extension.as_str(), "db" | "sqlite" | "ldb") {
        "cache_database"
    } else if matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "ico"
    ) {
        "image"
    } else if matches!(extension.as_str(), "log" | "txt") && name.contains("log") {
        "log"
    } else if matches!(extension.as_str(), "tmp" | "temp") {
        "temporary_file"
    } else if matches!(extension.as_str(), "vhdx" | "zst") || name == "initrd" || name == "vmlinuz"
    {
        "vm_bundle"
    } else if matches!(extension.as_str(), "wasm" | "bin" | "blob") || name.contains("code cache") {
        "compiled_artifact"
    } else if name.contains("cache")
        || path.components().any(|part| {
            part.as_os_str()
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("cache")
        })
    {
        "browser_cache"
    } else {
        "unknown_other"
    }
    .to_string()
}

pub fn io_error(path: &Path, error: &io::Error, fallback: CleanupErrorCategory) -> CleanupError {
    let category = match error.raw_os_error() {
        Some(32 | 33) => CleanupErrorCategory::FileLocked,
        _ if error.kind() == io::ErrorKind::PermissionDenied => {
            CleanupErrorCategory::PermissionDenied
        }
        _ if error.kind() == io::ErrorKind::NotFound => CleanupErrorCategory::PathNotFound,
        _ => fallback,
    };
    CleanupError {
        category,
        path: sanitize_path(path),
        message: error.to_string(),
    }
}

fn display_label(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn system_time_string(value: SystemTime) -> String {
    DateTime::<Local>::from(value).to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakdown_counts_each_file_once() {
        let root = std::env::temp_dir().join(format!("ccw-breakdown-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("nested").join("one.png"), vec![0_u8; 5]).unwrap();
        fs::write(root.join("two.log"), vec![0_u8; 7]).unwrap();
        let mut categories = BTreeMap::new();
        collect_breakdown(&root, &mut categories, &mut Vec::new());
        assert_eq!(
            categories.values().map(|(bytes, _)| *bytes).sum::<u64>(),
            12
        );
        assert_eq!(categories.values().map(|(_, files)| *files).sum::<u64>(), 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn largest_limit_is_hard_bounded() {
        assert_eq!(200_usize.clamp(1, MAX_LARGEST_ITEMS), MAX_LARGEST_ITEMS);
    }

    #[test]
    fn large_tree_counts_metadata_without_reading_contents() {
        let root = std::env::temp_dir().join(format!("ccw-large-tree-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        for index in 0..1_000 {
            fs::write(root.join(format!("item-{index}.bin")), [index as u8]).unwrap();
        }
        let stats = inspect_tree(&root, &mut Vec::new());
        assert_eq!(stats.files, 1_000);
        assert_eq!(stats.bytes, 1_000);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn windows_sharing_violations_are_structured_as_locked() {
        let error = io::Error::from_raw_os_error(32);
        let detail = io_error(
            Path::new("locked.bin"),
            &error,
            CleanupErrorCategory::DeleteFailed,
        );
        assert_eq!(detail.category, CleanupErrorCategory::FileLocked);
    }
}
