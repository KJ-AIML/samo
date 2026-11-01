//! Settings (ADR-003 M5). For now it holds **Updates**: this Samo's version, the latest one in the signed catalog,
//! and **Install and restart**.
//!
//! This is one of the two files allowed to make a `UserAction` (R12, the `network` gate): only in what a click
//! runs. Shown at launch (`SAMO_START_PAGE=settings`), it uses the store's cache and never goes online.

use crate::components::*;
use crate::shell::{Head, Page};
use crate::store_view::{SAMO_VERSION, Segment, update_count};
use crate::theme::*;
use crate::{Frame, NavSection, SamoWorkspaceApp};
use gpui::{Context, Div, FontWeight, IntoElement, ParentElement, Styled, div, prelude::*, px};
use samo_domain::store::{SamoRelease, newer};
use samo_extensions::net::UserAction;
use samo_extensions::update::can_update;
use samo_extensions::worker::{SamoUpdate, StoreJob, StoreView};

/// Where Samo's releases are, for when it can't update itself.
pub const RELEASES_URL: &str = "https://github.com/KJ-AIML/samo/releases";

/// What Updates' main button does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateAction {
    /// Nothing to install.
    None,
    Install {
        version: String,
    },
    /// A newer Samo exists, but this one can't replace itself; the words say why.
    Disabled(String),
    /// Working; the words say on what.
    Busy(String),
}

/// What Updates says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateWords {
    pub headline: String,
    /// This Samo's version, and when the store was last checked (or that it couldn't be).
    pub status: String,
    pub action: UpdateAction,
    pub error: Option<String>,
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

pub fn update_words(
    current: &str,
    latest: Option<&SamoRelease>,
    fetched_at_ms: Option<i64>,
    offline: Option<&str>,
    can: Result<(), String>,
    job: &SamoUpdate,
) -> UpdateWords {
    let waiting = latest.filter(|l| newer(&l.version, current));
    let headline = match (waiting, fetched_at_ms) {
        (Some(l), _) => format!("Samo {} is available", l.version),
        (None, Some(_)) => "You have the latest Samo".to_string(),
        (None, None) => "Samo hasn't checked for updates yet".to_string(),
    };
    let status = match (fetched_at_ms, offline) {
        (Some(at), Some(_)) => format!("Samo {current} · offline, as of {}", clock(at)),
        (None, Some(_)) => format!("Samo {current} · couldn't reach the store"),
        (Some(at), None) => format!("Samo {current} · checked {}", clock(at)),
        (None, None) => format!("Samo {current}"),
    };
    let mut error = None;
    let action = match (job, waiting) {
        (SamoUpdate::Preparing, Some(l)) => {
            UpdateAction::Busy(format!("Downloading Samo {}…", l.version))
        }
        (SamoUpdate::Ready(_), _) => UpdateAction::Busy("Restarting…".to_string()),
        (_, None) => UpdateAction::None,
        (other, Some(l)) => {
            if let SamoUpdate::Failed(why) = other {
                error = Some(format!("{why}."));
            }
            match can {
                Ok(()) => UpdateAction::Install {
                    version: l.version.clone(),
                },
                Err(why) => UpdateAction::Disabled(why),
            }
        }
    };
    UpdateWords {
        headline,
        status,
        action,
        error,
    }
}

impl SamoWorkspaceApp {
    fn settings_submit(&self, job: StoreJob, action: UserAction) {
        if let Some(store) = self.hub.store.get() {
            store.submit(job, action);
        }
    }

    /// The sidebar's or rail's Settings, clicked: the store's catalog is fetched if the cached one is over 10
    /// minutes old. Only click handlers call this.
    pub(crate) fn settings_opened_by_click(&mut self, cx: &mut Context<Self>) {
        self.set_nav(NavSection::Settings, cx);
        self.settings_submit(StoreJob::Refresh { force: false }, UserAction::clicked());
    }

