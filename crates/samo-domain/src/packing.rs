use samo_primitives::{FileCategory, FileNodeIdentity, PackedBall};

#[derive(Debug, Clone)]
pub struct PackInput {
    pub id: String,
    pub label: String,
    pub path: String,
    pub bytes: u64,
    pub category: FileCategory,
    pub identity: FileNodeIdentity,
}

/// Deterministic spiral circle-packing layout in a 1000x1000 viewport centered at (500, 500),
/// matching TMOG's Disk Space "View: Balls" display.
pub fn pack_circles(inputs: &[PackInput]) -> Vec<PackedBall> {
    if inputs.is_empty() {
        return Vec::new();
    }

    let mut sorted: Vec<PackInput> = inputs.iter().filter(|i| i.bytes > 0).cloned().collect();
    sorted.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.id.cmp(&b.id)));
    sorted.truncate(64);

    let max_bytes = sorted.first().map(|i| i.bytes).unwrap_or(1).max(1) as f64;
    let total_bytes: f64 = sorted.iter().map(|i| i.bytes as f64).sum::<f64>().max(1.0);

    // Compute raw radii proportional to sqrt(bytes), scaled so the largest ball fits cleanly
    let max_r = if sorted.len() == 1 { 260.0 } else { 195.0 };
    let min_r = 16.0;

    let mut placed: Vec<PackedBall> = Vec::with_capacity(sorted.len());

    for (idx, item) in sorted.into_iter().enumerate() {
        let ratio = (item.bytes as f64 / max_bytes).sqrt();
        let share = (item.bytes as f64 / total_bytes).sqrt();
        let r = (min_r + (max_r - min_r) * (0.65 * ratio + 0.35 * share)).clamp(min_r, max_r);

        if idx == 0 {
            placed.push(PackedBall {
                id: item.id,
                label: item.label,
                path: item.path,
                bytes: item.bytes,
                category: item.category,
                x: 500.0,
                y: 500.0,
                r,
                identity: item.identity,
            });
            continue;
        }

        // Golden-angle spiral search from center (500, 500) to find the closest non-overlapping point
        let golden_angle = 2.399_963_229_728_653_f64;
        let mut chosen_x = 500.0;
        let mut chosen_y = 500.0;
        let padding = 4.0;

        for step in 1..6000 {
            let angle = (step as f64) * golden_angle + (idx as f64) * 0.5;
            let dist = 8.0 * (step as f64).sqrt();
            let cx = 500.0 + dist * angle.cos();
            let cy = 500.0 + dist * angle.sin();

            let overlaps = placed.iter().any(|other| {
                let dx = cx - other.x;
                let dy = cy - other.y;
                let min_dist = r + other.r + padding;
                (dx * dx + dy * dy) < (min_dist * min_dist)
            });

            if !overlaps {
                chosen_x = cx;
                chosen_y = cy;
                break;
            }
        }

        placed.push(PackedBall {
            id: item.id,
            label: item.label,
            path: item.path,
            bytes: item.bytes,
            category: item.category,
            x: chosen_x,
            y: chosen_y,
            r,
            identity: item.identity,
        });
    }

    placed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_circles_are_non_overlapping_and_deterministic() {
        let items = vec![
            PackInput {
                id: "a".to_string(),
                label: "Media".to_string(),
                path: "/Users/kj/Media".to_string(),
                bytes: 380_000_000_000,
                category: FileCategory::Media,
                identity: FileNodeIdentity {
                    device_id: 1,
                    inode: 1,
                },
            },
            PackInput {
                id: "b".to_string(),
                label: "Projects".to_string(),
                path: "/Users/kj/Projects".to_string(),
                bytes: 162_000_000_000,
                category: FileCategory::Code,
                identity: FileNodeIdentity {
                    device_id: 1,
                    inode: 2,
                },
            },
            PackInput {
                id: "c".to_string(),
                label: "Documents".to_string(),
                path: "/Users/kj/Documents".to_string(),
                bytes: 112_000_000_000,
                category: FileCategory::Documents,
                identity: FileNodeIdentity {
                    device_id: 1,
                    inode: 3,
                },
            },
        ];

        let first = pack_circles(&items);
        let second = pack_circles(&items);
        assert_eq!(first, second);
        assert_eq!(first.len(), 3);

        for i in 0..first.len() {
            for j in (i + 1)..first.len() {
                let dx = first[i].x - first[j].x;
                let dy = first[i].y - first[j].y;
                let dist = (dx * dx + dy * dy).sqrt();
                assert!(
                    dist >= first[i].r + first[j].r,
                    "circles {i} and {j} overlapped"
                );
            }
        }
    }
}
