//! Reading today's transcripts as they grow (ADR-002 C2): only files changed since local midnight, each from
//! where it was last read, within a byte budget per call so it never runs hot (at most 8 MiB a second).
//! A line without its newline yet waits for it.
//!
//! A long-running session's file holds days of history before today's lines (KJ's held 724 MB in 8 files
//! touched in one morning). Lines are written in time order, so a new file is first searched for where today
//! begins, and only that part is read.

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
    /// The start of today's part has been found.
    positioned: bool,
}

/// Files smaller than this are read from the start.
const SEARCH_ABOVE: u64 = 1024 * 1024;
/// How much is read at each step of the search.
const PROBE: u64 = 256 * 1024;

/// The time on a transcript line, found without parsing the whole line.
fn line_time_ms(line: &[u8]) -> Option<i64> {
    let text = std::str::from_utf8(line).ok()?;
    let at = text.find("\"timestamp\":\"")? + "\"timestamp\":\"".len();
    let rest = &text[at..];
    let end = rest.find('"')?;
    chrono::DateTime::parse_from_rfc3339(&rest[..end])
        .ok()
        .map(|t| t.timestamp_millis())
}

/// The first timestamped line starting at or after `from`: its offset and time. `from` is moved past a partial
/// line first, unless it is 0.
fn probe(file: &mut File, from: u64, len: u64) -> Option<(u64, i64)> {
    file.seek(SeekFrom::Start(from)).ok()?;
    let mut buf = Vec::new();
    file.take(PROBE.min(len - from))
        .read_to_end(&mut buf)
        .ok()?;
    let mut pos = if from == 0 {
        0
    } else {
        buf.iter().position(|b| *b == b'\n')? + 1
    };
    while pos < buf.len() {
        let end = buf[pos..].iter().position(|b| *b == b'\n')?;
        if let Some(ts) = line_time_ms(&buf[pos..pos + end]) {
            return Some((from + u64::try_from(pos).ok()?, ts));
        }
        pos += end + 1;
    }
    None
}

/// Where the first line from `midnight_ms` on starts, or near before it (always the start of a line).
fn start_of_today(path: &Path, midnight_ms: i64) -> u64 {
    let Ok(mut file) = File::open(path) else {
        return 0;
    };
    let Some(len) = file.metadata().ok().map(|m| m.len()) else {
        return 0;
    };
    if len <= SEARCH_ABOVE {
        return 0;
    }
    // `lo` is always a line start known to be before midnight (or 0).
    let (mut lo, mut hi) = (0u64, len);
    while hi - lo > PROBE {
        let mid = lo + (hi - lo) / 2;
        match probe(&mut file, mid, len) {
            Some((at, ts)) if ts < midnight_ms => lo = at,
            Some(_) => hi = mid,
            // Nothing timestamped this far in: search the earlier half.
            None => hi = mid,
        }
    }
    lo
}

pub struct Reader {
    root: PathBuf,
    files: HashMap<PathBuf, Tracked>,
    tally: DayTally,
    /// The last read used its whole budget: there is more to read, so the count isn't complete yet.
    behind: bool,
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
            behind: false,
        }
    }

    /// Whether today's transcripts are still being caught up with (after Samo starts, or a burst of writing).
    pub fn behind(&self) -> bool {
        self.behind
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
            if !tracked.positioned {
                tracked.positioned = true;
                tracked.offset = start_of_today(&path, midnight);
            }
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
        self.behind = used >= budget;
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
    fn long_history_is_skipped_to_today() -> Result<(), String> {
        let (_dir, projects) = setup()?;
        let file = projects.join("-Users-kj-samo/s1.jsonl");
        let at = |ts: &str, msg: &str, input: u64| {
            format!(
                r#"{{"sessionId":"s1","timestamp":"{ts}","message":{{"id":"{msg}","usage":{{"input_tokens":{input},"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}},"content":"{}"}}}}"#,
                "x".repeat(300)
            ) + "\n"
        };
        // About 6 MB of yesterday, a line without a time, then today.
        let mut text = String::new();
        for i in 0..15_000 {
            text += &at("2026-10-31T10:00:00Z", &format!("old{i}"), 1_000);
        }
        text += "{\"type\":\"summary\"}\n";
        for i in 0..50 {
            text += &at("2026-11-01T09:00:00Z", &format!("new{i}"), 2);
        }
        append(&file, &text)?;
        let midnight = chrono::DateTime::parse_from_rfc3339("2026-11-01T00:00:00Z")
            .map_or(0, |t| t.timestamp_millis());

        let start = start_of_today(&file, midnight);
        let today_starts = text
            .find("2026-11-01")
            .map_or(0, |i| text[..i].rfind('\n').map_or(0, |n| n + 1));
        let today_starts = u64::try_from(today_starts).unwrap_or(0);
        assert!(start <= today_starts, "{start} > {today_starts}");
        assert!(
            today_starts - start <= PROBE * 2,
            "skipped too little: {start} of {today_starts}"
        );

        let mut reader = Reader::new(projects, midnight);
        reader.sweep();
        let mut read = 0;
        loop {
            let n = reader.read_new(u64::MAX);
            if n == 0 {
                break;
            }
            read += n;
        }
        assert_eq!(reader.tally().total().input, 100);
        assert!(
            read < u64::try_from(text.len()).unwrap_or(0) / 10,
            "read {read} bytes"
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
        assert!(reader.behind());
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
        assert!(!reader.behind());
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
