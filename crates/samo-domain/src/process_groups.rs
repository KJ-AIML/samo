//! Rolling processes up into the app they belong to, so the Processes page can say "Grok Bot, 11 processes, 1.4 GB"
//! instead of eleven rows that all start with "Grok Bot Helper". Pure: paths and numbers only, no I/O, so it is
//! tested on both OSes. Which apps are *applications* (the ones in the Dock) is a fact only the OS knows, so the
//! caller passes it in.

use samo_primitives::{EnergyImpact, ProcessRow};
use std::collections::{HashMap, HashSet};

/// The `.app` an executable belongs to: the **outermost** `.app` folder in its path, so a helper deep inside an app
/// (`Grok Bot.app/Contents/Frameworks/Grok Bot Helper.app/…`) counts for the app. A daemon, an XPC service or a
/// command-line tool is in no app.
pub fn app_bundle_of(executable_path: &str) -> Option<&str> {
    let at = executable_path.find(".app/")?;
    let end = at + ".app".len();
    let bundle = &executable_path[..end];
    // A bundle has a name: "/.app/x" is not one.
    let name_from = bundle.rfind('/').map_or(0, |slash| slash + 1);
    (end - name_from > ".app".len()).then_some(bundle)
}

/// An app's name: its folder without the `.app` (`/Applications/Grok Bot.app` is `Grok Bot`).
pub fn app_name_of(bundle: &str) -> &str {
    let name = bundle.rsplit('/').next().unwrap_or(bundle);
    name.strip_suffix(".app").unwrap_or(name)
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GroupKey {
    /// All the processes of one app, by the app's folder.
    App(String),
    /// A process that belongs to no app, alone.
    Process(u32),
}

/// One row of the Processes page: an app with all its processes, or one process on its own.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessGroup {
    pub key: GroupKey,
    /// The app's name, or the process's own.
    pub name: String,
    /// The app's folder, for its icon.
    pub bundle: Option<String>,
    /// One of its processes is an application of the OS's (in the Dock), as opposed to a background process.
    pub application: bool,
    /// Indices into the rows the group was built from, in the order those rows had (the busiest first, if they
    /// were sorted that way).
    pub members: Vec<usize>,
    pub cpu_pct: f32,
    pub memory_bytes: u64,
    pub disk_bytes_per_sec: u64,
    /// The highest of its processes'.
    pub energy: EnergyImpact,
}

fn energy_rank(e: EnergyImpact) -> u8 {
    match e {
        EnergyImpact::Low => 0,
        EnergyImpact::Moderate => 1,
        EnergyImpact::High => 2,
        EnergyImpact::VeryHigh => 3,
    }
}

/// Groups `rows` by app. `application_pids` is the OS's list of Dock applications (empty if it isn't known, which
/// leaves every group a background one). Every row lands in exactly one group, and every total covers all of the
/// group's members, which is why `rows` must be every process and not only the busiest.
pub fn group_processes(rows: &[ProcessRow], application_pids: &HashSet<u32>) -> Vec<ProcessGroup> {
    let mut groups: Vec<ProcessGroup> = Vec::new();
    let mut at: HashMap<GroupKey, usize> = HashMap::new();
    for (i, row) in rows.iter().enumerate() {
        let bundle = app_bundle_of(&row.executable_path);
        let key = bundle.map_or(GroupKey::Process(row.identity.pid), |b| {
            GroupKey::App(b.to_string())
        });
        let g = *at.entry(key.clone()).or_insert_with(|| {
            groups.push(ProcessGroup {
                key,
                name: bundle.map_or_else(|| row.name.clone(), |b| app_name_of(b).to_string()),
                bundle: bundle.map(str::to_string),
                application: false,
                members: Vec::new(),
                cpu_pct: 0.0,
                memory_bytes: 0,
                disk_bytes_per_sec: 0,
                energy: EnergyImpact::Low,
            });
            groups.len() - 1
        });
        let group = &mut groups[g];
        group.members.push(i);
        group.application |= application_pids.contains(&row.identity.pid);
        group.cpu_pct += row.cpu_pct;
        group.memory_bytes += row.memory_bytes;
        group.disk_bytes_per_sec += row.disk_read_bytes_per_sec + row.disk_write_bytes_per_sec;
        if energy_rank(row.energy_impact) > energy_rank(group.energy) {
            group.energy = row.energy_impact;
        }
    }
    groups
}

