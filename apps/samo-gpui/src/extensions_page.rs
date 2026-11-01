//! Extensions (ADR-002, ADR-003): the store's list of every signed extension and every installed one, and each
//! one's detail page, where it is installed, updated, switched and uninstalled. Each extension's own page (its
//! widgets) is drawn by `ext_view.rs`.
//!
//! This is one of the two files allowed to make a `UserAction` (R12, the `network` gate): only in what a click
//! runs. Shown at launch (`SAMO_START_PAGE=extensions`), the store uses its cache and never goes online.

use crate::art::dither_block;
use crate::components::*;
use crate::shell::{Head, Page};
use crate::store_view::{
    Block, InstalledAs, Row, RowState, SAMO_VERSION, Segment, can_run, description_blocks,
    kept_backups, rows, size_words, visible,
};
use crate::text_input::{Changed, TextInput};
use crate::theme::*;
use crate::{Frame, SamoWorkspaceApp};
use gpui::{
    AnyElement, Context, Div, Entity, FontWeight, Image, ImageFormat, IntoElement, ObjectFit,
    ParentElement, SharedString, Styled, StyledImage, Subscription, Window, div, img, prelude::*,
    px,
};
use samo_domain::store::FileRef;
use samo_ext_protocol::Change;
use samo_extensions::net::UserAction;
use samo_extensions::worker::{JobKind, JobView, StoreJob, StoreView};
use samo_extensions::{ExtSlot, ExtView, Status};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// The tag for an extension's status.
pub fn status_tag(status: &Status) -> (&'static str, TagKind) {
    match status {
        Status::Running => ("running", TagKind::Accent),
        Status::Starting => ("starting", TagKind::Neutral),
        Status::Off => ("off", TagKind::Neutral),
        Status::Stopped(_) => ("stopped", TagKind::Signal),
        Status::Blocked(_) => ("can't run here", TagKind::Neutral),
    }
}

/// A question the detail page asks before acting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreConfirm {
    ReplaceDev(String),
    Uninstall(String),
}

/// The store's own state in the window.
pub struct StoreUi {
    pub segment: Segment,
    pub search: Entity<TextInput>,
    /// The extension whose detail page is open.
    pub detail: Option<String>,
    /// Which screenshot is shown.
    pub shot: usize,
    pub confirm: Option<StoreConfirm>,
    /// Images handed to gpui, by SHA-256. Screenshots leave when their page closes.
    images: HashMap<String, Arc<Image>>,
    /// Screenshots handed to gpui for the page on screen.
    shown_shots: HashSet<String>,
    _search_changed: Subscription,
}

impl StoreUi {
    pub fn new(cx: &mut Context<SamoWorkspaceApp>) -> Self {
        let search = cx.new(|cx| TextInput::new("Search extensions", cx));
        let sub = cx.subscribe(&search, |_, _, _: &Changed, cx| cx.notify());
        Self {
            segment: Segment::All,
            search,
            detail: None,
            shot: 0,
            confirm: None,
            images: HashMap::new(),
            shown_shots: HashSet::new(),
            _search_changed: sub,
        }
    }
}

fn image_format(bytes: &[u8]) -> ImageFormat {
    if bytes.starts_with(b"\x89PNG") {
        ImageFormat::Png
    } else {
        ImageFormat::Jpeg
    }
}

/// "14:02", or "Nov 2, 14:02" if not today.
fn clock(ms: i64) -> String {
    let Some(t) = chrono::DateTime::from_timestamp_millis(ms) else {
        return String::new();
    };
    let local = t.with_timezone(&chrono::Local);
    if local.date_naive() == chrono::Local::now().date_naive() {
        local.format("%H:%M").to_string()
    } else {
        local.format("%b %-d, %H:%M").to_string()
    }
}

/// What the store says about where its list came from, if anything.
fn store_note(view: &StoreView) -> Option<(String, bool)> {
    if view.refreshing {
        return Some(("Checking the store…".into(), false));
    }
    match (&view.offline, view.fetched_at_ms) {
        (Some(_), Some(at)) => Some((
            format!("Offline: showing the store as of {}.", clock(at)),
            false,
        )),
        (Some(why), None) => Some((
            format!("Couldn't reach the store ({why}). Showing installed extensions only."),
            true,
        )),
        (None, None) => Some((
            "The store hasn't been opened yet. Refresh to see what's in it.".into(),
            false,
        )),
        (None, Some(_)) => None,
    }
}

