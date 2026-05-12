// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

use std::cell::RefCell;
use std::fs::File;
use std::io::{BufWriter, Write};

#[derive(Debug, Clone, Copy)]
pub(crate) enum SinkKind {
    Csv,
    Jsonl,
}

pub(crate) struct Sink {
    kind: SinkKind,
    filename: String,
    writer: RefCell<BufWriter<File>>,
}

impl Sink {
    pub(crate) fn new(kind: SinkKind) -> Result<Self, std::io::Error> {
        let filename = sink_filename(kind);
        let file = File::create(&filename)?;

        Ok(Self {
            kind,
            filename,
            writer: RefCell::new(BufWriter::new(file)),
        })
    }

    pub(crate) fn kind(&self) -> SinkKind {
        self.kind
    }

    pub(crate) fn filename(&self) -> &str {
        &self.filename
    }

    pub(crate) fn emit(&self, line: &str) {
        match self.kind {
            SinkKind::Csv => {}
            SinkKind::Jsonl => {
                let escaped = json_escape(line);
                let mut writer = self.writer.borrow_mut();
                if let Err(e) = writeln!(writer, "{{\"line\":\"{escaped}\"}}") {
                    eprintln!("WTG runtime error: failed to write sink output: {e}");
                } else if let Err(e) = writer.flush() {
                    eprintln!("WTG runtime error: failed to flush sink output: {e}");
                }
            }
        }
    }

    pub(crate) fn emit_raw_line(&self, line: &str) {
        let mut writer = self.writer.borrow_mut();
        if let Err(e) = writeln!(writer, "{line}") {
            eprintln!("WTG runtime error: failed to write sink output: {e}");
        } else if let Err(e) = writer.flush() {
            eprintln!("WTG runtime error: failed to flush sink output: {e}");
        }
    }
}

fn sink_filename(kind: SinkKind) -> String {
    let extension = match kind {
        SinkKind::Csv => "csv",
        SinkKind::Jsonl => "jsonl",
    };
    let timestamp = crate::now_ts().replace('.', "_");

    format!("wtg_sink_{timestamp}.{extension}")
}

fn json_escape(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());

    for c in s.chars() {
        match c {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if c.is_control() => escaped.push_str(&format!("\\u{:04x}", c as u32)),
            c => escaped.push(c),
        }
    }

    escaped
}
