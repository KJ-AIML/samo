mod art;
mod assets;
mod charts;
mod components;
mod disk_view;
mod hub;
mod layout;
mod live;
mod locations;
mod menubar;
mod overview;
mod performance;
mod processes;
mod projects;
mod shell;
mod storage;
mod theme;

use disk_view::DiskView;
use futures::StreamExt;
use gpui::{
    App, Application, Bounds, Context, FocusHandle, FontWeight, Global, IntoElement, KeyBinding,
    KeyDownEvent, Menu, MenuItem, ParentElement, Render, SharedString, Styled, TitlebarOptions,
    UniformListScrollHandle, Window, WindowAppearance, WindowBounds, WindowHandle, WindowOptions,
    actions, div, point, prelude::*, px, size,
};
use hub::Hub;
use layout::Layout;
use samo_blueprint::ScanConfig;
use samo_macos::{
    default_user_trash_dir, move_to_trash, restore_from_trash, scan_directory,
    terminate_verified_process,
};
use samo_primitives::{FileNodeIdentity, ProcessIdentity, ScanSnapshot, TelemetrySnapshot};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use theme::*;

actions!(samo, [Quit, ShowWindow]);

/// What lives for the whole app, not one window: the background work, the menu bar item, and handles on the
/// main window and the quick panel (either may be closed while Samo keeps running in the menu bar).
struct AppState {
    hub: Hub,
    main: Option<WindowHandle<SamoWorkspaceApp>>,
    panel: Option<WindowHandle<menubar::QuickPanel>>,
    panel_closed_at: Option<Instant>,
    menubar: Option<menubar::MenuBar>,
}

impl Global for AppState {}

/// One render's worth of shared state.
fn snapshot(hub: &Hub, layout: Layout) -> Option<Frame> {
    let g = hub.shared.lock().ok()?;
    Some(Frame {
        scan_root: g.scan_root.clone(),
        is_scanning: g.is_scanning,
        scan_error: g.scan_error.clone(),
        notice: g.notice.clone(),
        has_undo: g.last_trash.is_some(),
        disk: g.disk.clone(),
        telemetry: g.telemetry.clone(),
        telemetry_ready: g.telemetry_ready,
        telemetry_view: g.telemetry_view,
        busiest: g.busiest.clone(),
        thermal: g.thermal,
        layout,
    })
}

