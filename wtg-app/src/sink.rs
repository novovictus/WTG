// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Adam Hooper

use std::cell::RefCell;
use std::fs::File;
use std::io::{BufWriter, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SinkKind {
    Csv,
    Jsonl,
    Mqtt,
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

    pub(crate) fn emit_jsonl_lines(&self, text: &str) {
        if self.kind != SinkKind::Jsonl {
            return;
        }

        let mut lines = text.split('\n').peekable();
        while let Some(line) = lines.next() {
            if lines.peek().is_none() && line.is_empty() {
                break;
            }
            self.emit_jsonl_record(line.trim_end_matches('\r'));
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

    fn emit_jsonl_record(&self, line: &str) {
        let escaped = json_escape(line);
        let mut writer = self.writer.borrow_mut();
        if let Err(e) = writeln!(writer, "{{\"line\":\"{escaped}\"}}") {
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
        SinkKind::Mqtt => "mqtt",
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

pub(crate) fn csv_escape_field(s: &str) -> String {
    if !s.chars().any(|c| matches!(c, ',' | '"' | '\n' | '\r')) {
        return s.to_string();
    }

    let mut escaped = String::with_capacity(s.len() + 2);
    escaped.push('"');
    for c in s.chars() {
        if c == '"' {
            escaped.push('"');
        }
        escaped.push(c);
    }
    escaped.push('"');
    escaped
}

pub(crate) fn format_csv_row(fields: &[String]) -> String {
    fields
        .iter()
        .map(|field| csv_escape_field(field))
        .collect::<Vec<_>>()
        .join(",")
}