/// Orders groups by the column the page is sorted on, the biggest first (names and PIDs: first to last). A group's
/// PID is its lowest member's. Ties fall back to the name, so the order never flickers.
pub fn sort_groups(groups: &mut [ProcessGroup], rows: &[ProcessRow], sort_by: &str) {
    let lowest_pid = |g: &ProcessGroup| {
        g.members
            .iter()
            .filter_map(|&i| rows.get(i))
            .map(|r| r.identity.pid)
            .min()
            .unwrap_or(u32::MAX)
    };
    let by_name = |a: &ProcessGroup, b: &ProcessGroup| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    };
    match sort_by {
        "memory" => groups.sort_by(|a, b| {
            b.memory_bytes
                .cmp(&a.memory_bytes)
                .then_with(|| by_name(a, b))
        }),
        "disk" => groups.sort_by(|a, b| {
            b.disk_bytes_per_sec
                .cmp(&a.disk_bytes_per_sec)
                .then_with(|| by_name(a, b))
        }),
        "pid" => groups.sort_by_key(lowest_pid),
        "name" => groups.sort_by(by_name),
        _ => groups.sort_by(|a, b| b.cpu_pct.total_cmp(&a.cpu_pct).then_with(|| by_name(a, b))),
    }
}

/// Whether a process matches what was typed (already lower-cased): its name, its path or its PID.
pub fn row_matches(row: &ProcessRow, query_lower: &str) -> bool {
    row.name.to_ascii_lowercase().contains(query_lower)
        || row
            .executable_path
            .to_ascii_lowercase()
            .contains(query_lower)
        || row.identity.pid.to_string().contains(query_lower)
}

