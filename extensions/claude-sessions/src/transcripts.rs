//! Reading today's transcripts as they grow (ADR-002 C2): only files changed since local midnight, each from
//! where it was last read, within a byte budget per call so it never runs hot (at most 8 MiB a second).
//! A line without its newline yet waits for it.

use crate::usage::{DayTally, parse_line};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// `projects/<project>/<session>.jsonl` and `projects/<project>/<session>/subagents/agent-*.jsonl`.
const MAX_DEPTH: usize = 4;

#[derive(Debug, Default)]
struct Tracked {
    offset: u64,
    partial: Vec<u8>,
}

pub struct Reader {
    root: PathBuf,
    files: HashMap<PathBuf, Tracked>,
    tally: DayTally,
}

fn modified_ms(path: &Path) -> Option<i64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
}

/// The session a transcript belongs to: the file's stem, or for a subagent's, the folder it is in.
fn session_of(path: &Path) -> String {
    let parent = path.parent();
    if parent
        .and_then(Path::file_name)
        .is_some_and(|n| n == "subagents")
    {
        return parent
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    path.file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl Reader {
    /// `root` is Claude Code's `projects` folder.
    pub fn new(root: PathBuf, midnight_ms: i64) -> Self {
        Self {
            root,
            files: HashMap::new(),
            tally: DayTally::new(midnight_ms),
        }
    }

    pub fn tally(&self) -> &DayTally {
        &self.tally
    }

    /// A new day starts the count again, reading today's files from their start.
    pub fn set_midnight(&mut self, midnight_ms: i64) {
        if midnight_ms != self.tally.midnight_ms() {
            let found = self.tally.source_found;
            self.tally = DayTally::new(midnight_ms);
            self.tally.source_found = found;
            self.files.clear();
        }
    }

    /// Finds every transcript changed today under the root.
    pub fn sweep(&mut self) {
        self.tally.source_found = self.root.is_dir();
        let midnight = self.tally.midnight_ms();
        let mut found = Vec::new();
        walk(&self.root, 0, &mut found);
        for path in found {
            if modified_ms(&path).is_some_and(|m| m >= midnight) {
                self.files.entry(path).or_default();
            }
        }
    }

    /// Follows a transcript a hook named, wherever it is.
    pub fn follow(&mut self, path: &Path) {
        if path.extension().is_some_and(|e| e == "jsonl") && path.is_file() {
            self.files.entry(path.to_path_buf()).or_default();
        }
    }

    /// Reads what was added to tracked files, at most `budget` bytes in all. Returns the bytes read.
    pub fn read_new(&mut self, budget: u64) -> u64 {
        let midnight = self.tally.midnight_ms();
        let mut used = 0u64;
        let mut paths: Vec<PathBuf> = self.files.keys().cloned().collect();
        paths.sort();
        for path in paths {
            if used >= budget {
                break;
            }
            let Some(len) = fs::metadata(&path).ok().map(|m| m.len()) else {
                self.files.remove(&path);
                continue;
            };
            if modified_ms(&path).is_some_and(|m| m < midnight) {
                continue;
            }
            let session = session_of(&path);
            let Some(tracked) = self.files.get_mut(&path) else {
                continue;
            };
            if len < tracked.offset {
                // Rewritten from the start.
                tracked.offset = 0;
                tracked.partial.clear();
            }
            if len == tracked.offset {
                continue;
            }
            let take = (len - tracked.offset).min(budget - used);
            let Ok(mut file) = File::open(&path) else {
                continue;
            };
            if file.seek(SeekFrom::Start(tracked.offset)).is_err() {
                continue;
            }
            let mut chunk = Vec::new();
            if file.take(take).read_to_end(&mut chunk).is_err() {
                continue;
            }
            let read = u64::try_from(chunk.len()).unwrap_or(0);
            tracked.offset += read;
            used += read;
            tracked.partial.extend_from_slice(&chunk);
            let Some(last_newline) = tracked.partial.iter().rposition(|b| *b == b'\n') else {
                continue;
            };
            let complete: Vec<u8> = tracked.partial.drain(..=last_newline).collect();
            for line in complete.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
                let text = String::from_utf8_lossy(line);
                self.tally.add(&session, parse_line(&text));
            }
        }
        used
    }
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            walk(&path, depth + 1, out);
        } else if kind.is_file() && path.extension().is_some_and(|e| e == "jsonl") {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn line(msg: &str, session: &str, input: u64) -> String {
        format!(
            r#"{{"sessionId":"{session}","timestamp":"2026-11-01T09:00:00Z","message":{{"id":"{msg}","usage":{{"input_tokens":{input},"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}}}}"#
        ) + "\n"
    }

    fn append(path: &Path, text: &str) -> Result<(), String> {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        f.write_all(text.as_bytes()).map_err(|e| e.to_string())
    }

    fn setup() -> Result<(tempfile::TempDir, PathBuf), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let projects = dir.path().join("projects");
        fs::create_dir_all(projects.join("-Users-kj-samo/s1/subagents"))
            .map_err(|e| e.to_string())?;
        Ok((dir, projects))
    }

    #[test]
    fn offset_resume_reads_only_new_bytes() -> Result<(), String> {
        let (_dir, projects) = setup()?;
        let file = projects.join("-Users-kj-samo/s1.jsonl");
        append(&file, &(line("a", "s1", 1) + &line("b", "s1", 2)))?;
        let mut reader = Reader::new(projects, 0);
        reader.sweep();
        reader.read_new(u64::MAX);
        assert_eq!(reader.tally().total().input, 3);

        let more = line("c", "s1", 4);
        append(&file, &more)?;
        let read = reader.read_new(u64::MAX);
        assert_eq!(read, u64::try_from(more.len()).unwrap_or(0));
        assert_eq!(reader.tally().total().input, 7);
        assert_eq!(reader.read_new(u64::MAX), 0);
        Ok(())
    }

    #[test]
    fn partial_last_line_waits() -> Result<(), String> {
        let (_dir, projects) = setup()?;
        let file = projects.join("-Users-kj-samo/s1.jsonl");
        let whole = line("a", "s1", 5);
        let (first, rest) = whole.split_at(40);
        append(&file, first)?;
        let mut reader = Reader::new(projects, 0);
        reader.sweep();
        reader.read_new(u64::MAX);
        assert_eq!(reader.tally().total().input, 0);
        append(&file, rest)?;
        reader.read_new(u64::MAX);
        assert_eq!(reader.tally().total().input, 5);
        Ok(())
    }

    #[test]
    fn subagent_transcripts_counted() -> Result<(), String> {
        let (_dir, projects) = setup()?;
        append(
            &projects.join("-Users-kj-samo/s1.jsonl"),
            &line("a", "s1", 1),
        )?;
        append(
            &projects.join("-Users-kj-samo/s1/subagents/agent-x.jsonl"),
            &line("b", "s1", 10),
        )?;
        let mut reader = Reader::new(projects, 0);
        reader.sweep();
        reader.read_new(u64::MAX);
        assert_eq!(reader.tally().total().input, 11);
        assert_eq!(reader.tally().per_session("s1").map(|t| t.input), Some(11));
        assert_eq!(
            session_of(Path::new("/p/-proj/s1/subagents/agent-x.jsonl")),
            "s1"
        );
        Ok(())
    }

    #[test]
    fn budget_is_kept() -> Result<(), String> {
        let (_dir, projects) = setup()?;
        let file = projects.join("-Users-kj-samo/s1.jsonl");
        let text: String = (0..100).map(|i| line(&format!("m{i}"), "s1", 1)).collect();
        append(&file, &text)?;
        let mut reader = Reader::new(projects, 0);
        reader.sweep();
        assert_eq!(reader.read_new(1_000), 1_000);
        let mut total = 1_000;
        loop {
            let read = reader.read_new(1_000);
            if read == 0 {
                break;
            }
            total += read;
        }
        assert_eq!(total, u64::try_from(text.len()).unwrap_or(0));
        assert_eq!(reader.tally().total().input, 100);
        Ok(())
    }

    #[test]
    fn missing_root_is_not_found_and_old_files_are_skipped() -> Result<(), String> {
        let mut reader = Reader::new(PathBuf::from("/nonexistent/projects"), 0);
        reader.sweep();
        assert!(!reader.tally().source_found);

        let (_dir, projects) = setup()?;
        append(
            &projects.join("-Users-kj-samo/s1.jsonl"),
            &line("a", "s1", 1),
        )?;
        // Midnight is in the future: nothing changed "today" yet.
        let mut reader = Reader::new(projects, i64::MAX);
        reader.sweep();
        assert!(reader.tally().source_found);
        assert_eq!(reader.read_new(u64::MAX), 0);
        Ok(())
    }
}
