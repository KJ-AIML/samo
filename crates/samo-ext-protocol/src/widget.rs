use samo_primitives::Reading;
use serde::{Deserialize, Serialize};

/// What an extension's page is built from. Samo draws each with its own components, so every extension looks
/// like Samo: there is no custom drawing, no Primary or Danger button (one Ink action per view stays Samo's), and
/// no dither (dither means "Samo suggests").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "widget", rename_all = "snake_case")]
pub enum Widget {
    /// A rule with a label in capitals that splits the page: `SESSIONS · LIVE`.
    Section {
        title: String,
        #[serde(default)]
        about: Option<String>,
    },
    /// A big figure in dot-matrix digits. Unknown is drawn `--`, never `0`.
    Figure {
        label: String,
        value: Reading<f64>,
        #[serde(default)]
        unit: String,
        #[serde(default)]
        note: Option<String>,
    },
    /// A share of a whole, `0.0..=1.0`. Unknown lights nothing.
    Meter {
        label: String,
        fraction: Reading<f64>,
        #[serde(default)]
        note: Option<String>,
    },
    Rows {
        items: Vec<RowItem>,
    },
    /// A switch declared in the manifest's `[[settings]]`; the helper says whether it is on.
    Switch {
        key: String,
        label: String,
        on: bool,
        #[serde(default)]
        note: Option<String>,
    },
    Button {
        action: String,
        label: String,
        #[serde(default)]
        kind: ButtonKind,
    },
    Note {
        text: String,
        #[serde(default)]
        kind: NoteKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowItem {
    pub lamp: Lamp,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    /// A short figure at the row's end, in mono.
    #[serde(default)]
    pub trailing: String,
    #[serde(default)]
    pub tag: Option<String>,
}

/// A row's lamp. Only `Error` is drawn in Signal: waiting for you isn't wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lamp {
    Working,
    NeedsYou,
    Done,
    Off,
    Error,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonKind {
    #[default]
    Default,
    Link,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    #[default]
    Plain,
    Error,
}

/// How many rows a view holds, counting each widget as one and each item of a `Rows` as one (R8 caps this).
pub fn rows_in(view: &[Widget]) -> usize {
    view.iter()
        .map(|w| match w {
            Widget::Rows { items } => items.len().max(1),
            _ => 1,
        })
        .sum()
}
