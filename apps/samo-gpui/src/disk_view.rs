//! What the pages derive from a scan, worked out once per scan (on the scan thread) instead of every frame.

use samo_primitives::ScanSnapshot;
use std::collections::HashSet;
use std::sync::Arc;

pub struct DiskView {
    pub snap: Arc<ScanSnapshot>,
    /// Project indices, most reclaimable first, then largest.
    pub projects_ranked: Vec<usize>,
    pub reclaimable: u64,
    pub projects_with_artifacts: usize,
    pub widest_project: u64,
    /// Category indices with bytes, largest first.
    pub categories_ranked: Vec<usize>,
    artifact_paths: HashSet<String>,
    folder_paths: HashSet<String>,
}

impl DiskView {
    pub fn build(snap: Arc<ScanSnapshot>) -> Self {
        let mut projects_ranked: Vec<usize> = (0..snap.projects.len()).collect();
        projects_ranked.sort_by(|&a, &b| {
            let (pa, pb) = (&snap.projects[a], &snap.projects[b]);
            pb.artifact_bytes
                .cmp(&pa.artifact_bytes)
                .then(pb.total_bytes.cmp(&pa.total_bytes))
        });
        let mut categories_ranked: Vec<usize> = (0..snap.categories.len())
            .filter(|&i| snap.categories[i].bytes > 0)
            .collect();
        categories_ranked.sort_by_key(|&i| std::cmp::Reverse(snap.categories[i].bytes));

        Self {
            reclaimable: snap.projects.iter().map(|p| p.artifact_bytes).sum(),
            projects_with_artifacts: snap
                .projects
                .iter()
                .filter(|p| p.artifact_bytes > 0)
                .count(),
            widest_project: snap
                .projects
                .iter()
                .map(|p| p.total_bytes)
                .max()
                .unwrap_or(0),
            artifact_paths: snap
                .projects
                .iter()
                .flat_map(|p| p.artifacts.iter().map(|a| a.path.clone()))
                .collect(),
            folder_paths: snap
                .largest_folders
                .iter()
                .map(|f| f.path.clone())
                .collect(),
            projects_ranked,
            categories_ranked,
            snap,
        }
    }

    /// Is this one of the build artifacts Samo suggests clearing?
    pub fn is_suggestion(&self, path: &str) -> bool {
        self.artifact_paths.contains(path)
    }

    pub fn is_folder(&self, path: &str) -> bool {
        self.folder_paths.contains(path)
    }
}
