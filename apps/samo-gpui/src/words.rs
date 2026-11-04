//! What Samo calls things on each OS (ADR-006 §3). The Mac's words are exactly as they were; Windows has its own
//! names for the same things, and never hears "Mac", "Finder" or "Trash".

/// Where removed things go and come back from.
pub const TRASH: &str = if cfg!(windows) {
    "Recycle Bin"
} else {
    "Trash"
};
/// Showing a folder in the OS's file browser.
pub const REVEAL: &str = if cfg!(windows) {
    "Show in Explorer"
} else {
    "Reveal in Finder"
};
/// The computer Samo watches, as a name.
pub const THIS_COMPUTER: &str = if cfg!(windows) { "This PC" } else { "This Mac" };
/// The computer, in a sentence ("on this Mac", "Mac up 2 d").
pub const COMPUTER: &str = if cfg!(windows) { "PC" } else { "Mac" };
/// What Quit does, for its tooltip: a signal on the Mac; on Windows, which has none, a request that the app close.
pub const QUIT_TIP: &str = if cfg!(windows) {
    "Ask it to close, after checking it is still the same process"
} else {
    "Send SIGTERM, after checking it is still the same process"
};
/// Where a removed item can be put back by hand.
pub const RESTORE_FROM: &str = if cfg!(windows) {
    "the Recycle Bin"
} else {
    "the Trash in Finder"
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_follow_the_os() {
        let all = [TRASH, REVEAL, THIS_COMPUTER, COMPUTER, RESTORE_FROM];
        if cfg!(windows) {
            for word in all {
                assert!(
                    !word.contains("Mac") && !word.contains("Finder") && !word.contains("Trash"),
                    "{word}"
                );
            }
            assert_eq!(TRASH, "Recycle Bin");
            // Windows has no signals: Quit asks the app to close.
            assert!(!QUIT_TIP.contains("SIG"), "{QUIT_TIP}");
        } else {
            assert_eq!(
                QUIT_TIP,
                "Send SIGTERM, after checking it is still the same process"
            );
            assert_eq!(
                (TRASH, REVEAL, THIS_COMPUTER, COMPUTER, RESTORE_FROM),
                (
                    "Trash",
                    "Reveal in Finder",
                    "This Mac",
                    "Mac",
                    "the Trash in Finder"
                )
            );
        }
    }
}
