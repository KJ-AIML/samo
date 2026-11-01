//! The store's model (ADR-003 M1, M2), apart from drawing: which rows the list shows and in what order, their
//! tags, how many updates wait, and a description's paragraphs and bullets.

use crate::components::TagKind;
use samo_domain::extensions::samo_satisfies;
use samo_domain::store::{Catalog, FileRef, Listing, newer, rank};
use samo_ext_protocol::{Change, PROTOCOL_VERSION};
use samo_extensions::{ExtSlot, ExtSnapshot};

/// This Samo's version.
pub const SAMO_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment {
    All,
    Installed,
    Updates,
}

/// A row's state, and its tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    /// In the store, not installed.
    Available,
    NeedsNewerSamo,
    Installed,
    Off,
    /// The store has a newer version.
    Update,
    /// Installed from a developer's folder.
    Dev,
}

impl RowState {
    pub fn tag(self) -> Option<(&'static str, TagKind)> {
        match self {
            RowState::Available => None,
            RowState::NeedsNewerSamo => Some(("needs a newer Samo", TagKind::Neutral)),
            RowState::Installed => Some(("installed", TagKind::Neutral)),
            RowState::Off => Some(("off", TagKind::Neutral)),
            RowState::Update => Some(("update", TagKind::Accent)),
            RowState::Dev => Some(("dev", TagKind::Neutral)),
        }
    }
}

/// What is installed under a row's id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledAs {
    pub slot: ExtSlot,
    pub version: String,
    pub dev: bool,
    pub on: bool,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone)]
pub struct Row {
    /// The store's listing; for an installed extension the store doesn't list, one made from its manifest.
    pub listing: Listing,
    pub in_store: bool,
    pub installed: Option<InstalledAs>,
    pub state: RowState,
}

/// Whether this Samo can run `listing`'s version.
pub fn can_run(listing: &Listing, samo: &str) -> bool {
    listing.protocol == PROTOCOL_VERSION && samo_satisfies(&listing.min_samo, samo)
}

pub fn row_state(
    listing: Option<&Listing>,
    installed: Option<&InstalledAs>,
    samo: &str,
) -> RowState {
    match (installed, listing) {
        (None, Some(l)) if !can_run(l, samo) => RowState::NeedsNewerSamo,
        (None, _) => RowState::Available,
        (Some(i), _) if i.dev => RowState::Dev,
        (Some(i), Some(l)) if newer(&l.version, &i.version) && can_run(l, samo) => RowState::Update,
        (Some(i), _) if !i.on => RowState::Off,
        (Some(_), _) => RowState::Installed,
    }
}

/// Every extension in the store and every installed one: installed first, in the order they were installed, then
/// the rest by name.
pub fn rows(catalog: Option<&Catalog>, ext: &ExtSnapshot, samo: &str) -> Vec<Row> {
    let listed = |id: &str| catalog.and_then(|c| c.extensions.iter().find(|l| l.id == id));
    let mut out: Vec<Row> = ext
        .extensions
        .iter()
        .map(|e| {
            let installed = InstalledAs {
                slot: e.slot,
                version: e.version.clone(),
                dev: e.dev,
                on: e.on,
                changes: e.changes.clone(),
            };
            let store = listed(&e.id);
            let state = row_state(store, Some(&installed), samo);
            let listing = store.cloned().unwrap_or_else(|| Listing {
                id: e.id.clone(),
                name: e.name.clone(),
                author: String::new(),
                summary: e.summary.clone(),
                description: String::new(),
                categories: Vec::new(),
                homepage: String::new(),
                version: e.version.clone(),
                protocol: PROTOCOL_VERSION,
                min_samo: String::new(),
                whats_new: String::new(),
                package: FileRef {
                    file: String::new(),
                    size: None,
                    sha256: String::new(),
                    width: None,
                    height: None,
                },
                icon: None,
                screenshots: Vec::new(),
                changes: e.changes.clone(),
            });
            Row {
                listing,
                in_store: store.is_some(),
                installed: Some(installed),
                state,
            }
        })
        .collect();
    let mut rest: Vec<Row> = catalog
        .map(|c| c.extensions.as_slice())
        .unwrap_or_default()
        .iter()
        .filter(|l| !ext.extensions.iter().any(|e| e.id == l.id))
        .map(|l| Row {
            listing: l.clone(),
            in_store: true,
            installed: None,
            state: row_state(Some(l), None, samo),
        })
        .collect();
    rest.sort_by(|a, b| a.listing.name.cmp(&b.listing.name));
    out.extend(rest);
    out
}

