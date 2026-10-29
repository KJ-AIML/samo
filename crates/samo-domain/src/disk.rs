use crate::packing::{PackInput, pack_circles};
use samo_blueprint::{MAX_SCAN_THREADS, MAX_TOP_FILES, MAX_TOP_FOLDERS, ScanConfig};
use samo_primitives::{
    FileCategory, FileNodeIdentity, ProjectArtifactItem, ProjectSummary, ScanFileEntry,
    ScanFolderEntry, ScanProgressStatus, ScanSnapshot, types::CategoryBreakdownItem,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RawEntryKind {
    File,
    Directory,
    AppBundle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawDiskEntry {
    pub path: String,
    pub name: String,
    pub parent_path: Option<String>,
    pub kind: RawEntryKind,
    pub allocated_bytes: u64,
    pub identity: FileNodeIdentity,
    pub project_marker: Option<String>,
}

/// R1: Enforces hard caps on concurrent scanner threads and tracked file/folder counts.
pub fn validate_scan_config(input: ScanConfig) -> ScanConfig {
    ScanConfig {
        max_threads: input.max_threads.clamp(1, MAX_SCAN_THREADS),
        top_files_limit: input.top_files_limit.clamp(1, MAX_TOP_FILES),
        top_folders_limit: input.top_folders_limit.clamp(1, MAX_TOP_FOLDERS),
    }
}

pub fn detect_project_marker(file_name: &str) -> Option<&'static str> {
    match file_name {
        "Cargo.toml" => Some("Rust (Cargo)"),
        "package.json" => Some("Node / JS"),
        "pyproject.toml" | "requirements.txt" => Some("Python"),
        "go.mod" => Some("Go"),
        "Podfile" | "Package.swift" => Some("Swift / iOS"),
        ".git" => Some("Git Repository"),
        _ => None,
    }
}

pub fn is_artifact_dir_name(dir_name: &str) -> bool {
    matches!(
        dir_name,
        "target"
            | "node_modules"
            | ".venv"
            | "venv"
            | ".next"
            | "dist"
            | "build"
            | ".cache"
            | ".turbo"
            | ".parcel-cache"
            | "Pods"
            | "DerivedData"
            | "__pycache__"
            | ".pytest_cache"
            | ".gradle"
    )
}

pub fn classify_file_path(path: &str, is_app_bundle: bool) -> FileCategory {
    if is_app_bundle || path.ends_with(".app") || path.contains(".app/") {
        return FileCategory::Applications;
    }
    if path.starts_with("/System/")
        || path.starts_with("/Library/")
        || path.starts_with("/usr/")
        || path.starts_with("/bin/")
        || path.starts_with("/sbin/")
        || path.starts_with("/private/var/db/")
    {
        return FileCategory::System;
    }

    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    // A leading dot is a hidden name, not an extension (`.gitignore` has none).
    let ext = name
        .strip_prefix('.')
        .unwrap_or(name)
        .rsplit_once('.')
        .map(|(_, e)| e)
        .unwrap_or("");

    // Whole files that are disk images whatever their extension says.
    if name == "docker.raw" {
        return FileCategory::Archives;
    }
    // A repository's history is part of the code, and so is a build's extensionless output.
    if lower.contains("/.git/") {
        return FileCategory::Code;
    }
    if ext.is_empty()
        && (lower.contains("/target/")
            || lower.contains("/node_modules/.bin/")
            || lower.contains("/build/")
            || lower.contains("/.build/"))
    {
        return FileCategory::Code;
    }

    match ext {
        "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v" | "mpg" | "mpeg" | "mp3" | "wav"
        | "flac" | "aac" | "m4a" | "ogg" | "aiff" | "caf" | "jpg" | "jpeg" | "png" | "gif"
        | "webp" | "heic" | "avif" | "tif" | "tiff" | "bmp" | "ico" | "icns" | "svg" | "raw"
        | "psd" | "ai" | "fig" | "sketch" | "xd" | "blend" | "glb" | "gltf" | "fbx" | "usdz"
        | "ttf" | "otf" | "woff" | "woff2" => FileCategory::Media,

        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "pages" | "numbers" | "key"
        | "txt" | "md" | "rtf" | "csv" | "tsv" | "epub" | "parquet" | "jsonl" | "ndjson" => {
            FileCategory::Documents
        }

        "rs" | "ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs" | "map" | "py"
        | "ipynb" | "go" | "c" | "cc" | "cpp" | "h" | "hpp" | "m" | "mm" | "swift" | "kt"
        | "java" | "class" | "scala" | "cs" | "fs" | "vb" | "dart" | "rb" | "php" | "lua"
        | "zig" | "html" | "css" | "scss" | "vue" | "svelte" | "flow" | "sql" | "json" | "xml"
        | "toml" | "yaml" | "yml" | "lock" | "sh" | "zsh" | "gradle" | "meta" | "wasm" | "rlib"
        | "rmeta" | "d" | "o" | "obj" | "a" | "lib" | "dylib" | "so" | "dll" | "pdb" | "node"
        | "pcm" | "pch" | "pyc" | "pack" | "idx" => FileCategory::Code,

        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "br" | "lz4" | "lzma" | "7z"
        | "rar" | "jar" | "aar" | "war" | "whl" | "dmg" | "iso" | "img" | "pkg" | "qcow2"
        | "vmdk" | "vdi" | "vhd" | "vhdx" | "ova" | "gguf" | "safetensors" | "onnx" | "pt"
        | "bin" => FileCategory::Archives,

        "app" | "ipa" | "apk" | "exe" | "msi" | "deb" | "rpm" | "appimage" => {
            FileCategory::Applications
        }

        "sys" | "kext" | "plist" | "db" | "sqlite" | "sqlite3" | "sst" | "ldb" | "wal" | "shm"
        | "realm" | "log" => FileCategory::System,

        _ => FileCategory::Other,
    }
}

fn is_subpath_of(child: &str, ancestor: &str) -> bool {
    if child == ancestor {
        return true;
    }
    let prefix = if ancestor.ends_with('/') {
        ancestor.to_string()
    } else {
        format!("{ancestor}/")
    };
    child.starts_with(&prefix)
}

fn immediate_child_folder(root_path: &str, entry_path: &str) -> Option<String> {
    if entry_path == root_path {
        return None;
    }
    let prefix = if root_path == "/" {
        "/".to_string()
    } else {
        format!("{}/", root_path.trim_end_matches('/'))
    };
    let rel = entry_path.strip_prefix(&prefix)?;
    let first_seg = rel.split('/').next()?;
    if first_seg.is_empty() {
        return None;
    }
    if root_path == "/" {
        Some(format!("/{first_seg}"))
    } else {
        Some(format!("{}/{first_seg}", root_path.trim_end_matches('/')))
    }
}

#[allow(clippy::too_many_arguments)]
pub fn aggregate_scan_entries(
    scan_id: String,
    root_path: String,
    volume_name: String,
    started_at_ms: i64,
    completed_at_ms: i64,
    volume_total_bytes: u64,
    volume_used_bytes: u64,
    volume_free_bytes: u64,
    unreadable_paths: u64,
    entries: &[RawDiskEntry],
    raw_config: ScanConfig,
) -> ScanSnapshot {
    let config = validate_scan_config(raw_config);

    // 1. Identify directory metadata & candidate project roots
    let mut dir_identities: BTreeMap<String, (String, FileNodeIdentity)> = BTreeMap::new();
    let mut candidate_projects: BTreeMap<String, String> = BTreeMap::new();

    for entry in entries {
        if entry.kind == RawEntryKind::Directory || entry.kind == RawEntryKind::AppBundle {
            dir_identities.insert(entry.path.clone(), (entry.name.clone(), entry.identity));
            if let Some(marker) = &entry.project_marker {
                candidate_projects.insert(entry.path.clone(), marker.clone());
            }
        }
    }

    // Filter to outermost project roots so nested workspace crates don't double-count
    let all_project_paths: Vec<String> = candidate_projects.keys().cloned().collect();
    let mut outermost_projects: BTreeMap<String, String> = BTreeMap::new();
    for (proj_path, kind) in &candidate_projects {
        let has_outer_ancestor = all_project_paths
            .iter()
            .any(|other| other != proj_path && is_subpath_of(proj_path, other));
        if !has_outer_ancestor {
            outermost_projects.insert(proj_path.clone(), kind.clone());
        }
    }

    // 2. Sum category totals, top-level folders, and largest files
    let mut cat_bytes: BTreeMap<FileCategory, (u64, u64)> = BTreeMap::new();
    for cat in FileCategory::all() {
        cat_bytes.insert(cat, (0, 0));
    }

    let mut folder_rollup: BTreeMap<String, (u64, u64, BTreeMap<FileCategory, u64>)> =
        BTreeMap::new();
    let mut file_candidates: Vec<ScanFileEntry> = Vec::new();
    let mut scanned_bytes: u64 = 0;
    let mut scanned_files: u64 = 0;

    // Project accumulators: root_path -> (total_bytes, artifact_map<artifact_dir_path, bytes>)
    let mut project_totals: BTreeMap<String, (u64, BTreeMap<String, u64>)> = BTreeMap::new();
    for proj_path in outermost_projects.keys() {
        project_totals.insert(proj_path.clone(), (0, BTreeMap::new()));
    }

    for entry in entries {
        match entry.kind {
            RawEntryKind::Directory => continue,
            RawEntryKind::AppBundle | RawEntryKind::File => {
                let is_bundle = entry.kind == RawEntryKind::AppBundle;
                let cat = classify_file_path(&entry.path, is_bundle);
                let bytes = entry.allocated_bytes;

                scanned_bytes = scanned_bytes.saturating_add(bytes);
                scanned_files = scanned_files.saturating_add(1);

                if let Some((b, c)) = cat_bytes.get_mut(&cat) {
                    *b = b.saturating_add(bytes);
                    *c = c.saturating_add(1);
                }

                if let Some(top_folder) = immediate_child_folder(&root_path, &entry.path) {
                    let slot = folder_rollup
                        .entry(top_folder)
                        .or_insert_with(|| (0, 0, BTreeMap::new()));
                    slot.0 = slot.0.saturating_add(bytes);
                    slot.1 = slot.1.saturating_add(1);
                    let cat_count = slot.2.entry(cat).or_insert(0);
                    *cat_count = cat_count.saturating_add(bytes);
                }

                if !is_bundle {
                    file_candidates.push(ScanFileEntry {
                        name: entry.name.clone(),
                        path: entry.path.clone(),
                        bytes,
                        category: cat,
                        identity: entry.identity,
                    });
                }

                // Attribute to outermost project if inside one
                for proj_root in outermost_projects.keys() {
                    if is_subpath_of(&entry.path, proj_root) {
                        if let Some((tot, art_map)) = project_totals.get_mut(proj_root) {
                            *tot = tot.saturating_add(bytes);
                            let prefix = format!("{}/", proj_root.trim_end_matches('/'));
                            if let Some(rel) = entry.path.strip_prefix(&prefix) {
                                // Find if any path segment inside the project is an artifact dir
                                let mut accum_rel = String::new();
                                for seg in rel.split('/') {
                                    if accum_rel.is_empty() {
                                        accum_rel.push_str(seg);
                                    } else {
                                        accum_rel.push('/');
                                        accum_rel.push_str(seg);
                                    }
                                    if is_artifact_dir_name(seg) {
                                        let art_full = format!(
                                            "{}/{}",
                                            proj_root.trim_end_matches('/'),
                                            accum_rel
                                        );
                                        let art_slot = art_map.entry(art_full).or_insert(0);
                                        *art_slot = art_slot.saturating_add(bytes);
                                        break;
                                    }
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    // Sort largest files
    file_candidates.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    file_candidates.truncate(config.top_files_limit);

    // Build largest folders
    let mut largest_folders: Vec<ScanFolderEntry> = folder_rollup
        .into_iter()
        .map(|(folder_path, (bytes, file_count, by_cat))| {
            let dominant_cat = by_cat
                .into_iter()
                .max_by_key(|(_, b)| *b)
                .map(|(c, _)| c)
                .unwrap_or(FileCategory::Other);
            let (name, identity) = dir_identities
                .get(&folder_path)
                .cloned()
                .unwrap_or_else(|| {
                    let fallback_name = folder_path
                        .rsplit('/')
                        .next()
                        .unwrap_or(&folder_path)
                        .to_string();
                    (
                        fallback_name,
                        FileNodeIdentity {
                            device_id: 0,
                            inode: 0,
                        },
                    )
                });
            ScanFolderEntry {
                name,
                path: folder_path,
                bytes,
                file_count,
                category: dominant_cat,
                identity,
            }
        })
        .collect();
    largest_folders.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    largest_folders.truncate(config.top_folders_limit);

    // Build project summaries
    let mut projects: Vec<ProjectSummary> = outermost_projects
        .into_iter()
        .filter_map(|(proj_root, kind)| {
            let (total_bytes, art_map) = project_totals.remove(&proj_root)?;
            let mut artifacts: Vec<ProjectArtifactItem> = art_map
                .into_iter()
                .map(|(art_path, bytes)| {
                    let (name, identity) =
                        dir_identities.get(&art_path).cloned().unwrap_or_else(|| {
                            let n = art_path.rsplit('/').next().unwrap_or(&art_path).to_string();
                            (
                                n,
                                FileNodeIdentity {
                                    device_id: 0,
                                    inode: 0,
                                },
                            )
                        });
                    ProjectArtifactItem {
                        name,
                        path: art_path,
                        bytes,
                        identity,
                    }
                })
                .collect();
            artifacts.sort_by_key(|a| std::cmp::Reverse(a.bytes));
            let artifact_bytes: u64 = artifacts.iter().map(|a| a.bytes).sum();
            let source_bytes = total_bytes.saturating_sub(artifact_bytes);
            let (name, identity) = dir_identities.get(&proj_root).cloned().unwrap_or_else(|| {
                let n = proj_root
                    .rsplit('/')
                    .next()
                    .unwrap_or(&proj_root)
                    .to_string();
                (
                    n,
                    FileNodeIdentity {
                        device_id: 0,
                        inode: 0,
                    },
                )
            });
            Some(ProjectSummary {
                name,
                root_path: proj_root,
                kind,
                total_bytes,
                artifact_bytes,
                source_bytes,
                artifacts,
                identity,
            })
        })
        .collect();
    projects.sort_by(|a, b| {
        b.total_bytes
            .cmp(&a.total_bytes)
            .then_with(|| a.root_path.cmp(&b.root_path))
    });

    // Category breakdown
    let mut categories: Vec<CategoryBreakdownItem> = FileCategory::all()
        .into_iter()
        .map(|cat| {
            let (bytes, file_count) = cat_bytes.get(&cat).copied().unwrap_or((0, 0));
            let share_pct = if scanned_bytes > 0 {
                (bytes as f64 / scanned_bytes as f64) * 100.0
            } else {
                0.0
            };
            CategoryBreakdownItem {
                category: cat,
                label: cat.label().to_string(),
                bytes,
                file_count,
                share_pct,
            }
        })
        .collect();
    categories.sort_by_key(|a| std::cmp::Reverse(a.bytes));

    // Build packed balls from largest folders + top standalone files at root
    let mut pack_inputs: Vec<PackInput> = largest_folders
        .iter()
        .map(|f| PackInput {
            id: f.path.clone(),
            label: f.name.clone(),
            path: f.path.clone(),
            bytes: f.bytes,
            category: f.category,
            identity: f.identity,
        })
        .collect();
    if pack_inputs.len() < 24 {
        for file in file_candidates.iter().take(24 - pack_inputs.len()) {
            if !pack_inputs.iter().any(|p| p.path == file.path) {
                pack_inputs.push(PackInput {
                    id: file.path.clone(),
                    label: file.name.clone(),
                    path: file.path.clone(),
                    bytes: file.bytes,
                    category: file.category,
                    identity: file.identity,
                });
            }
        }
    }
    let balls = pack_circles(&pack_inputs);

    ScanSnapshot {
        scan_id,
        root_path,
        volume_name,
        status: ScanProgressStatus::Completed,
        started_at_ms,
        completed_at_ms: Some(completed_at_ms),
        volume_total_bytes,
        volume_used_bytes,
        volume_free_bytes,
        scanned_bytes,
        scanned_files,
        unreadable_paths,
        categories,
        largest_folders,
        largest_files: file_candidates,
        projects,
        balls,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn developer_files_are_classified_not_left_as_other() {
        let cat = |p: &str| classify_file_path(p, false);
        assert_eq!(cat("/u/p/bin/Debug/App.dll"), FileCategory::Code);
        assert_eq!(cat("/u/p/dist/app.js.map"), FileCategory::Code);
        assert_eq!(cat("/u/p/node_modules/x/index.mjs"), FileCategory::Code);
        assert_eq!(
            cat("/u/p/.git/objects/pack/pack-1.pack"),
            FileCategory::Code
        );
        assert_eq!(cat("/u/p/.git/objects/ab/cdef0123"), FileCategory::Code);
        assert_eq!(cat("/u/p/target/debug/samo-gpui"), FileCategory::Code);
        assert_eq!(cat("/u/p/data/000123.sst"), FileCategory::System);
        assert_eq!(cat("/u/p/public/app.js.br"), FileCategory::Archives);
        assert_eq!(cat("/u/p/fonts/Inter.woff2"), FileCategory::Media);
        assert_eq!(cat("/u/p/events.jsonl"), FileCategory::Documents);
    }

    #[test]
    fn docker_disk_is_an_archive_not_a_photo() {
        let path = "/Users/a/Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw";
        assert_eq!(classify_file_path(path, false), FileCategory::Archives);
        assert_eq!(
            classify_file_path("/u/photos/IMG_1.raw", false),
            FileCategory::Media
        );
    }

    #[test]
    fn hidden_names_and_unknown_files_stay_other() {
        assert_eq!(
            classify_file_path("/u/p/.gitignore", false),
            FileCategory::Other
        );
        assert_eq!(
            classify_file_path("/u/p/README", false),
            FileCategory::Other
        );
        assert_eq!(
            classify_file_path("/u/p/thing.unknownext", false),
            FileCategory::Other
        );
    }

    #[test]
    fn scanner_and_ring_buffer_caps_enforced() {
        let clamped = validate_scan_config(ScanConfig {
            max_threads: 99,
            top_files_limit: 50_000,
            top_folders_limit: 5_000,
        });
        assert_eq!(clamped.max_threads, MAX_SCAN_THREADS);
        assert_eq!(clamped.top_files_limit, MAX_TOP_FILES);
        assert_eq!(clamped.top_folders_limit, MAX_TOP_FOLDERS);
    }

    #[test]
    fn outermost_project_absorbs_workspace_and_app_bundles_are_atomic() {
        let entries = vec![
            RawDiskEntry {
                path: "/Users/kj/work/monorepo".to_string(),
                name: "monorepo".to_string(),
                parent_path: Some("/Users/kj/work".to_string()),
                kind: RawEntryKind::Directory,
                allocated_bytes: 0,
                identity: FileNodeIdentity {
                    device_id: 1,
                    inode: 10,
                },
                project_marker: Some("Rust (Cargo)".to_string()),
            },
            RawDiskEntry {
                path: "/Users/kj/work/monorepo/target".to_string(),
                name: "target".to_string(),
                parent_path: Some("/Users/kj/work/monorepo".to_string()),
                kind: RawEntryKind::Directory,
                allocated_bytes: 0,
                identity: FileNodeIdentity {
                    device_id: 1,
                    inode: 11,
                },
                project_marker: None,
            },
            RawDiskEntry {
                path: "/Users/kj/work/monorepo/target/debug/deps.rlib".to_string(),
                name: "deps.rlib".to_string(),
                parent_path: Some("/Users/kj/work/monorepo/target/debug".to_string()),
                kind: RawEntryKind::File,
                allocated_bytes: 8_000_000,
                identity: FileNodeIdentity {
                    device_id: 1,
                    inode: 12,
                },
                project_marker: None,
            },
            RawDiskEntry {
                path: "/Users/kj/work/monorepo/crates/subcrate".to_string(),
                name: "subcrate".to_string(),
                parent_path: Some("/Users/kj/work/monorepo/crates".to_string()),
                kind: RawEntryKind::Directory,
                allocated_bytes: 0,
                identity: FileNodeIdentity {
                    device_id: 1,
                    inode: 13,
                },
                project_marker: Some("Rust (Cargo)".to_string()),
            },
            RawDiskEntry {
                path: "/Users/kj/work/monorepo/crates/subcrate/src/lib.rs".to_string(),
                name: "lib.rs".to_string(),
                parent_path: Some("/Users/kj/work/monorepo/crates/subcrate/src".to_string()),
                kind: RawEntryKind::File,
                allocated_bytes: 50_000,
                identity: FileNodeIdentity {
                    device_id: 1,
                    inode: 14,
                },
                project_marker: None,
            },
            RawDiskEntry {
                path: "/Users/kj/work/Xcode.app".to_string(),
                name: "Xcode.app".to_string(),
                parent_path: Some("/Users/kj/work".to_string()),
                kind: RawEntryKind::AppBundle,
                allocated_bytes: 12_000_000,
                identity: FileNodeIdentity {
                    device_id: 1,
                    inode: 20,
                },
                project_marker: None,
            },
        ];

        let snap = aggregate_scan_entries(
            "scn_1".to_string(),
            "/Users/kj/work".to_string(),
            "Macintosh HD".to_string(),
            1000,
            2000,
            100_000_000,
            50_000_000,
            50_000_000,
            0,
            &entries,
            ScanConfig::default(),
        );

        // Only 1 outermost project (monorepo), not duplicated for subcrate
        assert_eq!(snap.projects.len(), 1);
        assert_eq!(snap.projects[0].root_path, "/Users/kj/work/monorepo");
        assert_eq!(snap.projects[0].total_bytes, 8_050_000);
        assert_eq!(snap.projects[0].artifact_bytes, 8_000_000);
        assert_eq!(snap.projects[0].source_bytes, 50_000);

        // Xcode.app is categorized as Applications and excluded from individual Largest Files
        let app_cat = snap
            .categories
            .iter()
            .find(|c| c.category == FileCategory::Applications);
        assert_eq!(app_cat.map(|c| c.bytes), Some(12_000_000));
        assert!(
            !snap
                .largest_files
                .iter()
                .any(|f| f.path.contains("Xcode.app"))
        );
    }
}
