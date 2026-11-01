//! `samo-ext-hello`: Samo's example extension. It uses every widget of protocol v1, keeps a badge while its
//! switch is on, and pops up when you press Ping. Copy it to start a new extension.
//!
//! A helper is a program Samo runs as `<helper> serve`. It reads one JSON message per line on stdin, writes one
//! per line on stdout, says `hello` first, and exits when it reads `shutdown`. Samo also runs `<helper> on`,
//! `off` and `uninstall` to undo or restore whatever the extension changes outside its own folder.

use samo_ext_protocol::{
    BadgeMeter, ButtonKind, HelperMsg, HostMsg, Lamp, NoteKind, PROTOCOL_VERSION, PopupKind,
    RowItem, Widget, decode_host_line, encode,
};
use samo_primitives::{Reading, UnsupportedReason};
use std::io::{self, BufRead, Write};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> ExitCode {
    let verb = std::env::args().nth(1).unwrap_or_default();
    match verb.as_str() {
        "serve" => serve(),
        // Hello changes nothing outside its own folder, so there is nothing to undo or restore.
        "on" | "off" | "uninstall" => {
            eprintln!("hello: nothing to {verb}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: samo-ext-hello serve|on|off|uninstall");
            ExitCode::from(2)
        }
    }
}

struct State {
    greet: bool,
    pings: u32,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn send(out: &mut impl Write, msg: &HelperMsg) -> io::Result<()> {
    let line = encode(msg).map_err(io::Error::other)?;
    writeln!(out, "{line}")?;
    out.flush()
}

fn view(state: &State) -> Vec<Widget> {
    let now = now_ms();
    vec![
        Widget::Section {
            title: "HELLO · EXAMPLE".into(),
            about: Some("protocol v1".into()),
        },
        Widget::Figure {
            label: "Pings".into(),
            value: Reading::available(f64::from(state.pings), now),
            unit: "pings".into(),
            note: None,
        },
        Widget::Figure {
            label: "Unknown, on purpose".into(),
            value: Reading::unsupported(UnsupportedReason::NotReported),
            unit: String::new(),
            note: Some("Unknown is drawn as --, never 0".into()),
        },
        Widget::Meter {
            label: "Half".into(),
            fraction: Reading::available(0.5, now),
            note: Some("A share of a whole".into()),
        },
        Widget::Section {
            title: "LAMPS".into(),
            about: None,
        },
        Widget::Rows {
            items: vec![
                row(Lamp::Working, "Something working", "since 2 min"),
                row(Lamp::NeedsYou, "Something waiting for you", "a question"),
                row(Lamp::Done, "Something done", "at 14:02"),
                row(Lamp::Off, "Something off", ""),
                row(Lamp::Error, "Something wrong", "stopped: an error"),
            ],
        },
        Widget::Section {
            title: "TRY IT".into(),
            about: None,
        },
        Widget::Switch {
            key: "greet".into(),
            label: "Wave from the menu bar".into(),
            on: state.greet,
            note: Some("While on, Hello keeps a badge beside Samo's menu bar figure".into()),
        },
        Widget::Button {
            action: "ping".into(),
            label: "Ping".into(),
            kind: ButtonKind::Default,
        },
        Widget::Note {
            text: "This is Samo's example extension. It changes nothing outside its own folder."
                .into(),
            kind: NoteKind::Plain,
        },
    ]
}

fn row(lamp: Lamp, title: &str, detail: &str) -> RowItem {
    RowItem {
        lamp,
        title: title.into(),
        detail: detail.into(),
        trailing: String::new(),
        tag: None,
    }
}

/// The badge, with a small load bar on the quick panel's line (half, to show what one looks like).
fn badge(state: &State) -> HelperMsg {
    HelperMsg::Badge {
        count: u32::from(state.greet),
        summary: if state.greet {
            "Hello · 1 waiting".into()
        } else {
            String::new()
        },
        meter: Some(BadgeMeter {
            label: "half".into(),
            fraction: Reading::available(0.5, now_ms()),
        }),
    }
}

fn serve() -> ExitCode {
    let mut out = io::stdout().lock();
    let mut state = State {
        greet: true,
        pings: 0,
    };
    let started = send(
        &mut out,
        &HelperMsg::Hello {
            protocol: PROTOCOL_VERSION,
        },
    )
    .and_then(|()| {
        send(
            &mut out,
            &HelperMsg::View {
                sections: view(&state),
            },
        )
    })
    .and_then(|()| send(&mut out, &badge(&state)));
    if started.is_err() {
        return ExitCode::FAILURE;
    }

    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let sent = match decode_host_line(&line) {
            Ok(HostMsg::Setting { key, value }) if key == "greet" => {
                state.greet = value;
                send(
                    &mut out,
                    &HelperMsg::View {
                        sections: view(&state),
                    },
                )
                .and_then(|()| send(&mut out, &badge(&state)))
            }
            Ok(HostMsg::Action { id }) if id == "ping" => {
                state.pings += 1;
                send(
                    &mut out,
                    &HelperMsg::View {
                        sections: view(&state),
                    },
                )
                .and_then(|()| {
                    send(
                        &mut out,
                        &HelperMsg::Popup {
                            key: format!("ping-{}", state.pings),
                            kind: PopupKind::Info,
                            title: "Hello".into(),
                            body: format!("Ping number {}", state.pings),
                        },
                    )
                })
            }
            Ok(HostMsg::Shutdown) => break,
            // Hello, page visibility, unknown switches and actions: nothing to do.
            Ok(_) | Err(_) => Ok(()),
        };
        if sent.is_err() {
            break;
        }
    }
    ExitCode::SUCCESS
}
