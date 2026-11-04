//! The list at Clips' shortcut on Windows (ADR-005 C2): a small window at the pointer, over everything, with a search
//! field and your copies, pins first. Its keys are the list model's; Windows turns only those into commands (an
//! accelerator table), so every other key types into the search, IME included. A pick closes it and pastes where you
//! were; Esc, Alt+F4 or a click elsewhere closes it without pasting. It runs on its own thread, with its own
//! message loop, and asks Clips for everything over the serving loop's channel.

use crate::clips::ListAsk;
use crate::history::{Item, Kind};
use crate::list_model::{Key, ListModel, Mods, Outcome};
use crate::page::{self, ShortcutState};
use crate::paste::{Outcome as Pasted, PasteFlow};
use crate::serve::Event;
use crate::settings::Shortcut;
use crate::win::hotkey;
use crate::win::paste::WinWindows;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{self, Sender, SyncSender};
use std::thread;
use std::time::Duration;
use winsafe::{self as w, co, gui, prelude::*};

const WIDTH: i32 = 480;
const SEARCH: i32 = 28;
const PAD: i32 = 6;
const LIST: i32 = 300;
const TITLE_COLUMN: i32 = 350;
/// How long the list waits for Clips' answer.
const ANSWER: Duration = Duration::from_secs(2);
/// Characters of a row's title.
const TITLE_CHARS: usize = 60;

// The list's own keys, as commands from the accelerator table.
const UP: u16 = 1001;
const DOWN: u16 = 1002;
const ENTER: u16 = 1003;
const PLAIN: u16 = 1004;
const ESCAPE: u16 = 1005;
const PIN: u16 = 1006;
const DELETE: u16 = 1007;
const BACKSPACE: u16 = 1008;
/// `DIGIT + n` is Ctrl+n, for n in 1–9.
const DIGIT: u16 = 1010;

const DIGITS: [co::VK; 9] = [
    co::VK::CHAR_1,
    co::VK::CHAR_2,
    co::VK::CHAR_3,
    co::VK::CHAR_4,
    co::VK::CHAR_5,
    co::VK::CHAR_6,
    co::VK::CHAR_7,
    co::VK::CHAR_8,
    co::VK::CHAR_9,
];

/// The keys Windows hands the list as commands, with what the model calls them.
fn keys() -> Vec<(co::ACCELF, co::VK, u16, Key, Mods)> {
    let plain = Mods::default();
    let alt = Mods {
        alt: true,
        ..Mods::default()
    };
    let ctrl = Mods {
        ctrl: true,
        ..Mods::default()
    };
    let mut keys = vec![
        (co::ACCELF::VIRTKEY, co::VK::UP, UP, Key::Up, plain),
        (co::ACCELF::VIRTKEY, co::VK::DOWN, DOWN, Key::Down, plain),
        (
            co::ACCELF::VIRTKEY,
            co::VK::RETURN,
            ENTER,
            Key::Enter,
            plain,
        ),
        (
            co::ACCELF::VIRTKEY | co::ACCELF::ALT,
            co::VK::RETURN,
            PLAIN,
            Key::Enter,
            alt,
        ),
        (
            co::ACCELF::VIRTKEY,
            co::VK::ESCAPE,
            ESCAPE,
            Key::Escape,
            plain,
        ),
        (
            co::ACCELF::VIRTKEY | co::ACCELF::ALT,
            co::VK::CHAR_P,
            PIN,
            Key::P,
            alt,
        ),
        (
            co::ACCELF::VIRTKEY | co::ACCELF::ALT,
            co::VK::DELETE,
            DELETE,
            Key::Delete,
            alt,
        ),
        (
            co::ACCELF::VIRTKEY | co::ACCELF::ALT,
            co::VK::BACK,
            BACKSPACE,
            Key::Backspace,
            alt,
        ),
    ];
    for (n, vk) in (1u8..).zip(DIGITS) {
        keys.push((
            co::ACCELF::VIRTKEY | co::ACCELF::CONTROL,
            vk,
            DIGIT + u16::from(n),
            Key::Digit(n),
            ctrl,
        ));
    }
    keys
}

struct State {
    model: ListModel,
    items: Vec<Item>,
    flow: PasteFlow<w::HWND>,
    open: bool,
}

#[derive(Clone)]
struct List {
    wnd: gui::WindowMain,
    search: gui::Edit,
    rows: gui::ListView,
    clips: Sender<Event>,
    state: Rc<RefCell<State>>,
    /// Where to hand the window once it is made (for the shortcut bridge).
    made: Rc<RefCell<Option<Sender<gui::WindowMain>>>>,
}

