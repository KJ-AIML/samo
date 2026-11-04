//! Samo's shortcuts are `secondary-…`: ⌘ on the Mac (as they always were) and Ctrl on Windows, where GPUI reads
//! `cmd-` as the Windows key (Win+Q and Win+0 belong to Windows) (ADR-006 §3).

/// How a `secondary-<key>` shortcut is written for people: `⌘R` on the Mac, `Ctrl+R` on Windows.
pub fn shortcut_label(key: char) -> String {
    if cfg!(target_os = "macos") {
        format!("⌘{key}")
    } else {
        format!("Ctrl+{key}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secondary_is_cmd_on_the_mac_and_ctrl_on_windows() -> Result<(), String> {
        let quit = gpui::Keystroke::parse("secondary-q").map_err(|e| format!("{e:?}"))?;
        if cfg!(target_os = "macos") {
            assert!(quit.modifiers.platform && !quit.modifiers.control);
            assert_eq!(shortcut_label('R'), "⌘R");
        } else {
            assert!(quit.modifiers.control && !quit.modifiers.platform);
            assert_eq!(shortcut_label('R'), "Ctrl+R");
        }
        Ok(())
    }
}
