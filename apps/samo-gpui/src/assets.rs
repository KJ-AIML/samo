//! Embedded 16px stroke icons, served to `gpui::svg()` as `icons/<name>.svg`.

use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

pub struct Assets;

const HEAD: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#000" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">"##;

const ICONS: &[(&str, &str)] = &[
    (
        "gauge",
        r##"<path d="M2.25 11.5a5.75 5.75 0 1 1 11.5 0"/><path d="M8 11.5l2.75-3.5"/><circle cx="8" cy="11.5" r="1" fill="#000" stroke="none"/>"##,
    ),
    (
        "package",
        r##"<path d="M8 1.75l5.75 3.1v6.3L8 14.25l-5.75-3.1v-6.3z"/><path d="M2.5 5L8 8l5.5-3"/><path d="M8 8v6"/>"##,
    ),
    (
        "bubbles",
        r##"<circle cx="6" cy="9.5" r="4"/><circle cx="11.75" cy="5" r="2.5"/><circle cx="12.25" cy="11.75" r="1.5"/>"##,
    ),
    (
        "pulse",
        r##"<path d="M1.75 8.5h2.5l2-4.5 3.5 8 2-3.5h2.5"/>"##,
    ),
    (
        "chip",
        r##"<rect x="4" y="4" width="8" height="8" rx="1.5"/><rect x="6.5" y="6.5" width="3" height="3" rx=".5"/><path d="M6.5 1.75V4M9.5 1.75V4M6.5 12v2.25M9.5 12v2.25M1.75 6.5H4M1.75 9.5H4M12 6.5h2.25M12 9.5h2.25"/>"##,
    ),
    (
        "folder",
        r##"<path d="M1.75 4.25c0-.7.55-1.25 1.25-1.25h3l1.5 1.75H13c.7 0 1.25.55 1.25 1.25v6c0 .7-.55 1.25-1.25 1.25H3c-.7 0-1.25-.55-1.25-1.25z"/>"##,
    ),
    (
        "flask",
        r##"<path d="M6 2h4"/><path d="M6.75 2v4.1L3.1 12.3c-.5.85.1 1.7 1 1.7h7.8c.9 0 1.5-.85 1-1.7L9.25 6.1V2"/><path d="M4.75 10h6.5"/>"##,
    ),
    (
        "home",
        r##"<path d="M2.25 7.25L8 2.5l5.75 4.75"/><path d="M3.75 6v7.25h8.5V6"/><path d="M6.5 13.25V9.75h3v3.5"/>"##,
    ),
    (
        "download",
        r##"<path d="M8 2v7.5"/><path d="M4.75 6.5L8 9.75l3.25-3.25"/><path d="M2.25 10.5v2c0 .7.55 1.25 1.25 1.25h9c.7 0 1.25-.55 1.25-1.25v-2"/>"##,
    ),
    (
        "apps",
        r##"<rect x="2.25" y="2.25" width="4.5" height="4.5" rx="1.25"/><rect x="9.25" y="2.25" width="4.5" height="4.5" rx="1.25"/><rect x="2.25" y="9.25" width="4.5" height="4.5" rx="1.25"/><rect x="9.25" y="9.25" width="4.5" height="4.5" rx="1.25"/>"##,
    ),
    (
        "refresh",
        r##"<path d="M13.25 8A5.25 5.25 0 1 1 11.7 4.3"/><path d="M13.25 2.25v3h-3"/>"##,
    ),
    (
        "reveal",
        r##"<path d="M9.5 2.25h4.25V6.5"/><path d="M13.75 2.25L7.75 8.25"/><path d="M12 9.5v3c0 .7-.55 1.25-1.25 1.25H3.5c-.7 0-1.25-.55-1.25-1.25V5.25C2.25 4.55 2.8 4 3.5 4h3"/>"##,
    ),
    (
        "trash",
        r##"<path d="M2.5 4.25h11"/><path d="M6.25 4.25v-1.5c0-.4.35-.75.75-.75h2c.4 0 .75.35.75.75v1.5"/><path d="M3.75 4.25l.7 8.6c.06.66.6 1.15 1.25 1.15h4.6c.65 0 1.2-.5 1.25-1.15l.7-8.6"/>"##,
    ),
    (
        "arrow-up",
        r##"<path d="M8 13.25V3"/><path d="M4 7l4-4 4 4"/>"##,
    ),
    (
        "undo",
        r##"<path d="M5.5 3.5L2.5 6.5l3 3"/><path d="M2.75 6.5h6.75a3.5 3.5 0 0 1 0 7H7"/>"##,
    ),
    ("close", r##"<path d="M4 4l8 8M12 4l-8 8"/>"##),
    (
        "window",
        r##"<rect x="2" y="2.75" width="12" height="10.5" rx="1.75"/><path d="M2 6.25h12"/><circle cx="4.25" cy="4.5" r=".6" fill="#000" stroke="none"/>"##,
    ),
    (
        "desktop",
        r##"<rect x="1.75" y="2.5" width="12.5" height="8.75" rx="1.5"/><path d="M5.5 13.75h5M8 11.25v2.5"/>"##,
    ),
    (
        "document",
        r##"<path d="M4 1.75h5.25L12 4.5v9.75H4z"/><path d="M9 1.75v3h3"/><path d="M6 8.25h4M6 10.75h4"/>"##,
    ),
    (
        "file",
        r##"<path d="M4 1.75h5.25L12 4.5v9.75H4z"/><path d="M9 1.75v3h3"/>"##,
    ),
    ("plus", r##"<path d="M8 3v10M3 8h10"/>"##),
    (
        "power",
        r##"<path d="M8 2.25v5"/><path d="M4.6 4.4a5 5 0 1 0 6.8 0"/>"##,
    ),
    ("chevron-right", r##"<path d="M6 3.5L10.5 8 6 12.5"/>"##),
    ("chevron-down", r##"<path d="M4 6l4 4 4-4"/>"##),
    ("check", r##"<path d="M3 8.5l3.25 3.25L13 4.75"/>"##),
    (
        "alert",
        r##"<path d="M7.13 2.75a1 1 0 0 1 1.74 0l5.2 9.25a1 1 0 0 1-.87 1.5H2.8a1 1 0 0 1-.87-1.5z"/><path d="M8 6.25v3"/><circle cx="8" cy="11.25" r=".75" fill="#000" stroke="none"/>"##,
    ),
    (
        "shield",
        r##"<path d="M8 1.75l5 1.9v4.1c0 3.1-2.1 5.4-5 6.5-2.9-1.1-5-3.4-5-6.5v-4.1z"/><path d="M5.75 8l1.6 1.6 2.9-3.1"/>"##,
    ),
    (
        "plug",
        r##"<path d="M6 1.75v3M10 1.75v3"/><path d="M3.75 4.75h8.5v2.5a4.25 4.25 0 0 1-8.5 0z"/><path d="M8 11.5v2.75"/>"##,
    ),
];

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let Some(name) = path
            .strip_prefix("icons/")
            .and_then(|p| p.strip_suffix(".svg"))
        else {
            return Ok(None);
        };
        Ok(ICONS
            .iter()
            .find(|(icon, _)| *icon == name)
            .map(|(_, body)| Cow::Owned(format!("{HEAD}{body}</svg>").into_bytes())))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        if path.trim_end_matches('/') != "icons" {
            return Ok(Vec::new());
        }
        Ok(ICONS
            .iter()
            .map(|(name, _)| SharedString::from(format!("icons/{name}.svg")))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_loads_as_well_formed_svg() {
        for (name, _) in ICONS {
            let loaded = Assets.load(&format!("icons/{name}.svg"));
            assert!(
                matches!(&loaded, Ok(Some(_))),
                "icon `{name}` failed to load"
            );
            if let Ok(Some(bytes)) = loaded {
                let svg = String::from_utf8_lossy(&bytes);
                assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
            }
        }
    }

    #[test]
    fn unknown_paths_are_absent() {
        assert!(matches!(Assets.load("icons/nope.svg"), Ok(None)));
        assert!(matches!(Assets.load("fonts/x.ttf"), Ok(None)));
    }
}
