pub mod limits;
pub mod spec;

pub use limits::{
    DEFAULT_SAMPLE_INTERVAL_MS, DEFAULT_SCAN_THREADS, MAX_RING_BUFFER_SAMPLES,
    MAX_SAMPLE_INTERVAL_MS, MAX_SCAN_THREADS, MAX_TOP_FILES, MAX_TOP_FOLDERS,
    MIN_SAMPLE_INTERVAL_MS, ScanConfig,
};
pub use spec::{ALL_COMMANDS, CommandSpec, Permission, Surface, find_command_spec};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn all_command_specs_have_unique_names_and_non_empty_surfaces() {
        let mut names = BTreeSet::new();
        for cmd in ALL_COMMANDS {
            assert!(!cmd.name.is_empty());
            assert!(!cmd.surfaces.is_empty());
            assert!(
                names.insert(cmd.name),
                "duplicate command spec {}",
                cmd.name
            );
        }
        assert_eq!(ALL_COMMANDS.len(), 10);
    }
}