/// What a row says beside its title: its Ctrl+number first (the part you use), then its kind, and pinned.
fn info(item: &Item, row: usize) -> String {
    let kind = match item.kind {
        Kind::Text => "Text",
        Kind::Rich => "Rich text",
        Kind::Image => "Image",
        Kind::Files => "Files",
    };
    let mut words = Vec::new();
    if row < 9 {
        words.push(format!("Ctrl+{}", row + 1));
    }
    words.push(kind.to_string());
    if item.pinned {
        words.push("Pinned".to_string());
    }
    words.join(" · ")
}

impl List {
    fn new(
        clips: Sender<Event>,
        shortcut: Option<Shortcut>,
        made: Sender<gui::WindowMain>,
    ) -> Self {
        let accel: Vec<w::ACCEL> = keys()
            .iter()
            .map(|(flags, vk, cmd, _, _)| w::ACCEL {
                fVirt: *flags,
                key: *vk,
                cmd: *cmd,
            })
            .collect();
        let (width, height) = gui::dpi(WIDTH, SEARCH + LIST + 3 * PAD);
        let wnd = gui::WindowMain::new(gui::WindowMainOpts {
            class_name: "SamoClipsList",
            title: "Clips",
            size: (width, height),
            style: co::WS::POPUP | co::WS::BORDER | co::WS::CLIPCHILDREN,
            ex_style: co::WS_EX::TOPMOST | co::WS_EX::TOOLWINDOW,
            accel_table: w::HACCEL::CreateAcceleratorTable(&accel).ok(),
            process_dlg_msgs: false,
            ..Default::default()
        });
        let search = gui::Edit::new(
            &wnd,
            gui::EditOpts {
                position: gui::dpi(PAD, PAD),
                width: gui::dpi_x(WIDTH - 2 * PAD),
                height: gui::dpi_y(SEARCH - 4),
                ..Default::default()
            },
        );
        let title_column = gui::dpi_x(TITLE_COLUMN);
        let info_column = gui::dpi_x(WIDTH - TITLE_COLUMN - 2 * PAD - 24);
        let rows = gui::ListView::new(
            &wnd,
            gui::ListViewOpts {
                position: gui::dpi(PAD, SEARCH + 2 * PAD),
                size: gui::dpi(WIDTH - 2 * PAD, LIST),
                columns: &[("", title_column), ("", info_column)],
                control_style: co::LVS::REPORT
                    | co::LVS::NOCOLUMNHEADER
                    | co::LVS::SINGLESEL
                    | co::LVS::SHOWSELALWAYS,
                ..Default::default()
            },
        );
        let list = Self {
            wnd,
            search,
            rows,
            clips,
            state: Rc::new(RefCell::new(State {
                model: ListModel::default(),
                items: Vec::new(),
                flow: PasteFlow::default(),
                open: false,
            })),
            made: Rc::new(RefCell::new(Some(made))),
        };
        list.events(shortcut);
        list
    }

    fn events(&self, shortcut: Option<Shortcut>) {
        let me = self.clone();
        self.wnd.on().wm_create(move |_| {
            let state = hotkey::register(me.wnd.hwnd(), shortcut);
            let _ = me.clips.send(Event::List(ListAsk::Shortcut(state)));
            if let Some(made) = me.made.borrow_mut().take() {
                let _ = made.send(me.wnd.clone());
            }
            Ok(0)
        });
        let me = self.clone();
        self.wnd.on().wm(co::WM::HOTKEY, move |_| {
            if me.state.borrow().open {
                me.close(false);
            } else {
                me.open();
            }
            Ok(0)
        });
        let me = self.clone();
        self.wnd.on().wm_activate(move |p| {
            if p.event == co::WA::INACTIVE {
                me.close(false);
            }
            Ok(())
        });
        let me = self.clone();
        self.wnd.on().wm_close(move || {
            // Alt+F4 hides it; the list stays for the next shortcut.
            me.close(false);
            Ok(())
        });
        let me = self.clone();
        self.search.on().en_change(move || {
            let query = me.search.text().unwrap_or_default();
            {
                let mut state = me.state.borrow_mut();
                let State { model, items, .. } = &mut *state;
                model.search(&query, items);
            }
            me.fill();
            Ok(())
        });
        for (_, _, cmd, key, mods) in keys() {
            let me = self.clone();
            self.wnd.on().wm_command_acc_menu(cmd, move || {
                let outcome = me.state.borrow_mut().model.key(key, mods);
                me.act(outcome);
                Ok(())
            });
        }
        let me = self.clone();
        self.rows.on().nm_click(move |p| {
            if let Ok(row) = usize::try_from(p.iItem) {
                let plain = w::GetKeyState(co::VK::MENU).0;
                let outcome = me.state.borrow().model.click(row, plain);
                me.act(outcome);
            }
            Ok(())
        });
    }