/// The rows a segment and a search show, in order: the list's order, or by match once there is a search.
pub fn visible(rows: &[Row], segment: Segment, query: &str) -> Vec<usize> {
    let in_segment: Vec<usize> = (0..rows.len())
        .filter(|&i| match segment {
            Segment::All => true,
            Segment::Installed => rows[i].installed.is_some(),
            Segment::Updates => rows[i].state == RowState::Update,
        })
        .collect();
    let listings: Vec<Listing> = in_segment
        .iter()
        .map(|&i| rows[i].listing.clone())
        .collect();
    rank(query, &listings)
        .into_iter()
        .filter_map(|j| in_segment.get(j).copied())
        .collect()
}

/// How many installed extensions the store has a newer version of (the sidebar's count, from the cache).
pub fn update_count(catalog: Option<&Catalog>, ext: &ExtSnapshot, samo: &str) -> usize {
    rows(catalog, ext, samo)
        .iter()
        .filter(|r| r.state == RowState::Update)
        .count()
}

/// A description's blocks: plain paragraphs, and runs of `- ` lines drawn as bullets. No markdown engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Para(String),
    Bullets(Vec<String>),
}

pub fn description_blocks(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let mut bullets: Vec<String> = Vec::new();
    let flush_para = |para: &mut Vec<&str>, blocks: &mut Vec<Block>| {
        if !para.is_empty() {
            blocks.push(Block::Para(para.join(" ")));
            para.clear();
        }
    };
    let flush_bullets = |bullets: &mut Vec<String>, blocks: &mut Vec<Block>| {
        if !bullets.is_empty() {
            blocks.push(Block::Bullets(std::mem::take(bullets)));
        }
    };
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            flush_para(&mut para, &mut blocks);
            flush_bullets(&mut bullets, &mut blocks);
        } else if let Some(item) = line.strip_prefix("- ") {
            flush_para(&mut para, &mut blocks);
            bullets.push(item.trim().to_string());
        } else {
            flush_bullets(&mut bullets, &mut blocks);
            para.push(line);
        }
    }
    flush_para(&mut para, &mut blocks);
    flush_bullets(&mut bullets, &mut blocks);
    blocks
}

/// The backups an extension says it keeps (`… (backup: name)` in a change's place), as paths; uninstalling
/// leaves them, and the page says so.
pub fn kept_backups(changes: &[Change]) -> Vec<String> {
    let mut out: Vec<String> = changes
        .iter()
        .filter_map(|c| {
            let (path, rest) = c.place.split_once(" (backup: ")?;
            let name = rest.split(')').next()?.trim();
            let dir = path.rsplit_once('/').map(|(d, _)| d)?;
            Some(format!("{dir}/{name}"))
        })
        .collect();
    out.dedup();
    out
}