impl SamoWorkspaceApp {
    fn store_submit(&self, job: StoreJob, action: UserAction) {
        if let Some(store) = self.hub.store.get() {
            store.submit(job, action);
        }
    }

    /// The sidebar's or rail's Extensions, clicked: the store, with the catalog fetched if the cached one is over
    /// 10 minutes old. Only click handlers call this.
    pub(crate) fn store_opened_by_click(&mut self, cx: &mut Context<Self>) {
        self.store_ui.detail = None;
        self.store_ui.confirm = None;
        self.set_nav(crate::NavSection::Extensions, cx);
        self.store_submit(StoreJob::Refresh { force: false }, UserAction::clicked());
    }

    /// A row clicked: its detail page, and its screenshots downloaded.
    fn open_detail(&mut self, row: &Row, cx: &mut Context<Self>) {
        self.store_ui.detail = Some(row.listing.id.clone());
        self.store_ui.shot = 0;
        self.store_ui.confirm = None;
        if !row.listing.screenshots.is_empty() {
            let refs = row
                .listing
                .screenshots
                .iter()
                .map(|s| s.file.clone())
                .collect();
            self.store_submit(StoreJob::Screenshots(refs), UserAction::clicked());
        }
        cx.notify();
    }

    /// Hands gpui the verified bytes of an image, once.
    fn gpui_image(&mut self, r: &FileRef, view: &StoreView) -> Option<Arc<Image>> {
        if let Some(image) = self.store_ui.images.get(&r.sha256) {
            return Some(Arc::clone(image));
        }
        let bytes = view.images.get(&r.sha256)?.as_ref().ok()?;
        let image = Arc::new(Image::from_bytes(image_format(bytes), bytes.to_vec()));
        self.store_ui
            .images
            .insert(r.sha256.clone(), Arc::clone(&image));
        Some(image)
    }

    /// Frees the screenshots of a page that is no longer on screen: from gpui's textures and caches, and from the
    /// worker (R13, RF5). Icons stay; they are small.
    pub(crate) fn sweep_store_images(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let on_screen: HashSet<String> = match (&self.nav, &self.store_ui.detail) {
            (crate::NavSection::Extensions, Some(id)) => self
                .hub
                .store
                .get()
                .and_then(|s| s.view().catalog)
                .and_then(|c| c.extensions.iter().find(|l| &l.id == id).cloned())
                .map(|l| {
                    l.screenshots
                        .iter()
                        .map(|s| s.file.sha256.clone())
                        .collect()
                })
                .unwrap_or_default(),
            _ => HashSet::new(),
        };
        let gone: Vec<String> = self
            .store_ui
            .shown_shots
            .iter()
            .filter(|sha| !on_screen.contains(*sha))
            .cloned()
            .collect();
        if gone.is_empty() {
            return;
        }
        for sha in &gone {
            self.store_ui.shown_shots.remove(sha);
            if let Some(image) = self.store_ui.images.remove(sha) {
                if let Some(texture) = Arc::clone(&image).get_render_image(window, cx) {
                    cx.drop_image(texture, Some(window));
                }
                image.remove_asset(cx);
            }
        }
        if let Some(store) = self.hub.store.get() {
            store.forget_images(&gone);
        }
    }

    pub(crate) fn render_extensions(&mut self, frame: &Frame, cx: &mut Context<Self>) -> Page {
        let all = rows(frame.store.catalog.as_deref(), &frame.ext, SAMO_VERSION);
        if let Some(id) = self.store_ui.detail.clone() {
            if let Some(row) = all.iter().find(|r| r.listing.id == id) {
                return self.render_detail(row, frame, cx);
            }
            self.store_ui.detail = None;
        }
        self.render_store_list(&all, frame, cx)
    }