/// Brings the main window forward (opening it again if it was closed), optionally on a page.
fn open_main(cx: &mut App, nav: Option<NavSection>) {
    if let Some(handle) = cx.global::<AppState>().main {
        let reused = handle
            .update(cx, |view, window, cx| {
                if let Some(section) = nav {
                    view.nav = section;
                }
                window.activate_window();
                cx.notify();
            })
            .is_ok();
        if reused {
            cx.activate(true);
            return;
        }
    }
    let bounds = Bounds::centered(None, size(px(1360.0), px(880.0)), cx);
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(640.0), px(520.0))),
            titlebar: Some(TitlebarOptions {
                title: Some(SharedString::from("Samo")),
                appears_transparent: true,
                traffic_light_position: Some(point(px(20.0), px(20.0))),
            }),
            ..Default::default()
        },
        |window, cx| {
            let view = cx.new(|cx| SamoWorkspaceApp::new(window, cx));
            if let Some(section) = nav {
                view.update(cx, |v, _| v.nav = section);
            }
            let focus = view.read(cx).focus.clone();
            window.focus(&focus);
            view
        },
    );
    if let Ok(handle) = opened {
        cx.global_mut::<AppState>().main = Some(handle);
        cx.activate(true);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NavSection {
    Overview,
    Projects,
    Storage,
    Processes,
    Performance,
}

impl NavSection {
    const ALL: [Self; 5] = [
        Self::Overview,
        Self::Projects,
        Self::Storage,
        Self::Processes,
        Self::Performance,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Projects => "Projects",
            Self::Storage => "Storage",
            Self::Processes => "Processes",
            Self::Performance => "Performance",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Overview => "gauge",
            Self::Projects => "package",
            Self::Storage => "bubbles",
            Self::Processes => "pulse",
            Self::Performance => "chip",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessSort {
    Memory,
    Cpu,
    Disk,
    Pid,
}

impl ProcessSort {
    fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Cpu => "cpu",
            Self::Disk => "disk",
            Self::Pid => "pid",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Cpu => "CPU",
            Self::Disk => "disk I/O",
            Self::Pid => "PID",
        }
    }
}

#[derive(Debug, Clone)]
struct PendingTrashConfirm {
    label: String,
    target_path: String,
    bytes: u64,
    identity: FileNodeIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoticeKind {
    Progress,
    Success,
    Error,
}

#[derive(Debug, Clone)]
struct Notice {
    text: String,
    kind: NoticeKind,
}

impl Notice {
    fn new(kind: NoticeKind, text: impl Into<String>) -> Option<Self> {
        Some(Self {
            text: text.into(),
            kind,
        })
    }
}

/// Everything one render pass needs, copied out of the shared lock.
struct Frame {
    scan_root: String,
    is_scanning: bool,
    scan_error: Option<String>,
    notice: Option<Notice>,
    has_undo: bool,
    /// The last finished scan (it may be of another folder while a new one runs).
    disk: Option<Arc<DiskView>>,
    telemetry: Arc<TelemetrySnapshot>,
    telemetry_ready: bool,
    /// The sort and filter the process list in `telemetry` was taken with.
    telemetry_view: (ProcessSort, bool),
    /// The process using the most CPU right now, across all processes.
    busiest: Option<(String, f32)>,
    thermal: Option<hub::Thermal>,
    layout: Layout,
}

impl Frame {
    /// The scan of the folder currently selected; `None` while a different folder loads.
    fn view(&self) -> Option<&DiskView> {
        self.disk
            .as_deref()
            .filter(|d| d.snap.root_path == self.scan_root)
    }

    fn current_disk(&self) -> Option<&ScanSnapshot> {
        self.view().map(|v| v.snap.as_ref())
    }

    /// Volume figures come with any scan, whichever folder it was of.
    fn volume(&self) -> Option<&ScanSnapshot> {
        self.disk
            .as_deref()
            .map(|v| v.snap.as_ref())
            .filter(|d| d.volume_total_bytes > 0)
    }

    fn reclaimable_bytes(&self) -> Option<u64> {
        self.view().map(|v| v.reclaimable)
    }
}

struct SamoWorkspaceApp {
    hub: Hub,
    nav: NavSection,
    proc_sort: ProcessSort,
    include_self: bool,
    pending_confirm: Option<PendingTrashConfirm>,
    trash_dir: PathBuf,
    focus: FocusHandle,
    /// The circle last pointed at (it stays shown, so its actions can be reached).
    hovered_ball: Option<String>,
    /// The location menu in a disk page's header is open.
    location_menu: bool,
    location_menu_closed_at: Option<Instant>,
    /// Where Samo can measure: the standard folders, plus any you added.
    locations: Vec<locations::Location>,
    /// While the pointer is over the process table its rows hold still, so a row can't move under a click.
    table_hovered: bool,
    held_procs: Option<Arc<TelemetrySnapshot>>,
    /// What the virtualised process list draws from, set each render.
    proc_rows: Arc<TelemetrySnapshot>,
    proc_cols: processes::Columns,
    proc_scroll: UniformListScrollHandle,
    /// How much history Overview's live charts show, and where the pointer is over each.
    live_span: live::Span,
    live_hover: [live::ChartHover; 3],
}

fn resolve_default_root() -> String {
    if let Ok(custom) = env::var("SAMO_DEFAULT_SCAN_ROOT") {
        return custom;
    }
    if let Ok(home) = env::var("HOME") {
        let dev = PathBuf::from(&home).join("Developer");
        if dev.is_dir() {
            return dev.to_string_lossy().into_owned();
        }
        return home;
    }
    "/".to_string()
}

/// `SAMO_START_PAGE=projects|storage|processes|performance` opens on that page.
fn resolve_start_page() -> NavSection {
    let wanted = env::var("SAMO_START_PAGE")
        .unwrap_or_default()
        .to_ascii_lowercase();
    NavSection::ALL
        .into_iter()
        .find(|s| s.title().to_ascii_lowercase() == wanted)
        .unwrap_or(NavSection::Overview)
}

/// How many recent scans are kept for instant back-and-forth.
const CACHE_LIMIT: usize = 8;

/// Keeps `view` as the newest recent scan (replacing an older one of the same folder).
fn remember(cache: &mut Vec<Arc<DiskView>>, view: Arc<DiskView>) {
    cache.retain(|v| v.snap.root_path != view.snap.root_path);
    cache.insert(0, view);
    cache.truncate(CACHE_LIMIT);
}

/// Forgets recent scans that contain `path` or lie inside it: they no longer describe the disk.
fn forget_around(cache: &mut Vec<Arc<DiskView>>, path: &str) {
    cache.retain(|v| !is_within(path, &v.snap.root_path) && !is_within(&v.snap.root_path, path));
}

fn is_within(path: &str, parent: &str) -> bool {
    path == parent || path.starts_with(&format!("{}/", parent.trim_end_matches('/')))
}

/// Drops rows that now live in the Trash so they can't be acted on again while the
/// follow-up rescan runs. Totals are left for the rescan to correct.
fn prune_trashed(snapshot: &ScanSnapshot, trashed: &str) -> ScanSnapshot {
    let mut next = snapshot.clone();
    next.largest_folders
        .retain(|f| !is_within(&f.path, trashed));
    next.largest_files.retain(|f| !is_within(&f.path, trashed));
    next.balls.retain(|b| !is_within(&b.path, trashed));
    next.projects.retain(|p| !is_within(&p.root_path, trashed));
    for project in &mut next.projects {
        let removed: u64 = project
            .artifacts
            .iter()
            .filter(|a| is_within(&a.path, trashed))
            .map(|a| a.bytes)
            .sum();
        if removed > 0 {
            project.artifacts.retain(|a| !is_within(&a.path, trashed));
            project.artifact_bytes = project.artifact_bytes.saturating_sub(removed);
            project.total_bytes = project.total_bytes.saturating_sub(removed);
        }
    }
    next
}

impl SamoWorkspaceApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let hub = cx.global::<AppState>().hub.clone();

        // Light or dark follows the system, unless SAMO_THEME pins one.
        cx.observe_window_appearance(window, |_, _, cx| cx.notify())
            .detach();

        let placeholder = hub.shared.lock().map(|g| g.telemetry.clone()).ok();

        Self {
            hub,
            nav: resolve_start_page(),
            proc_sort: ProcessSort::Memory,
            include_self: false,
            pending_confirm: None,
            trash_dir: default_user_trash_dir()
                .unwrap_or_else(|_| PathBuf::from("/tmp/.samo-trash")),
            focus: cx.focus_handle(),
            hovered_ball: None,
            location_menu: false,
            location_menu_closed_at: None,
            locations: locations::load(),
            table_hovered: false,
            held_procs: None,
            proc_rows: placeholder.unwrap_or_else(|| Arc::new(hub::placeholder_telemetry())),
            proc_cols: processes::Columns::all(),
            proc_scroll: UniformListScrollHandle::new(),
            live_span: live::Span::TwoMin,
            live_hover: Default::default(),
        }
    }

    fn frame(&self, layout: Layout) -> Option<Frame> {
        snapshot(&self.hub, layout)
    }

    /// Measures `root_path` on its own thread. Unless `fresh`, a recent scan of the same folder is shown at once
    /// instead (it says when it was measured; Rescan measures again).
    fn spawn_scan(hub: &Hub, root_path: String, fresh: bool) {
        let generation = match hub.shared.lock() {
            Ok(mut guard) => {
                let cached = (!fresh)
                    .then(|| {
                        guard
                            .cache
                            .iter()
                            .find(|v| v.snap.root_path == root_path)
                            .cloned()
                    })
                    .flatten();
                if let Some(view) = cached {
                    guard.scan_generation += 1;
                    guard.scan_root = root_path;
                    guard.is_scanning = false;
                    guard.scan_error = None;
                    if guard
                        .notice
                        .as_ref()
                        .is_some_and(|n| n.kind == NoticeKind::Progress)
                    {
                        guard.notice = None;
                    }
                    guard.disk = Some(view);
                    drop(guard);
                    hub.wake_ui();
                    return;
                }
                guard.scan_generation += 1;
                guard.scan_root = root_path.clone();
                guard.is_scanning = true;
                guard.scan_error = None;
                guard.notice = Notice::new(
                    NoticeKind::Progress,
                    format!("Measuring {}…", display_path(&root_path)),
                );
                guard.scan_generation
            }
            Err(_) => return,
        };
        // Let the sampler start stepping the UI for the probe's sweep.
        hub.poke();

        let hub = hub.clone();
        std::thread::spawn(move || {
            let res = scan_directory(
                "scn_gpui".to_string(),
                Path::new(&root_path),
                ScanConfig::default(),
            );
            // The derived view is built here, off the UI thread.
            let view = res.map(|snap| Arc::new(DiskView::build(Arc::new(snap))));
            if let Ok(mut guard) = hub.shared.lock() {
                if guard.scan_generation != generation {
                    return;
                }
                guard.is_scanning = false;
                match view {
                    Ok(view) => {
                        // The page itself now shows the result; keep any outcome (a trash, an error) in view.
                        if guard
                            .notice
                            .as_ref()
                            .is_some_and(|n| n.kind == NoticeKind::Progress)
                        {
                            guard.notice = None;
                        }
                        remember(&mut guard.cache, view.clone());
                        guard.disk = Some(view);
                    }
                    Err(err) => {
                        guard.scan_error = Some(err.message.clone());
                        guard.notice = Notice::new(NoticeKind::Error, err.message);
                    }
                }
            }
            hub.wake_ui();
        });
    }

    fn trigger_scan(&mut self, path: String, cx: &mut Context<Self>) {
        let already_running = self
            .hub
            .shared
            .lock()
            .map(|g| g.is_scanning && g.scan_root == path)
            .unwrap_or(false);
        if already_running {
            return;
        }
        self.pending_confirm = None;
        self.location_menu = false;
        self.hovered_ball = None;
        Self::spawn_scan(&self.hub, path, false);
        cx.notify();
    }

    /// Measure the current folder again, ignoring any recent result.
    fn rescan(&mut self, cx: &mut Context<Self>) {
        let (root, running) = self
            .hub
            .shared
            .lock()
            .map(|g| (g.scan_root.clone(), g.is_scanning))
            .unwrap_or_else(|_| (resolve_default_root(), false));
        if running {
            return;
        }
        self.pending_confirm = None;
        Self::spawn_scan(&self.hub, root, true);
        cx.notify();
    }

    fn set_nav(&mut self, section: NavSection, cx: &mut Context<Self>) {
        self.nav = section;
        cx.notify();
    }

    fn request_trash_confirmation(
        &mut self,
        label: String,
        target_path: String,
        bytes: u64,
        identity: FileNodeIdentity,
        cx: &mut Context<Self>,
    ) {
        self.pending_confirm = Some(PendingTrashConfirm {
            label,
            target_path,
            bytes,
            identity,
        });
        cx.notify();
    }

    fn execute_confirmed_trash(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.pending_confirm.take() else {
            return;
        };
        let current_root = self
            .hub
            .shared
            .lock()
            .ok()
            .map(|g| g.scan_root.clone())
            .unwrap_or_else(resolve_default_root);

        match move_to_trash(
            Path::new(&confirm.target_path),
            confirm.identity,
            &self.trash_dir,
        ) {
            Ok(outcome) => {
                if let Ok(mut guard) = self.hub.shared.lock() {
                    forget_around(&mut guard.cache, &confirm.target_path);
                }
                Self::spawn_scan(&self.hub, current_root, true);
                if let Ok(mut guard) = self.hub.shared.lock() {
                    guard.notice = Notice::new(
                        NoticeKind::Success,
                        format!(
                            "Moved “{}” ({}) to the Trash.",
                            confirm.label,
                            format_bytes(outcome.reclaimed_bytes)
                        ),
                    );
                    guard.disk = guard.disk.as_deref().map(|view| {
                        Arc::new(DiskView::build(Arc::new(prune_trashed(
                            &view.snap,
                            &confirm.target_path,
                        ))))
                    });
                    guard.last_trash = Some(outcome);
                }
                if self.hovered_ball.as_deref() == Some(confirm.target_path.as_str()) {
                    self.hovered_ball = None;
                }
            }
            Err(err) => {
                if let Ok(mut guard) = self.hub.shared.lock() {
                    guard.notice = Notice::new(NoticeKind::Error, err.message);
                }
            }
        }
        cx.notify();
    }

    fn undo_last_trash(&mut self, cx: &mut Context<Self>) {
        let outcome_opt = self
            .hub
            .shared
            .lock()
            .ok()
            .and_then(|mut g| g.last_trash.take());
        let Some(outcome) = outcome_opt else {
            return;
        };
        let current_root = self
            .hub
            .shared
            .lock()
            .ok()
            .map(|g| g.scan_root.clone())
            .unwrap_or_else(resolve_default_root);

        match restore_from_trash(
            Path::new(&outcome.trashed_path),
            Path::new(&outcome.original_path),
        ) {
            Ok(()) => {
                if let Ok(mut guard) = self.hub.shared.lock() {
                    forget_around(&mut guard.cache, &outcome.original_path);
                }
                Self::spawn_scan(&self.hub, current_root, true);
                if let Ok(mut guard) = self.hub.shared.lock() {
                    guard.notice = Notice::new(
                        NoticeKind::Success,
                        format!(
                            "Put {} back from the Trash.",
                            display_path(&outcome.original_path)
                        ),
                    );
                }
            }
            Err(err) => {
                if let Ok(mut guard) = self.hub.shared.lock() {
                    guard.notice = Notice::new(NoticeKind::Error, err.message);
                }
            }
        }
        cx.notify();
    }

    fn reveal_in_finder(&mut self, path: String, cx: &mut Context<Self>) {
        let _ = std::process::Command::new("/usr/bin/open")
            .arg("-R")
            .arg(&path)
            .status();
        cx.notify();
    }

    fn quit_process(
        &mut self,
        identity: ProcessIdentity,
        name: String,
        signal: &'static str,
        cx: &mut Context<Self>,
    ) {
        let notice = match terminate_verified_process(identity, signal) {
            Ok(()) => Notice::new(
                NoticeKind::Success,
                format!("Sent {signal} to {name} (PID {})", identity.pid),
            ),
            Err(err) => Notice::new(NoticeKind::Error, err.message),
        };
        if let Ok(mut guard) = self.hub.shared.lock() {
            guard.notice = notice;
        }
        self.refresh_processes(cx);
    }

    /// Ask the sampler for a list in the current sort; the table lets go of any held rows.
    fn refresh_processes(&mut self, cx: &mut Context<Self>) {
        self.held_procs = None;
        self.hub.set_process_view(self.include_self, self.proc_sort);
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.key == "escape" && (self.pending_confirm.is_some() || self.location_menu) {
            self.pending_confirm = None;
            self.location_menu = false;
            cx.notify();
            return;
        }
        if !keystroke.modifiers.platform {
            return;
        }
        match keystroke.key.as_str() {
            "r" => self.rescan(cx),
            "w" => window.remove_window(),
            key => {
                let picked = key
                    .parse::<usize>()
                    .ok()
                    .and_then(|n| n.checked_sub(1))
                    .and_then(|i| NavSection::ALL.get(i).copied());
                if let Some(section) = picked {
                    self.set_nav(section, cx);
                }
            }
        }
    }
}