    /// Asks Clips, and waits a moment for its answer.
    fn ask<T>(&self, ask: impl FnOnce(SyncSender<T>) -> ListAsk) -> Option<T> {
        let (reply, answer) = mpsc::sync_channel(1);
        self.clips.send(Event::List(ask(reply))).ok()?;
        answer.recv_timeout(ANSWER).ok()
    }

    fn open(&self) {
        let me = self.wnd.hwnd();
        let front = w::HWND::GetForegroundWindow().filter(|h| h != me);
        let items = self.ask(ListAsk::Items).unwrap_or_default();
        {
            let mut state = self.state.borrow_mut();
            state.flow.remember(front);
            state.items = items;
            let State { model, items, .. } = &mut *state;
            model.open(items);
            state.open = true;
        }
        // An empty search (its change event filters again, the same way).
        let _ = self.search.set_text("");
        self.fill();
        self.place();
        me.ShowWindow(co::SW::SHOW);
        let _ = me.SetForegroundWindow();
        let _ = self.search.focus();
    }

    /// At the pointer, inside its monitor's work area.
    fn place(&self) {
        let Ok(pointer) = w::GetCursorPos() else {
            return;
        };
        let monitor = w::HMONITOR::MonitorFromPoint(pointer, co::MONITOR::DEFAULTTONEAREST);
        let Ok(info) = monitor.GetMonitorInfo() else {
            return;
        };
        let work = info.rcWork;
        let Ok(rect) = self.wnd.hwnd().GetWindowRect() else {
            return;
        };
        let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
        let x = pointer.x.min(work.right - width).max(work.left);
        let y = pointer.y.min(work.bottom - height).max(work.top);
        let _ = self.wnd.hwnd().SetWindowPos(
            w::HwndPlace::Place(co::HWND_PLACE::TOPMOST),
            w::POINT::with(x, y),
            w::SIZE::with(width, height),
            co::SWP::NOSIZE,
        );
    }

    /// The rows, from the model.
    fn fill(&self) {
        let state = self.state.borrow();
        let _ = self.rows.items().delete_all();
        for (row, id) in state.model.shown().iter().enumerate() {
            let Some(item) = state.items.iter().find(|i| &i.id == id) else {
                continue;
            };
            let mut title = page::cut(&item.title(), TITLE_CHARS);
            if title.is_empty() {
                title = " ".into();
            }
            let _ = self.rows.items().add(&[title, info(item, row)], None, ());
        }
        self.show_selection(&state.model);
    }

    fn show_selection(&self, model: &ListModel) {
        let Ok(selected) = u32::try_from(model.selected()) else {
            return;
        };
        if !model.shown().is_empty() {
            let item = self.rows.items().get(selected);
            let _ = item.select(true);
            let _ = item.ensure_visible();
        }
    }

    fn act(&self, outcome: Outcome) {
        match outcome {
            Outcome::Type | Outcome::None => {}
            Outcome::Moved => self.show_selection(&self.state.borrow().model),
            Outcome::Close => self.close(false),
            Outcome::Pin(id) => self.reload(self.ask(|reply| ListAsk::Pin(id, reply))),
            Outcome::Delete(id) => self.reload(self.ask(|reply| ListAsk::Delete(id, reply))),
            Outcome::Pick { id, plain } => self.pick(id, plain),
        }
    }

    fn reload(&self, items: Option<Vec<Item>>) {
        let Some(items) = items else {
            return;
        };
        {
            let mut state = self.state.borrow_mut();
            state.items = items;
            let State { model, items, .. } = &mut *state;
            model.reload(items);
        }
        self.fill();
    }

    /// Puts the item on the clipboard as Clips' own, closes, and pastes where you were (R24).
    fn pick(&self, id: String, plain: bool) {
        let put = self
            .ask(|done| ListAsk::Put { id, plain, done })
            .unwrap_or(false);
        self.close(true);
        if !put {
            self.state.borrow_mut().flow.cancelled();
            return;
        }
        // Out of the state first: the paste brings windows forward, which must find the state free.
        let mut flow = std::mem::take(&mut self.state.borrow_mut().flow);
        let pasted = flow.picked(&WinWindows);
        if let Pasted::CopiedOnly(words) = pasted {
            let _ = self.clips.send(Event::List(ListAsk::Say(words)));
        }
    }