    fn render_store_list(&mut self, all: &[Row], frame: &Frame, cx: &mut Context<Self>) -> Page {
        let pal = p();
        let installed = frame.ext.extensions.len();
        let on = frame.ext.extensions.iter().filter(|e| e.on).count();
        let sub = match installed {
            0 => {
                "Extensions add to what Samo watches. Every one in the store is signed.".to_string()
            }
            n => format!("{n} installed, {on} on. Every one in the store is signed."),
        };
        let query = self.store_ui.search.read(cx).text().to_string();
        let picked = self.store_ui.segment;
        let shown = visible(all, picked, &query);
        let updates = visible(all, Segment::Updates, "").len();

        let mut content = div().flex().flex_col().gap(px(S4));
        for problem in &frame.ext.problems {
            content = content.child(error_line(&format!("{problem}.")));
        }
        if let Some((words, failed)) = store_note(&frame.store) {
            content = content.child(if frame.store.refreshing {
                loading(&words)
            } else if failed {
                error_line(&words)
            } else {
                div()
                    .font(body(FontWeight::NORMAL))
                    .text_size(px(T_META))
                    .text_color(pal.text_2)
                    .child(words)
            });
        }

        let seg = |id: &'static str, label: String, which: Segment, cx: &mut Context<Self>| {
            segment(id, &label, picked == which).on_click(cx.listener(move |this, _, _, cx| {
                this.store_ui.segment = which;
                cx.notify();
            }))
        };
        let updates_label = if updates > 0 {
            format!("Updates {updates}")
        } else {
            "Updates".to_string()
        };
        content = content.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(S3))
                .child(segments([
                    seg("store-seg-all", "All".into(), Segment::All, cx),
                    seg(
                        "store-seg-installed",
                        "Installed".into(),
                        Segment::Installed,
                        cx,
                    ),
                    seg("store-seg-updates", updates_label, Segment::Updates, cx),
                ]))
                .child(
                    div()
                        .flex_1()
                        .max_w(px(360.0))
                        .child(self.store_ui.search.clone()),
                ),
        );

        if shown.is_empty() {
            let words = match (picked, query.trim().is_empty()) {
                (_, false) => format!("No extensions match “{}”.", query.trim()),
                (Segment::Updates, true) => "Everything installed is up to date.".to_string(),
                (Segment::Installed, true) => {
                    "Nothing installed yet. Pick one from All to install it.".to_string()
                }
                (Segment::All, true) => {
                    "No extensions to show. Refresh to load the store.".to_string()
                }
            };
            content = content.child(empty(&words));
        } else {
            let mut list = card().p(px(0.0)).gap(px(0.0));
            for (n, &i) in shown.iter().enumerate() {
                if let Some(row) = all.get(i) {
                    list = list.child(self.store_row(row, n > 0, &frame.store, cx));
                }
            }
            content = content.child(list);
        }

        Page {
            head: Head {
                icon: "plug",
                title: "Extensions".to_string(),
                sub,
                crumbs: false,
                actions: vec![
                    button(
                        "store-refresh",
                        "Refresh",
                        Some("refresh"),
                        Variant::Default,
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.store_submit(StoreJob::Refresh { force: true }, UserAction::clicked());
                        cx.notify();
                    }))
                    .into_any_element(),
                ],
            },
            body: content,
            fill: false,
        }
    }

    fn icon_box(&mut self, r: Option<&FileRef>, size: f32, view: &StoreView) -> AnyElement {
        let pal = p();
        let image = r.and_then(|r| self.gpui_image(r, view));
        let frame = div()
            .size(px(size))
            .flex_shrink_0()
            .rounded(px(size * 0.22))
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center();
        match image {
            Some(image) => frame
                .child(
                    img(image)
                        .size(px(size))
                        .object_fit(ObjectFit::Contain)
                        .with_loading(move || {
                            dither_block(size, size, p().hairline_strong).into_any_element()
                        }),
                )
                .into_any_element(),
            None => frame
                .bg(pal.shell)
                .border_1()
                .border_color(pal.hairline)
                .child(icon("plug", size * 0.45, pal.text_2))
                .into_any_element(),
        }
    }

    fn store_row(
        &mut self,
        row: &Row,
        rule: bool,
        view: &StoreView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = p();
        let l = &row.listing;
        let author = (!l.author.is_empty()).then(|| format!("by {}", l.author));
        let job = view
            .jobs
            .get(&l.id)
            .filter(|j| j.error.is_none())
            .map(|j| job_words(j.kind));
        let opened = row.clone();
        div()
            .id(SharedString::from(format!("store-row-{}", l.id)))
            .px(px(S4))
            .py(px(S3))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(S3))
            .when(rule, |el| el.border_t_1().border_color(pal.hairline))
            .cursor_pointer()
            .hover(|s| s.bg(pal.hover))
            .on_click(cx.listener(move |this, _, _, cx| this.open_detail(&opened, cx)))
            .child(self.icon_box(l.icon.as_ref(), 40.0, view))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(S2))
                            .child(
                                div()
                                    .font(body(FontWeight::SEMIBOLD))
                                    .text_size(px(T_LEAD - 1.0))
                                    .text_color(pal.text)
                                    .whitespace_nowrap()
                                    .child(l.name.clone()),
                            )
                            .children(author.map(|a| {
                                div()
                                    .font(mono(FontWeight::NORMAL))
                                    .text_size(px(T_META))
                                    .text_color(pal.text_2)
                                    .whitespace_nowrap()
                                    .child(a)
                            })),
                    )
                    .child(
                        line(l.summary.clone(), FontWeight::NORMAL, T_META, pal.text_2).w_full(),
                    ),
            )
            .children(match job {
                Some(words) => Some(tag(words, TagKind::Accent, true)),
                None => row.state.tag().map(|(words, kind)| tag(words, kind, false)),
            })
            .child(icon("chevron-right", 15.0, pal.text_2))
            .into_any_element()
    }

    fn render_detail(&mut self, row: &Row, frame: &Frame, cx: &mut Context<Self>) -> Page {
        let pal = p();
        let l = &row.listing;
        let id = l.id.clone();
        let view = &frame.store;
        let ext = row
            .installed
            .as_ref()
            .and_then(|i| frame.ext.get(i.slot).cloned());

        let mut content = div().flex().flex_col().gap(px(S5));
        content = content.child(
            div().flex().flex_row().child(
                button(
                    "store-back",
                    "All extensions",
                    Some("chevron-left"),
                    Variant::Link,
                    true,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.store_ui.detail = None;
                    this.store_ui.confirm = None;
                    cx.notify();
                })),
            ),
        );

        // Who made it, how big, and what Samo can do with it.
        let mut facts: Vec<String> = Vec::new();
        match &row.installed {
            Some(i) if row.state == RowState::Update => {
                facts.push(format!("v{} installed", i.version));
                facts.push(format!("v{} available", l.version));
            }
            Some(i) => facts.push(format!("v{}", i.version)),
            None => facts.push(format!("v{}", l.version)),
        }
        if !l.author.is_empty() {
            facts.push(format!("by {}", l.author));
        }
        if row.in_store
            && let Some(size) = size_words(l.package.size)
        {
            facts.push(size);
        }
        if !l.categories.is_empty() {
            facts.push(l.categories.join(", "));
        }
        let trust = if row.installed.as_ref().is_some_and(|i| i.dev) {
            tag("dev", TagKind::Neutral, false)
        } else if row.in_store {
            tag("signed", TagKind::Accent, false)
        } else {
            tag("installed", TagKind::Neutral, false)
        };
        let homepage = l.homepage.clone();
        let top = card()
            .flex_row()
            .items_center()
            .gap(px(S4))
            .child(self.icon_box(l.icon.as_ref(), 72.0, view))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(S1))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(S2))
                            .child(h2(l.name.clone()))
                            .child(trust),
                    )
                    .child(mono_line(facts.join(" · "), T_META, pal.text_2).w_full())
                    .when(homepage.starts_with("https://"), |el| {
                        el.child(
                            div().flex().flex_row().child(
                                button(
                                    "store-homepage",
                                    "Homepage",
                                    Some("reveal"),
                                    Variant::Link,
                                    true,
                                )
                                .on_click(move |_, _, cx| cx.open_url(&homepage)),
                            ),
                        )
                    }),
            )
            .child(self.detail_actions(row, ext.as_ref(), view, cx));
        content = content.child(top);

        if let Some(confirm) = self.store_ui.confirm.clone() {
            content = content.child(self.confirm_panel(&confirm, row, cx));
        }
        if let Some(job) = view.jobs.get(&id)
            && let Some(error) = &job.error
        {
            content = content.child(self.job_error(&id, job, error, cx));
        }
        if let Some(e) = &ext {
            content = content.children(self.status_detail(e, cx));
            content = content.children(e.note.clone().map(|n| {
                div()
                    .font(body(FontWeight::NORMAL))
                    .text_size(px(T_META))
                    .text_color(pal.text_2)
                    .child(n)
            }));
        }

        // An update waits: what's new in it, and what it changes.
        let update = row.state == RowState::Update;
        if update {
            content = content.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(S3))
                    .child(section_rule(
                        &format!("In {}", l.version),
                        Some("update available".into()),
                    ))
                    .when(!l.whats_new.is_empty(), |el| {
                        el.child(paragraph(&l.whats_new))
                    }),
            );
        }

        if !l.screenshots.is_empty() {
            content = content.child(self.screenshots(row, view, cx));
        }

        let blocks = description_blocks(&l.description);
        if !blocks.is_empty() {
            content = content.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(S3))
                    .child(section_rule("About", None))
                    .children(blocks.into_iter().map(block)),
            );
        }

        // What it changes: the installed version's, or (when an update waits, or it isn't installed) the store's.
        let (changes, about): (&[Change], Option<String>) = match &row.installed {
            Some(i) if !update => (&i.changes, Some(format!("v{}", i.version))),
            _ => (&l.changes, Some(format!("v{}", l.version))),
        };
        content = content.child(
            div()
                .flex()
                .flex_col()
                .child(section_rule("What it changes on your Mac", about))
                .children(changes.iter().map(change_row)),
        );

        if !update && !l.whats_new.is_empty() {
            content = content.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(S3))
                    .child(section_rule("What's new", Some(format!("v{}", l.version))))
                    .child(paragraph(&l.whats_new)),
            );
        }

        let (status, sub) = match &ext {
            Some(e) => (Some(status_tag(&e.status)), l.summary.clone()),
            None => (None, l.summary.clone()),
        };
        Page {
            head: Head {
                icon: "plug",
                title: l.name.clone(),
                sub,
                crumbs: false,
                actions: status
                    .map(|(words, kind)| vec![tag(words, kind, true).into_any_element()])
                    .unwrap_or_default(),
            },
            body: content,
            fill: false,
        }
    }

    /// The detail page's buttons: one Primary action at most, Uninstall as Danger, and the switch once installed.
    fn detail_actions(
        &self,
        row: &Row,
        ext: Option<&ExtView>,
        view: &StoreView,
        cx: &mut Context<Self>,
    ) -> Div {
        let pal = p();
        let l = &row.listing;
        let id = l.id.clone();
        let mut actions = div()
            .flex()
            .flex_col()
            .items_end()
            .gap(px(S2))
            .flex_shrink_0();
        if let Some(job) = view.jobs.get(&id).filter(|j| j.error.is_none()) {
            return actions.child(busy_button(job_words(job.kind)));
        }
        let mut buttons = div().flex().flex_row().items_center().gap(px(S2));
        let install = |label: String, cx: &mut Context<Self>| {
            let id = id.clone();
            button(
                "store-install",
                label,
                Some("download"),
                Variant::Primary,
                false,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.store_ui.confirm = None;
                this.store_submit(StoreJob::Install(id.clone()), UserAction::clicked());
                cx.notify();
            }))
        };
        let uninstall = |cx: &mut Context<Self>| {
            let id = id.clone();
            button(
                "store-uninstall",
                "Uninstall",
                Some("trash"),
                Variant::Danger,
                false,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.store_ui.confirm = Some(StoreConfirm::Uninstall(id.clone()));
                cx.notify();
            }))
        };
        match (row.state, &row.installed) {
            (RowState::Available, _) => buttons = buttons.child(install("Install".into(), cx)),
            (RowState::NeedsNewerSamo, _) => {
                buttons = buttons.child(
                    div()
                        .font(body(FontWeight::NORMAL))
                        .text_size(px(T_META))
                        .text_color(pal.text_2)
                        .child(format!("Needs Samo {} or newer", l.min_samo)),
                )
            }
            (RowState::Update, _) => {
                buttons = buttons
                    .child(uninstall(cx))
                    .child(install(format!("Update to {}", l.version), cx))
            }
            (RowState::Dev, _) if row.in_store && can_run(l, SAMO_VERSION) => {
                let rid = id.clone();
                buttons = buttons.child(uninstall(cx)).child(
                    button(
                        "store-replace-dev",
                        "Install signed version",
                        Some("download"),
                        Variant::Primary,
                        false,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.store_ui.confirm = Some(StoreConfirm::ReplaceDev(rid.clone()));
                        cx.notify();
                    })),
                )
            }
            (_, Some(_)) => buttons = buttons.child(uninstall(cx)),
            (_, None) => {}
        }
        actions = actions.child(buttons);
        if let (Some(installed), Some(e)) = (&row.installed, ext) {
            actions = actions.child(self.switch_line(installed, e, cx));
        }
        actions
    }

    fn switch_line(&self, installed: &InstalledAs, e: &ExtView, cx: &mut Context<Self>) -> Div {
        let pal = p();
        let slot: ExtSlot = installed.slot;
        let next = !installed.on;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(S3))
            .when(e.on && e.status == Status::Running, |el| {
                el.child(
                    button(
                        SharedString::from(format!("ext-open-{}", slot.0)),
                        format!("Open {}", e.name),
                        Some("chevron-right"),
                        Variant::Link,
                        true,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_nav(crate::NavSection::Extension(slot), cx);
                    })),
                )
            })
            .child(
                div()
                    .font(body(FontWeight::NORMAL))
                    .text_size(px(T_META))
                    .text_color(pal.text_2)
                    .child(if installed.on { "On" } else { "Off" }),
            )
            .child(
                face_switch(
                    SharedString::from(format!("ext-on-{}", slot.0)),
                    installed.on,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(host) = this.hub.ext.get() {
                        host.set_on(slot, next);
                    }
                    cx.notify();
                })),
            )
    }

    fn confirm_panel(&self, confirm: &StoreConfirm, row: &Row, cx: &mut Context<Self>) -> Div {
        let pal = p();
        let name = row.listing.name.clone();
        let (question, detail, label, variant, job) = match confirm {
            StoreConfirm::ReplaceDev(id) => (
                "Replace the dev build with the signed one?".to_string(),
                format!(
                    "{name} keeps its data and its switches, and stays connected. From then on a dev folder won't \
                     replace it; uninstall it first to go back to the dev build."
                ),
                "Replace",
                Variant::Primary,
                StoreJob::Install(id.clone()),
            ),
            StoreConfirm::Uninstall(id) => {
                let changes = row
                    .installed
                    .as_ref()
                    .map(|i| i.changes.clone())
                    .unwrap_or_default();
                let kept = kept_backups(&changes);
                let keeps = if kept.is_empty() {
                    String::new()
                } else {
                    format!(" Samo keeps {}.", kept.join(" and "))
                };
                (
                    format!("Uninstall {name}?"),
                    format!(
                        "It first undoes what it changed on your Mac, then its folder and its data are deleted.{keeps}"
                    ),
                    "Uninstall",
                    Variant::Danger,
                    StoreJob::Uninstall {
                        id: id.clone(),
                        anyway: false,
                    },
                )
            }
        };
        let job = std::cell::RefCell::new(Some(job));
        card()
            .gap(px(S3))
            .border_color(pal.accent_line)
            .child(
                div()
                    .font(body(FontWeight::SEMIBOLD))
                    .text_size(px(T_BODY))
                    .child(question),
            )
            .child(paragraph(&detail))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(S2))
                    .child(
                        button("store-confirm", label, None, variant, false).on_click(cx.listener(
                            move |this, _, _, cx| {
                                if let Some(job) = job.borrow_mut().take() {
                                    this.store_ui.confirm = None;
                                    this.store_submit(job, UserAction::clicked());
                                }
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        button("store-cancel", "Cancel", None, Variant::Default, false).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.store_ui.confirm = None;
                                cx.notify();
                            }),
                        ),
                    ),
            )
    }

    fn job_error(&self, id: &str, job: &JobView, error: &str, cx: &mut Context<Self>) -> Div {
        let dismiss_id = id.to_string();
        let force_id = id.to_string();
        div()
            .flex()
            .flex_col()
            .gap(px(S2))
            .child(error_line(error))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(S2))
                    .when(job.can_force, |el| {
                        el.child(
                            button(
                                "store-force",
                                "Uninstall anyway",
                                Some("trash"),
                                Variant::Danger,
                                true,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.store_submit(
                                        StoreJob::Uninstall {
                                            id: force_id.clone(),
                                            anyway: true,
                                        },
                                        UserAction::clicked(),
                                    );
                                    cx.notify();
                                },
                            )),
                        )
                    })
                    .child(
                        button("store-dismiss", "Dismiss", None, Variant::Default, true).on_click(
                            cx.listener(move |this, _, _, cx| {
                                if let Some(store) = this.hub.store.get() {
                                    store.dismiss(&dismiss_id);
                                }
                                cx.notify();
                            }),
                        ),
                    ),
            )
    }

    /// One screenshot at a time, with ‹ › and "1 of 3". A dithered placeholder until it is verified and decoded.
    fn screenshots(&mut self, row: &Row, view: &StoreView, cx: &mut Context<Self>) -> Div {
        let pal = p();
        let shots = &row.listing.screenshots;
        let count = shots.len();
        let at = self.store_ui.shot.min(count.saturating_sub(1));
        let Some(shot) = shots.get(at) else {
            return div();
        };
        let failed = view
            .images
            .get(&shot.file.sha256)
            .and_then(|r| r.as_ref().err().cloned());
        let image = self.gpui_image(&shot.file, view);
        if image.is_some() {
            self.store_ui.shown_shots.insert(shot.file.sha256.clone());
        }
        let height = 380.0;
        let stage = div()
            .h(px(height))
            .w_full()
            .rounded(px(R_CARD))
            .overflow_hidden()
            .bg(pal.glass)
            .border_1()
            .border_color(pal.glass_line)
            .flex()
            .items_center()
            .justify_center();
        let stage = match (image, failed) {
            (Some(image), _) => stage.child(
                img(image)
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .with_loading(move || {
                        dither_block(160.0, 100.0, p().glass_line).into_any_element()
                    }),
            ),
            (None, Some(why)) => stage.child(
                div()
                    .px(px(S5))
                    .font(body(FontWeight::NORMAL))
                    .text_size(px(T_META))
                    .text_color(pal.glass_text_2)
                    .child(format!("This screenshot wasn't shown: {why}.")),
            ),
            (None, None) => stage.child(dither_block(160.0, 100.0, pal.glass_line)),
        };
        let step = |id: &'static str,
                    glyph: &'static str,
                    to: usize,
                    enabled: bool,
                    cx: &mut Context<Self>| {
            icon_button(
                id,
                glyph,
                if to < at { "Previous" } else { "Next" },
                Variant::Default,
                true,
            )
            .when(!enabled, |el| el.opacity(0.4))
            .when(enabled, |el| {
                el.on_click(cx.listener(move |this, _, _, cx| {
                    this.store_ui.shot = to;
                    cx.notify();
                }))
            })
        };
        div()
            .flex()
            .flex_col()
            .gap(px(S3))
            .child(section_rule(
                "Screenshots",
                Some(format!("{} of {count}", at + 1)),
            ))
            .child(stage)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(S2))
                    .child(step(
                        "store-shot-prev",
                        "chevron-left",
                        at.saturating_sub(1),
                        at > 0,
                        cx,
                    ))
                    .child(step(
                        "store-shot-next",
                        "chevron-right",
                        at + 1,
                        at + 1 < count,
                        cx,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .font(body(FontWeight::NORMAL))
                            .text_size(px(T_META))
                            .text_color(pal.text_2)
                            .child(shot.caption.clone()),
                    ),
            )
    }

    /// What a stopped or blocked extension needs from you, if anything.
    fn status_detail(&self, e: &ExtView, cx: &mut Context<Self>) -> Option<AnyElement> {
        let slot = e.slot;
        match &e.status {
            Status::Stopped(reason) => Some(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(S2))
                    .child(error_line(&format!("{reason}.")))
                    .child(
                        div().flex().flex_row().child(
                            button(
                                SharedString::from(format!("ext-restart-{}", slot.0)),
                                "Restart",
                                Some("refresh"),
                                Variant::Default,
                                true,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if let Some(host) = this.hub.ext.get() {
                                        host.restart(slot);
                                    }
                                    cx.notify();
                                },
                            )),
                        ),
                    )
                    .into_any_element(),
            ),
            Status::Blocked(reason) => Some(empty(&format!("{reason}.")).into_any_element()),
            Status::Starting | Status::Running | Status::Off => None,
        }
    }

    pub(crate) fn render_extension(
        &self,
        slot: ExtSlot,
        frame: &Frame,
        cx: &mut Context<Self>,
    ) -> Page {
        let Some(e) = frame.ext.get(slot) else {
            return Page {
                head: Head {
                    icon: "plug",
                    title: "Extension".to_string(),
                    sub: "This extension is no longer installed.".to_string(),
                    crumbs: false,
                    actions: Vec::new(),
                },
                body: empty("It may have been uninstalled. Extensions lists what is installed."),
                fill: false,
            };
        };
        let (status_words, status_kind) = status_tag(&e.status);
        let body = match &e.status {
            Status::Running if e.view.is_empty() => {
                div().child(loading(&format!("Waiting for {} to report…", e.name)))
            }
            Status::Running => self.render_ext_widgets(slot, &e.view, cx),
            Status::Starting => div().child(loading(&format!("Starting {}…", e.name))),
            Status::Off => div().child(empty(&format!(
                "{} is off. Turn it on in Extensions to see it here.",
                e.name
            ))),
            Status::Stopped(_) | Status::Blocked(_) => div()
                .flex()
                .flex_col()
                .gap(px(S3))
                .children(self.status_detail(e, cx)),
        };
        Page {
            head: Head {
                icon: "plug",
                title: e.name.clone(),
                sub: e.summary.clone(),
                crumbs: false,
                actions: vec![tag(status_words, status_kind, true).into_any_element()],
            },
            body,
            fill: false,
        }
    }
}

