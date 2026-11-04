//! Windows' clipboard as Clips reads it (ADR-005 C1, R21): the sequence number alone when nothing changed; the
//! formats' names and the secret markers next; the contents last. Each read opens the clipboard briefly and
//! closes it, retrying for up to 50 ms while another app has it open.

use crate::history::file_name;
use crate::watcher::{Board, Put};
use clipboard_win::{Clipboard, EnumFormats, options::NoClear, raw};
use winsafe::{self as w, co};

/// Retries (5 ms apart) while another app has the clipboard open.
const ATTEMPTS: usize = 10;
/// Standard formats by the names `format_name_big` gives them; the rest are registered by name.
const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;

/// The clipboard of this Windows session.
#[derive(Debug, Clone, Copy, Default)]
pub struct WinBoard;

/// A format's number, standard or registered by name.
fn format_id(name: &str) -> Option<u32> {
    match name {
        "CF_DIB" => Some(CF_DIB),
        "CF_UNICODETEXT" => Some(CF_UNICODETEXT),
        "CF_HDROP" => Some(CF_HDROP),
        _ => raw::register_format(name).map(std::num::NonZeroU32::get),
    }
}

fn open() -> Option<Clipboard> {
    Clipboard::new_attempts(ATTEMPTS).ok()
}

impl Board for WinBoard {
    fn sequence(&self) -> u32 {
        raw::seq_num().map_or(0, std::num::NonZeroU32::get)
    }

    fn formats(&self) -> Vec<String> {
        let Some(_open) = open() else {
            return Vec::new();
        };
        EnumFormats::new()
            .filter_map(raw::format_name_big)
            .collect()
    }

    fn marker(&self, name: &str) -> Option<u32> {
        let format = format_id(name)?;
        let _open = open()?;
        if !raw::is_format_avail(format) {
            return None;
        }
        let mut value = [0u8; 4];
        let read = raw::get(format, &mut value).ok()?;
        (read == value.len()).then(|| u32::from_le_bytes(value))
    }

    fn text(&self) -> Option<String> {
        let _open = open()?;
        let mut out = Vec::new();
        raw::get_string(&mut out).ok()?;
        String::from_utf8(out).ok()
    }

    fn data(&self, format: &str) -> Option<Vec<u8>> {
        let format = format_id(format)?;
        let _open = open()?;
        let mut out = Vec::new();
        raw::get_vec(format, &mut out).ok()?;
        Some(out)
    }

    fn files(&self) -> Vec<String> {
        let Some(_open) = open() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        match raw::get_file_list(&mut out) {
            Ok(_) => out,
            Err(_) => Vec::new(),
        }
    }

    fn put(&mut self, put: &Put) -> Option<u32> {
        let open = open()?;
        raw::empty().ok()?;
        if let Some(text) = &put.text {
            raw::set_string_with(text, NoClear).ok()?;
        }
        if let Some((name, bytes)) = &put.data {
            raw::set_without_clear(format_id(name)?, bytes).ok()?;
        }
        if !put.files.is_empty() {
            raw::set_file_list_with(&put.files, NoClear).ok()?;
        }
        drop(open);
        Some(self.sequence())
    }
}