    fn close(&self, picked: bool) {
        let was_open = std::mem::replace(&mut self.state.borrow_mut().open, false);
        if !was_open {
            return;
        }
        if !picked {
            self.state.borrow_mut().flow.cancelled();
        }
        self.wnd.hwnd().ShowWindow(co::SW::HIDE);
    }
}

/// Starts the list on its own thread, holding `shortcut`; it asks Clips on `clips`. Returns how to give it a new
/// shortcut: through a bridge thread, since that waits for the list's thread and the serving loop never should.
pub fn start(
    clips: Sender<Event>,
    shortcut: Option<Shortcut>,
) -> Box<dyn Fn(Option<Shortcut>) + Send> {
    let (made_tx, made_rx) = mpsc::channel();
    let to_clips = clips.clone();
    let started = thread::Builder::new().name("list".into()).spawn(move || {
        let list = List::new(to_clips.clone(), shortcut, made_tx);
        let _ = list.wnd.run_main(Some(co::SW::HIDE));
        // The list is gone (it couldn't be made, or its loop ended): nothing holds the shortcut now (R28).
        let _ = to_clips.send(Event::List(ListAsk::Shortcut(ShortcutState::Broken)));
    });
    let made = started
        .ok()
        .and_then(|_| made_rx.recv_timeout(Duration::from_secs(5)).ok());
    let Some(wnd) = made else {
        let _ = clips.send(Event::List(ListAsk::Shortcut(ShortcutState::Broken)));
        return Box::new(|_| {});
    };
    let (new_tx, new_rx) = mpsc::channel::<Option<Shortcut>>();
    let _ = thread::Builder::new()
        .name("list-shortcut".into())
        .spawn(move || {
            for shortcut in new_rx {
                let on_list = wnd.clone();
                let clips = clips.clone();
                wnd.run_ui_thread(move || {
                    let state = hotkey::register(on_list.hwnd(), shortcut);
                    let _ = clips.send(Event::List(ListAsk::Shortcut(state)));
                    Ok(())
                });
            }
        });
    Box::new(move |shortcut| {
        let _ = new_tx.send(shortcut);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::ShortcutState;

    fn rare(key: char) -> Shortcut {
        // Nobody uses these (the Office key takes W, X, T, O, P, D, L, N and Y with the same modifiers).
        Shortcut {
            ctrl: true,
            alt: true,
            shift: true,
            win: true,
            key,
        }
    }

    fn told(rx: &mpsc::Receiver<Event>) -> Option<ShortcutState> {
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Event::List(ListAsk::Shortcut(state))) => Some(state),
            _ => None,
        }
    }

    #[test]
    fn a_rows_ctrl_number_comes_first() {
        let mut h = crate::history::History::default();
        h.add(
            &crate::history::Capture::text("one", None),
            chrono::Utc::now(),
        );
        let Some(item) = h.items().first() else {
            return;
        };
        let mut pinned = item.clone();
        pinned.pinned = true;
        assert_eq!(info(&pinned, 0), "Ctrl+1 · Text · Pinned");
        assert_eq!(info(item, 9), "Text", "only rows 1 to 9 have a Ctrl+number");
    }

    /// The list's thread starts, holds its shortcut as its window is made, and takes a new one from the page; each
    /// time it tells Clips whether Windows gave it. No key is pressed.
    #[test]
    fn the_list_holds_its_shortcut_and_takes_a_new_one() {
        let (tx, rx) = mpsc::channel();
        let new_shortcut = start(tx, Some(rare('J')));
        assert_eq!(told(&rx), Some(ShortcutState::Ready));
        new_shortcut(Some(rare('K')));
        assert_eq!(told(&rx), Some(ShortcutState::Ready));
        // Another holds this one: refused, never forced (R18).
        let (held_tx, held_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel::<()>();
        let other = thread::spawn(move || {
            let _ = held_tx.send(hotkey::register(&w::HWND::NULL, Some(rare('G'))));
            let _ = done_rx.recv();
            let _ = w::HWND::NULL.UnregisterHotKey(hotkey::ID);
        });
        assert_eq!(held_rx.recv().ok(), Some(ShortcutState::Ready));
        new_shortcut(Some(rare('G')));
        assert_eq!(told(&rx), Some(ShortcutState::Refused));
        new_shortcut(None);
        assert_eq!(
            told(&rx),
            Some(ShortcutState::Ready),
            "no shortcut takes nothing"
        );
        let _ = done_tx.send(());
        let _ = other.join();
    }
}
