//! Extensions (ADR-002): the list of what is installed, with a switch each, and each extension's own page.
//!
//! Opening these pages never goes online. In this build extensions come from a developer's folder
//! (`SAMO_EXT_DEV_DIR`); finding and downloading them from Samo's catalog comes later, behind a button.

use crate::components::*;
use crate::shell::{Head, Page};
use crate::theme::*;
use crate::{Frame, SamoWorkspaceApp};
use gpui::{
    AnyElement, Context, Div, FontWeight, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::*, px,
};
use samo_extensions::{ExtSlot, ExtView, Status};

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

impl SamoWorkspaceApp {
    pub(crate) fn render_extensions(&self, frame: &Frame, cx: &mut Context<Self>) -> Page {
        let pal = p();
        let ext = &frame.ext;
        let on = ext.extensions.iter().filter(|e| e.on).count();
        let sub = match (ext.extensions.len(), on) {
            (0, _) => {
                "Extensions add to what Samo watches. You choose which to install.".to_string()
            }
            (n, on) => {
                format!("{n} installed, {on} on. Each runs on its own and can be turned off.")
            }
        };

        let mut content = div().flex().flex_col().gap(px(S4));
        for problem in &ext.problems {
            content = content.child(error_line(&format!(
                "{problem}. Samo skipped it; fix it or remove it from the Extensions folder."
            )));
        }
        if ext.extensions.is_empty() {
            content = content.child(empty(
                "No extensions yet. Extensions you install appear here, each with a switch. \
                 To try one you built, start Samo with SAMO_EXT_DEV_DIR pointing at its folder.",
            ));
        }
        for e in &ext.extensions {
            content = content.child(self.extension_card(e, cx));
        }
        content = content.child(
            div()
                .font(body(FontWeight::NORMAL))
                .text_size(px(T_META))
                .text_color(pal.text_2)
                .child(
                    "Turning an extension off stops it and undoes what it changed outside its own folder; \
                     turning it on puts that back.",
                ),
        );

        Page {
            head: Head {
                icon: "plug",
                title: "Extensions".to_string(),
                sub,
                crumbs: false,
                actions: Vec::new(),
            },
            body: content,
            fill: false,
        }
    }

    fn extension_card(&self, e: &ExtView, cx: &mut Context<Self>) -> Div {
        let pal = p();
        let slot = e.slot;
        let next = !e.on;
        let (status_words, status_kind) = status_tag(&e.status);
        let open_id = SharedString::from(format!("ext-open-{}", slot.0));

        let head = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(S3))
            .child(icon("plug", 18.0, pal.text_2))
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
                            .child(h2(e.name.clone()))
                            .child(
                                div()
                                    .font(mono(FontWeight::NORMAL))
                                    .text_size(px(T_META))
                                    .text_color(pal.text_2)
                                    .child(e.version.clone()),
                            )
                            .when(e.dev, |el| el.child(tag("dev", TagKind::Neutral, false))),
                    )
                    .child(
                        line(e.summary.clone(), FontWeight::NORMAL, T_META, pal.text_2).w_full(),
                    ),
            )
            .child(tag(status_words, status_kind, true))
            .child(
                face_switch(SharedString::from(format!("ext-on-{}", slot.0)), e.on).on_click(
                    cx.listener(move |this, _, _, cx| {
                        if let Some(host) = this.hub.ext.get() {
                            host.set_on(slot, next);
                        }
                        cx.notify();
                    }),
                ),
            );

        let changes = div()
            .flex()
            .flex_col()
            .child(mono_label("What it changes on your Mac", pal.text_2))
            .children(e.changes.iter().map(|c| {
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
                    .child(
                        mono_line(format!("{} · {}", c.place, c.undo), T_META, pal.text_2).w_full(),
                    )
            }));

        card()
            .gap(px(S4))
            .child(head)
            .children(self.status_detail(e, cx))
            .children(e.note.clone().map(|n| {
                div()
                    .font(body(FontWeight::NORMAL))
                    .text_size(px(T_META))
                    .text_color(pal.text_2)
                    .child(n)
            }))
            .child(changes)
            .when(e.on && e.status == Status::Running, |el| {
                el.child(
                    div().flex().flex_row().child(
                        button(
                            open_id,
                            format!("Open {}", e.name),
                            Some("chevron-right"),
                            Variant::Default,
                            true,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_nav(crate::NavSection::Extension(slot), cx);
                        })),
                    ),
                )
            })
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
                body: empty(
                    "It may have been removed. The Extensions page lists what is installed.",
                ),
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
}