/// The program in front, by its file's name (`notepad.exe`); `None` when there is none or Windows won't say (an
/// elevated one).
pub fn front_app() -> Option<String> {
    let window = w::HWND::GetForegroundWindow()?;
    let (_, pid) = window.GetWindowThreadProcessId();
    let process =
        w::HPROCESS::OpenProcess(co::PROCESS::QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let path = process
        .QueryFullProcessImageName(co::PROCESS_NAME::WIN32)
        .ok()?;
    Some(file_name(&path).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clips::Clips;
    use crate::watcher::{Watcher, clip};
    use crate::win::real::real_clipboard;
    use chrono::Utc;

    #[test]
    fn a_text_put_is_read_back_as_text() {
        let Some(_board) = real_clipboard() else {
            return;
        };
        let mut board = WinBoard;
        let text = "Samo ทดสอบ é 🙂\r\nline two";
        let put = board.put(&Put {
            text: Some(text.into()),
            ..Put::default()
        });
        assert!(put.is_some(), "Windows refused the put");
        assert_eq!(put, Some(board.sequence()));
        assert!(board.formats().iter().any(|f| f == clip::TEXT));
        assert_eq!(board.text().as_deref(), Some(text));
    }

    #[test]
    fn rich_text_and_files_go_back_as_they_came() {
        let Some(_board) = real_clipboard() else {
            return;
        };
        let mut board = WinBoard;
        let rtf = b"{\\rtf1\\ansi bold}".to_vec();
        board.put(&Put {
            text: Some("bold".into()),
            data: Some((clip::RTF.into(), rtf.clone())),
            files: Vec::new(),
        });
        assert_eq!(board.text().as_deref(), Some("bold"));
        // Windows may pad a format's memory; what was put starts it.
        assert!(board.data(clip::RTF).is_some_and(|d| d.starts_with(&rtf)));

        let files = vec![
            r"C:\Windows\notepad.exe".to_string(),
            r"C:\ไทย\a b.txt".to_string(),
        ];
        board.put(&Put {
            files: files.clone(),
            ..Put::default()
        });
        assert!(board.formats().iter().any(|f| f == clip::FILES));
        assert_eq!(board.files(), files);
    }

    #[test]
    fn copies_marked_secret_are_refused() {
        let Some(_board) = real_clipboard() else {
            return;
        };
        let mut watcher = Watcher::new(WinBoard, Box::new(|| Some("notepad.exe".into())));
        watcher.board_mut().put(&Put {
            text: Some("hunter2".into()),
            data: Some((clip::EXCLUDE_FROM_MONITORING.into(), vec![0])),
            files: Vec::new(),
        });
        assert_eq!(watcher.tick(Utc::now()), None, "an Exclude… copy was kept");

        watcher.board_mut().put(&Put {
            text: Some("hunter3".into()),
            data: Some((
                clip::CAN_INCLUDE_IN_HISTORY.into(),
                0u32.to_le_bytes().to_vec(),
            )),
            files: Vec::new(),
        });
        assert_eq!(
            watcher.board_mut().marker(clip::CAN_INCLUDE_IN_HISTORY),
            Some(0)
        );
        assert_eq!(
            watcher.tick(Utc::now()),
            None,
            "a CanIncludeInClipboardHistory=0 copy was kept"
        );

        watcher.board_mut().put(&Put {
            text: Some("fine".into()),
            ..Put::default()
        });
        assert_eq!(
            watcher.board_mut().marker(clip::CAN_INCLUDE_IN_HISTORY),
            None
        );
        assert!(watcher.tick(Utc::now()).is_some());
    }

    #[test]
    fn clips_own_put_is_not_a_new_copy() -> Result<(), String> {
        let Some(_board) = real_clipboard() else {
            return Ok(());
        };
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let mut clips = Clips::open(
            tmp.path(),
            WinBoard,
            Box::new(|| Some("notepad.exe".into())),
        );
        // Another app copies.
        WinBoard.put(&Put {
            text: Some("from notepad".into()),
            ..Put::default()
        });
        assert!(clips.tick(Utc::now()));
        let id = clips
            .history()
            .items()
            .first()
            .map(|i| i.id.clone())
            .unwrap_or_default();
        WinBoard.put(&Put {
            text: Some("something else".into()),
            ..Put::default()
        });
        assert!(clips.tick(Utc::now()));
        // Clips puts the first back, to paste it.
        assert!(clips.put(&id, false));
        assert_eq!(WinBoard.text().as_deref(), Some("from notepad"));
        assert!(!clips.tick(Utc::now()), "Clips' own put was kept");
        assert_eq!(clips.history().items().len(), 2);
        Ok(())
    }
}