impl Render for SamoWorkspaceApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        theme::set_dark(theme::pinned_dark().unwrap_or_else(|| {
            matches!(
                window.appearance(),
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            )
        }));
        let pal = theme::p();
        let layout = Layout::for_window(f32::from(window.viewport_size().width));

        let Some(frame) = self.frame(layout) else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(pal.frame)
                .text_color(pal.signal_text)
                .child("Samo lost track of its own state. Quit and open it again to recover.");
        };

        let page = match self.nav {
            NavSection::Overview => self.render_overview(&frame, cx),
            NavSection::Projects => self.render_projects(&frame, cx),
            NavSection::Storage => self.render_storage(&frame, cx),
            NavSection::Processes => self.render_processes(&frame, cx),
            NavSection::Performance => self.render_performance(&frame),
        };

        div()
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .bg(pal.frame)
            .text_color(pal.text)
            .font(theme::body(FontWeight::NORMAL))
            .text_size(px(T_BODY))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key_down))
            .child(self.render_sidebar(&frame, cx))
            .child(self.render_stage(page, &frame, cx))
            .children(
                self.pending_confirm
                    .clone()
                    .map(|conf| self.render_confirm_dialog(conf, cx)),
            )
    }
}

fn main() {
    let app = Application::new().with_assets(assets::Assets);
    // Closing the window leaves Samo in the menu bar; the Dock icon brings the window back.
    app.on_reopen(|cx| open_main(cx, None));
    app.run(|cx: &mut App| {
        theme::init_fonts(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action(|_: &ShowWindow, cx| open_main(cx, None));
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-0", ShowWindow, None),
        ]);
        cx.set_menus(vec![Menu {
            name: "Samo".into(),
            items: vec![
                MenuItem::action("Show Samo", ShowWindow),
                MenuItem::separator(),
                MenuItem::action("Quit Samo", Quit),
            ],
        }]);

        // The background work belongs to the app, so it keeps going while only the menu bar item shows.
        let (wake_tx, mut wake_rx) = futures::channel::mpsc::unbounded::<()>();
        let hub = Hub::new(resolve_default_root(), wake_tx);
        hub.start_sampler();
        let root = hub
            .shared
            .lock()
            .map(|g| g.scan_root.clone())
            .unwrap_or_else(|_| resolve_default_root());
        SamoWorkspaceApp::spawn_scan(&hub, root, true);
        cx.set_global(AppState {
            hub,
            main: None,
            panel: None,
            panel_closed_at: None,
            menubar: None,
        });
        let menubar = menubar::MenuBar::install(cx);
        cx.global_mut::<AppState>().menubar = menubar;

        // Draw when there is news, and only then: a burst of wakes becomes one frame.
        cx.spawn(async move |cx| {
            while wake_rx.next().await.is_some() {
                while wake_rx.try_recv().is_ok() {}
                let drawn = cx.update(|cx| {
                    cx.refresh_windows();
                    menubar::refresh(cx);
                });
                if drawn.is_err() {
                    break;
                }
            }
        })
        .detach();

        open_main(cx, None);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view_of(root: &str) -> Arc<DiskView> {
        Arc::new(DiskView::build(Arc::new(ScanSnapshot {
            scan_id: String::new(),
            root_path: root.to_string(),
            volume_name: String::new(),
            status: samo_primitives::ScanProgressStatus::Completed,
            started_at_ms: 0,
            completed_at_ms: Some(0),
            volume_total_bytes: 0,
            volume_used_bytes: 0,
            volume_free_bytes: 0,
            scanned_bytes: 0,
            scanned_files: 0,
            unreadable_paths: 0,
            categories: Vec::new(),
            largest_folders: Vec::new(),
            largest_files: Vec::new(),
            projects: Vec::new(),
            balls: Vec::new(),
        })))
    }

    fn roots(cache: &[Arc<DiskView>]) -> Vec<&str> {
        cache.iter().map(|v| v.snap.root_path.as_str()).collect()
    }

    #[test]
    fn recent_scans_are_newest_first_without_duplicates_and_bounded() {
        let mut cache = Vec::new();
        remember(&mut cache, view_of("/a"));
        remember(&mut cache, view_of("/b"));
        remember(&mut cache, view_of("/a"));
        assert_eq!(roots(&cache), vec!["/a", "/b"]);
        for i in 0..20 {
            remember(&mut cache, view_of(&format!("/x{i}")));
        }
        assert_eq!(cache.len(), CACHE_LIMIT);
        assert_eq!(roots(&cache)[0], "/x19");
    }

    #[test]
    fn a_trash_forgets_scans_that_contain_it_or_lie_inside_it() {
        let mut cache = Vec::new();
        for root in ["/u", "/u/dev", "/u/dev/app", "/u/dev/app/target", "/u/docs"] {
            remember(&mut cache, view_of(root));
        }
        forget_around(&mut cache, "/u/dev/app");
        let mut left = roots(&cache);
        left.sort();
        assert_eq!(left, vec!["/u/docs"]);
    }

    #[test]
    fn is_within_matches_self_and_children_only() {
        assert!(is_within("/a/b", "/a/b"));
        assert!(is_within("/a/b/c", "/a/b"));
        assert!(!is_within("/a/bc", "/a/b"));
        assert!(!is_within("/a", "/a/b"));
    }
}