/// The members of `group` to show for what was typed (`query` as typed). Nothing typed: all of them. The app's own
/// name matches: all of them. Otherwise only the processes that match, and none at all if none does, which hides
/// the group.
pub fn visible_members(group: &ProcessGroup, rows: &[ProcessRow], query: &str) -> Vec<usize> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() || group.name.to_ascii_lowercase().contains(&q) {
        return group.members.clone();
    }
    group
        .members
        .iter()
        .copied()
        .filter(|&i| rows.get(i).is_some_and(|r| row_matches(r, &q)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use samo_primitives::ProcessIdentity;

    fn row(pid: u32, name: &str, path: &str, cpu: f32, mem: u64) -> ProcessRow {
        ProcessRow {
            identity: ProcessIdentity {
                pid,
                start_time_secs: 1_700_000_000,
            },
            parent_pid: Some(1),
            name: name.to_string(),
            executable_path: path.to_string(),
            user: "501".to_string(),
            cpu_pct: cpu,
            memory_bytes: mem,
            disk_read_bytes_per_sec: 10,
            disk_write_bytes_per_sec: 5,
            energy_impact: EnergyImpact::Low,
            is_self: false,
        }
    }

    const GROK: &str = "/Applications/Grok Bot.app/Contents/MacOS/Grok Bot";
    const GROK_HELPER: &str = "/Applications/Grok Bot.app/Contents/Frameworks/Grok Bot Helper (Renderer).app/Contents/MacOS/Grok Bot Helper (Renderer)";

    #[test]
    fn a_bundle_is_the_outermost_app_folder() {
        assert_eq!(app_bundle_of(GROK), Some("/Applications/Grok Bot.app"));
        // A helper deep inside an app belongs to the app, not to its own nested `.app`.
        assert_eq!(
            app_bundle_of(GROK_HELPER),
            Some("/Applications/Grok Bot.app")
        );
        assert_eq!(
            app_bundle_of("/System/Library/CoreServices/Finder.app/Contents/MacOS/Finder"),
            Some("/System/Library/CoreServices/Finder.app")
        );
    }

    #[test]
    fn what_is_in_no_app_has_no_bundle() {
        for path in [
            "",
            "/bin/zsh",
            "/usr/libexec/trustd",
            "/System/Library/Frameworks/WebKit.framework/XPCServices/com.apple.WebKit.WebContent.xpc/Contents/MacOS/com.apple.WebKit.WebContent",
            // `.application` and `.app-build` are not `.app`.
            "/x/thing.application/bin/y",
            "/x/thing.app-build/bin/y",
            // a bundle has a name
            "/.app/Contents/MacOS/x",
            ".app/x",
            // Windows paths (in Samo's one shape) have none
            "C:/Program Files/Chrome/chrome.exe",
        ] {
            assert_eq!(app_bundle_of(path), None, "{path:?}");
        }
    }

    #[test]
    fn an_app_is_named_by_its_folder() {
        assert_eq!(app_name_of("/Applications/Grok Bot.app"), "Grok Bot");
        assert_eq!(app_name_of("Finder.app"), "Finder");
        assert_eq!(app_name_of("/odd/folder"), "folder");
    }

    fn sample() -> Vec<ProcessRow> {
        vec![
            row(100, "Grok Bot", GROK, 1.0, 1_000),
            row(101, "Grok Bot Helper (Renderer)", GROK_HELPER, 4.0, 3_000),
            row(102, "Grok Bot Helper (Renderer)", GROK_HELPER, 2.5, 2_000),
            row(7, "trustd", "/usr/libexec/trustd", 0.2, 500),
            row(8, "launchd", "/sbin/launchd", 0.1, 400),
        ]
    }

    #[test]
    fn helpers_roll_up_into_their_app_with_whole_totals() {
        let rows = sample();
        let groups = group_processes(&rows, &HashSet::new());
        assert_eq!(groups.len(), 3, "Grok Bot, trustd, launchd");
        let app = &groups[0];
        assert_eq!(app.key, GroupKey::App("/Applications/Grok Bot.app".into()));
        assert_eq!(app.name, "Grok Bot");
        assert_eq!(app.bundle.as_deref(), Some("/Applications/Grok Bot.app"));
        assert_eq!(app.members, vec![0, 1, 2]);
        assert_eq!(app.memory_bytes, 6_000);
        assert!((app.cpu_pct - 7.5).abs() < 1e-4);
        assert_eq!(app.disk_bytes_per_sec, 45);
        // A process in no app stays alone, under its own name, with no icon source.
        assert_eq!(groups[1].key, GroupKey::Process(7));
        assert_eq!(groups[1].name, "trustd");
        assert_eq!(groups[1].bundle, None);
    }

    #[test]
    fn every_process_is_in_exactly_one_group() {
        let rows = sample();
        let groups = group_processes(&rows, &HashSet::new());
        let mut seen: Vec<usize> = groups.iter().flat_map(|g| g.members.clone()).collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..rows.len()).collect::<Vec<_>>());
        let total: u64 = groups.iter().map(|g| g.memory_bytes).sum();
        assert_eq!(total, rows.iter().map(|r| r.memory_bytes).sum::<u64>());
    }

    #[test]
    fn an_application_is_one_the_os_says_is_in_the_dock() {
        let rows = sample();
        // Only the helper is known to the OS as running; the app it belongs to is an application.
        let groups = group_processes(&rows, &HashSet::from([101]));
        assert!(groups[0].application);
        assert!(!groups[1].application && !groups[2].application);
        // Nothing known: nothing is an application.
        assert!(
            group_processes(&rows, &HashSet::new())
                .iter()
                .all(|g| !g.application)
        );
    }

    #[test]
    fn a_group_is_as_hungry_as_its_hungriest_process() {
        let mut rows = sample();
        rows[1].energy_impact = EnergyImpact::VeryHigh;
        rows[2].energy_impact = EnergyImpact::Moderate;
        let groups = group_processes(&rows, &HashSet::new());
        assert_eq!(groups[0].energy, EnergyImpact::VeryHigh);
        assert_eq!(groups[1].energy, EnergyImpact::Low);
    }

    #[test]
    fn groups_sort_by_the_chosen_column() {
        let rows = sample();
        let order = |by: &str| {
            let mut groups = group_processes(&rows, &HashSet::new());
            sort_groups(&mut groups, &rows, by);
            groups.into_iter().map(|g| g.name).collect::<Vec<_>>()
        };
        assert_eq!(order("cpu"), ["Grok Bot", "trustd", "launchd"]);
        assert_eq!(order("memory"), ["Grok Bot", "trustd", "launchd"]);
        assert_eq!(order("name"), ["Grok Bot", "launchd", "trustd"]);
        // By PID: an app is where its lowest process is (100), after launchd (8) and trustd (7).
        assert_eq!(order("pid"), ["trustd", "launchd", "Grok Bot"]);
    }

    #[test]
    fn typing_shows_the_matching_processes_or_the_whole_app() {
        let rows = sample();
        let groups = group_processes(&rows, &HashSet::new());
        let app = &groups[0];
        assert_eq!(visible_members(app, &rows, ""), vec![0, 1, 2]);
        assert_eq!(visible_members(app, &rows, "  "), vec![0, 1, 2]);
        // The app's own name: all of it.
        assert_eq!(visible_members(app, &rows, "grok"), vec![0, 1, 2]);
        // Something only the path or the PID has: just those.
        assert_eq!(visible_members(app, &rows, "renderer"), vec![1, 2]);
        assert_eq!(visible_members(app, &rows, "101"), vec![1]);
        // Nothing: the group goes.
        assert!(visible_members(app, &rows, "zzz").is_empty());
        assert_eq!(visible_members(&groups[1], &rows, "TRUST"), vec![3]);
    }
}