    pub(crate) fn render_settings(&mut self, frame: &Frame, cx: &mut Context<Self>) -> Page {
        let view: &StoreView = &frame.store;
        let exe = std::env::current_exe().ok();
        let can = match exe.as_deref().map(can_update) {
            Some(Ok(_)) => Ok(()),
            Some(Err(why)) => Err(why.to_string()),
            None => Err("Samo can't tell where it is running from".to_string()),
        };
        let words = update_words(
            SAMO_VERSION,
            view.catalog.as_ref().and_then(|c| c.samo.as_ref()),
            view.fetched_at_ms,
            view.offline.as_deref(),
            can,
            &view.samo,
        );
        let whats_new = view
            .catalog
            .as_ref()
            .and_then(|c| c.samo.clone())
            .filter(|l| newer(&l.version, SAMO_VERSION))
            .map(|l| l.whats_new)
            .filter(|w| !w.trim().is_empty());
        let updates = update_count(view.catalog.as_deref(), &frame.ext, SAMO_VERSION);

        let body = div()
            .flex()
            .flex_col()
            .gap(px(S4))
            .child(section_rule("Updates", None))
            .child(self.updates_card(&words, whats_new, view.refreshing, exe, cx))
            .when(updates > 0, |el| {
                let label = if updates == 1 {
                    "1 extension has an update".to_string()
                } else {
                    format!("{updates} extensions have updates")
                };
                el.child(
                    div().flex().flex_row().child(
                        button(
                            "settings-ext-updates",
                            label,
                            Some("chevron-right"),
                            Variant::Link,
                            true,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.store_ui.segment = Segment::Updates;
                            this.store_opened_by_click(cx);
                        })),
                    ),
                )
            });

        Page {
            head: Head {
                icon: "gear",
                title: "Settings".to_string(),
                sub: "Samo checks for updates only when you open this page or the store."
                    .to_string(),
                crumbs: false,
                actions: vec![
                    button(
                        "settings-check",
                        "Check now",
                        Some("refresh"),
                        Variant::Default,
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.settings_submit(
                            StoreJob::Refresh { force: true },
                            UserAction::clicked(),
                        );
                        cx.notify();
                    }))
                    .into_any_element(),
                ],
            },
            body,
            fill: false,
        }
    }

    fn updates_card(
        &self,
        words: &UpdateWords,
        whats_new: Option<String>,
        checking: bool,
        exe: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) -> Div {
        let pal = p();
        let action: gpui::AnyElement = match &words.action {
            UpdateAction::None => div().into_any_element(),
            UpdateAction::Busy(label) => busy_button(label).into_any_element(),
            UpdateAction::Install { version } => {
                let label = format!("Install {version} and restart");
                button(
                    "settings-install",
                    label,
                    Some("download"),
                    Variant::Primary,
                    false,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(current_exe) = exe.clone() {
                        this.settings_submit(
                            StoreJob::UpdateSamo { current_exe },
                            UserAction::clicked(),
                        );
                    }
                    cx.notify();
                }))
                .into_any_element()
            }
            UpdateAction::Disabled(_) => button(
                "settings-releases",
                "Release page",
                Some("reveal"),
                Variant::Default,
                false,
            )
            .on_click(|_, _, cx| cx.open_url(RELEASES_URL))
            .into_any_element(),
        };
        card()
            .gap(px(S3))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(S4))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(S1))
                            .child(h2(words.headline.clone()))
                            .child(
                                div()
                                    .font(mono(FontWeight::NORMAL))
                                    .text_size(px(T_META))
                                    .text_color(pal.text_2)
                                    .child(if checking {
                                        "Checking the store…".to_string()
                                    } else {
                                        words.status.clone()
                                    }),
                            ),
                    )
                    .child(action),
            )
            .children(match &words.action {
                UpdateAction::Disabled(why) => Some(
                    div()
                        .font(body(FontWeight::NORMAL))
                        .text_size(px(T_META))
                        .text_color(pal.text_2)
                        .child(format!("{why}. Download it from the release page instead.")),
                ),
                _ => None,
            })
            .children(words.error.clone().map(|e| error_line(&e)))
            .children(whats_new.map(|w| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(S2))
                    .child(mono_label("What's new", pal.text_2))
                    .children(
                        crate::store_view::description_blocks(&w)
                            .into_iter()
                            .map(crate::extensions_page::block),
                    )
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use samo_domain::store::FileRef;

    fn release(version: &str) -> SamoRelease {
        SamoRelease {
            version: version.into(),
            whats_new: "- Faster".into(),
            min_macos: "11.0".into(),
            package: FileRef {
                file: format!("Samo-{version}-macos-arm64.zip"),
                size: Some(1),
                sha256: "0".repeat(64),
                width: None,
                height: None,
            },
        }
    }

    #[test]
    fn update_state_words() {
        let now = chrono::Utc::now().timestamp_millis();
        let idle = SamoUpdate::Idle;

        // Never checked: no headline about versions, no button.
        let w = update_words("0.2.0", None, None, None, Ok(()), &idle);
        assert_eq!(w.headline, "Samo hasn't checked for updates yet");
        assert_eq!(
            (w.status.as_str(), &w.action),
            ("Samo 0.2.0", &UpdateAction::None)
        );

        // Current.
        let w = update_words(
            "0.2.0",
            Some(&release("0.2.0")),
            Some(now),
            None,
            Ok(()),
            &idle,
        );
        assert_eq!(w.headline, "You have the latest Samo");
        assert!(
            w.status.starts_with("Samo 0.2.0 · checked "),
            "{}",
            w.status
        );
        assert_eq!(w.action, UpdateAction::None);

        // A newer one: Install and restart, unless this Samo can't replace itself.
        let w = update_words(
            "0.2.0",
            Some(&release("0.3.0")),
            Some(now),
            None,
            Ok(()),
            &idle,
        );
        assert_eq!(w.headline, "Samo 0.3.0 is available");
        assert_eq!(
            w.action,
            UpdateAction::Install {
                version: "0.3.0".into()
            }
        );
        let w = update_words(
            "0.2.0",
            Some(&release("0.3.0")),
            Some(now),
            None,
            Err("not Samo.app".into()),
            &idle,
        );
        assert_eq!(w.action, UpdateAction::Disabled("not Samo.app".into()));

        // Working, then failed (said, with the button back).
        let w = update_words(
            "0.2.0",
            Some(&release("0.3.0")),
            Some(now),
            None,
            Ok(()),
            &SamoUpdate::Preparing,
        );
        assert_eq!(
            w.action,
            UpdateAction::Busy("Downloading Samo 0.3.0…".into())
        );
        let failed =
            SamoUpdate::Failed("Couldn't get Samo 0.3.0: the store stopped answering".into());
        let w = update_words(
            "0.2.0",
            Some(&release("0.3.0")),
            Some(now),
            None,
            Ok(()),
            &failed,
        );
        assert!(
            w.error
                .as_deref()
                .is_some_and(|e| e.ends_with("answering."))
        );
        assert!(matches!(w.action, UpdateAction::Install { .. }));

        // Offline: what Samo knew, and when.
        let w = update_words(
            "0.2.0",
            Some(&release("0.2.0")),
            Some(now),
            Some("offline"),
            Ok(()),
            &idle,
        );
        assert!(w.status.contains("offline, as of"), "{}", w.status);
        let w = update_words("0.2.0", None, None, Some("offline"), Ok(()), &idle);
        assert!(
            w.status.contains("couldn't reach the store"),
            "{}",
            w.status
        );
    }
}
