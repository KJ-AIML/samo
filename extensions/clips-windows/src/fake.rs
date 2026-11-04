//! A clipboard for tests: what is on it, what Clips put on it, and how often each part of it was read.

use crate::watcher::{Board, Put, clip};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Default)]
pub struct FakeBoard {
    pub sequence: u32,
    pub formats: Vec<String>,
    pub no_history: bool,
    pub text: Option<String>,
    pub data: Vec<(String, Vec<u8>)>,
    pub files: Vec<String>,
    /// What Clips put, in order.
    pub puts: Vec<Put>,
    /// Another app has the clipboard open: puts fail.
    pub busy: bool,
    pub sequence_reads: Rc<Cell<u32>>,
    pub format_reads: Rc<Cell<u32>>,
    pub content_reads: Rc<Cell<u32>>,
}

impl FakeBoard {
    /// Someone copied `text`.
    pub fn copy_text(&mut self, text: &str) {
        self.sequence += 1;
        self.formats = vec![clip::TEXT.into()];
        self.no_history = false;
        self.text = Some(text.into());
        self.data.clear();
        self.files.clear();
    }
}

impl Board for FakeBoard {
    fn sequence(&self) -> u32 {
        self.sequence_reads.set(self.sequence_reads.get() + 1);
        self.sequence
    }
    fn formats(&self) -> Vec<String> {
        self.format_reads.set(self.format_reads.get() + 1);
        self.formats.clone()
    }
    fn marker(&self, name: &str) -> Option<u32> {
        (name == clip::CAN_INCLUDE_IN_HISTORY && self.no_history).then_some(0)
    }
    fn text(&self) -> Option<String> {
        self.content_reads.set(self.content_reads.get() + 1);
        self.text.clone()
    }
    fn data(&self, format: &str) -> Option<Vec<u8>> {
        self.content_reads.set(self.content_reads.get() + 1);
        self.data
            .iter()
            .find(|(f, _)| f == format)
            .map(|(_, d)| d.clone())
    }
    fn files(&self) -> Vec<String> {
        self.content_reads.set(self.content_reads.get() + 1);
        self.files.clone()
    }
    fn put(&mut self, put: &Put) -> Option<u32> {
        if self.busy {
            return None;
        }
        self.sequence += 1;
        self.formats.clear();
        if put.text.is_some() {
            self.formats.push(clip::TEXT.into());
        }
        self.text.clone_from(&put.text);
        self.data = put.data.iter().cloned().collect();
        self.formats
            .extend(self.data.iter().map(|(f, _)| f.clone()));
        self.files.clone_from(&put.files);
        if !self.files.is_empty() {
            self.formats.push(clip::FILES.into());
        }
        self.puts.push(put.clone());
        Some(self.sequence)
    }
}