fn job_words(kind: JobKind) -> &'static str {
    match kind {
        JobKind::Installing => "Installing…",
        JobKind::Updating => "Updating…",
        JobKind::Uninstalling => "Uninstalling…",
    }
}

fn paragraph(text: &str) -> Div {
    div()
        .font(body(FontWeight::NORMAL))
        .text_size(px(T_BODY))
        .line_height(px(21.0))
        .text_color(p().text)
        .child(text.to_string())
}

/// A description block: a paragraph, or bullets.
pub(crate) fn block(b: Block) -> AnyElement {
    let pal = p();
    match b {
        Block::Para(text) => paragraph(&text).into_any_element(),
        Block::Bullets(items) => div()
            .flex()
            .flex_col()
            .gap(px(S1))
            .children(items.into_iter().map(|item| {
                div()
                    .flex()
                    .flex_row()
                    .gap(px(S2))
                    .child(
                        div()
                            .mt(px(8.0))
                            .size(px(5.0))
                            .flex_shrink_0()
                            .bg(pal.accent),
                    )
                    .child(paragraph(&item).flex_1())
            }))
            .into_any_element(),
    }
}

fn change_row(c: &Change) -> Div {
    let pal = p();
    div()
        .py(px(6.0))
        .border_b_1()
        .border_color(pal.hairline)
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .font(body(FontWeight::NORMAL))
                .text_size(px(T_BODY))
                .text_color(pal.text)
                .child(c.what.clone()),
        )
        .child(mono_line(format!("{} · {}", c.place, c.undo), T_META, pal.text_2).w_full())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_stopped_extension_is_signal() {
        for status in [
            Status::Running,
            Status::Starting,
            Status::Off,
            Status::Blocked("dev build".into()),
        ] {
            assert!(status_tag(&status).1 != TagKind::Signal, "{status:?}");
        }
        assert!(status_tag(&Status::Stopped("crashed".into())).1 == TagKind::Signal);
    }

    #[test]
    fn the_store_says_where_its_list_came_from() {
        let mut v = StoreView::default();
        assert!(
            store_note(&v).is_some_and(|(w, failed)| w.contains("hasn't been opened") && !failed)
        );
        v.fetched_at_ms = Some(chrono::Utc::now().timestamp_millis());
        assert!(store_note(&v).is_none());
        v.offline = Some("offline".into());
        assert!(
            store_note(&v).is_some_and(|(w, _)| w.starts_with("Offline: showing the store as of"))
        );
        v.fetched_at_ms = None;
        assert!(
            store_note(&v)
                .is_some_and(|(w, failed)| w.contains("installed extensions only") && failed)
        );
    }
}