/// "1.2 MB" for a package's size.
pub fn size_words(bytes: Option<u64>) -> Option<String> {
    bytes.map(crate::theme::format_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use samo_extensions::{ExtView, Status};
    use std::sync::Arc;

    fn listing(id: &str, version: &str, min_samo: &str) -> Listing {
        Listing {
            id: id.into(),
            name: id.to_uppercase(),
            author: "KJ".into(),
            summary: format!("{id} summary"),
            description: String::new(),
            categories: vec!["tools".into()],
            homepage: String::new(),
            version: version.into(),
            protocol: 1,
            min_samo: min_samo.into(),
            whats_new: String::new(),
            package: FileRef {
                file: format!("{id}.tar.gz"),
                size: Some(1),
                sha256: "0".repeat(64),
                width: None,
                height: None,
            },
            icon: None,
            screenshots: Vec::new(),
            changes: Vec::new(),
        }
    }

    fn installed(slot: u8, id: &str, version: &str, dev: bool, on: bool) -> ExtView {
        ExtView {
            slot: ExtSlot(slot),
            id: id.into(),
            name: id.to_uppercase(),
            summary: String::new(),
            version: version.into(),
            dev,
            on,
            status: Status::Running,
            note: None,
            changes: Vec::new(),
            view: Arc::new(Vec::new()),
            badge: 0,
            badge_summary: String::new(),
            badge_meter: None,
        }
    }

    #[test]
    fn row_state_tags() {
        let catalog = Catalog {
            schema: 1,
            published_at: "2026-11-02T10:00:00Z".into(),
            samo: None,
            extensions: vec![
                listing("zeta", "0.1.0", "0.1.0"),
                listing("alpha", "0.1.0", "0.1.0"),
                listing("future", "0.1.0", "9.0.0"),
                listing("hello", "0.3.0", "0.1.0"),
                listing("claude", "0.2.0", "0.1.0"),
                listing("quiet", "0.1.0", "0.1.0"),
            ],
        };
        let ext = ExtSnapshot {
            extensions: vec![
                installed(0, "claude", "0.2.0", true, true),
                installed(1, "hello", "0.2.0", false, true),
                installed(2, "quiet", "0.1.0", false, false),
                installed(3, "mine", "0.1.0", true, true),
            ],
            problems: Vec::new(),
        };
        let rows = rows(Some(&catalog), &ext, "0.2.0");
        let got: Vec<(&str, Option<&str>)> = rows
            .iter()
            .map(|r| (r.listing.id.as_str(), r.state.tag().map(|t| t.0)))
            .collect();
        // Installed first, in install order; then the store's others by name.
        assert_eq!(
            got,
            vec![
                ("claude", Some("dev")),
                ("hello", Some("update")),
                ("quiet", Some("off")),
                ("mine", Some("dev")),
                ("alpha", None),
                ("future", Some("needs a newer Samo")),
                ("zeta", None),
            ]
        );
        assert!(
            rows.iter()
                .find(|r| r.listing.id == "mine")
                .is_some_and(|r| !r.in_store)
        );
        assert_eq!(update_count(Some(&catalog), &ext, "0.2.0"), 1);
        assert_eq!(update_count(None, &ext, "0.2.0"), 0);

        assert_eq!(visible(&rows, Segment::Updates, ""), vec![1]);
        assert_eq!(visible(&rows, Segment::Installed, "").len(), 4);
        // Search: a name match before a summary or category match.
        assert_eq!(visible(&rows, Segment::All, "ze"), vec![6]);
        assert_eq!(visible(&rows, Segment::All, "tools").len(), 6);
        assert!(visible(&rows, Segment::Installed, "zeta").is_empty());
        // An installed, current extension is "installed".
        let i = InstalledAs {
            slot: ExtSlot(0),
            version: "0.3.0".into(),
            dev: false,
            on: true,
            changes: Vec::new(),
        };
        assert_eq!(
            row_state(Some(&listing("hello", "0.3.0", "0.1.0")), Some(&i), "0.2.0"),
            RowState::Installed
        );
        // A newer version this Samo can't run isn't offered as an update.
        assert_eq!(
            row_state(Some(&listing("hello", "0.4.0", "9.0.0")), Some(&i), "0.2.0"),
            RowState::Installed
        );
    }

    #[test]
    fn description_paragraphs_and_bullets() {
        let text =
            "First line\nstill the first paragraph.\n\nSecond.\n- one\n-  two \n\n- three\nAfter.";
        assert_eq!(
            description_blocks(text),
            vec![
                Block::Para("First line still the first paragraph.".into()),
                Block::Para("Second.".into()),
                Block::Bullets(vec!["one".into(), "two".into()]),
                Block::Bullets(vec!["three".into()]),
                Block::Para("After.".into()),
            ]
        );
        assert!(description_blocks("").is_empty());
        assert_eq!(
            description_blocks("-not a bullet"),
            vec![Block::Para("-not a bullet".into())]
        );
    }

    #[test]
    fn backups_named_from_changes() {
        let change = |place: &str| Change {
            what: String::new(),
            place: place.into(),
            undo: String::new(),
        };
        assert_eq!(
            kept_backups(&[
                change("~/.claude/settings.json (backup: settings.json.samo-bak)"),
                change("~/.claude/settings.json"),
            ]),
            vec!["~/.claude/settings.json.samo-bak".to_string()]
        );
        assert!(kept_backups(&[change("nowhere")]).is_empty());
    }
}
