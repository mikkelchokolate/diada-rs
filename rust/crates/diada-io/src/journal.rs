use crate::{EncodingError, decode_cp866, encode_cp866};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    Crlf,
    Lf,
    Cr,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalLine {
    pub bytes: Vec<u8>,
    pub ending: LineEnding,
}

impl JournalLine {
    pub fn text(&self) -> String {
        decode_cp866(&self.bytes)
    }
    pub fn is_marker(&self) -> bool {
        self.bytes.starts_with(b"_") && self.bytes.len() > 2
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Journal {
    lines: Vec<JournalLine>,
}

impl Journal {
    pub(crate) fn from_lines(lines: Vec<JournalLine>) -> Self {
        Self { lines }
    }
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut lines = Vec::new();
        let mut position = 0;
        while position < bytes.len() {
            let start = position;
            while position < bytes.len() && bytes[position] != b'\r' && bytes[position] != b'\n' {
                position += 1;
            }
            let line = bytes[start..position].to_vec();
            let ending = match bytes.get(position) {
                Some(b'\r') if bytes.get(position + 1) == Some(&b'\n') => {
                    position += 2;
                    LineEnding::Crlf
                }
                Some(b'\r') => {
                    position += 1;
                    LineEnding::Cr
                }
                Some(b'\n') => {
                    position += 1;
                    LineEnding::Lf
                }
                _ => LineEnding::None,
            };
            lines.push(JournalLine {
                bytes: line,
                ending,
            });
        }
        Self { lines }
    }

    pub fn lines(&self) -> &[JournalLine] {
        &self.lines
    }
    pub fn cursor(&self) -> LegacyCursor<'_> {
        LegacyCursor {
            journal: self,
            position: 0,
        }
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut output = Vec::new();
        for line in &self.lines {
            output.extend_from_slice(&line.bytes);
            output.extend_from_slice(match line.ending {
                LineEnding::Crlf => b"\r\n",
                LineEnding::Lf => b"\n",
                LineEnding::Cr => b"\r",
                LineEnding::None => b"",
            });
        }
        output
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalError {
    pub line: usize,
    pub message: String,
}
impl fmt::Display for JournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "dialogue line {}: {}", self.line, self.message)
    }
}
impl std::error::Error for JournalError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LegacyName(pub [u8; 2]);
impl LegacyName {
    pub const BLANK: Self = Self([b' ', b' ']);
    pub fn from_text(text: &str) -> Result<Self, EncodingError> {
        let bytes = encode_cp866(text)?;
        if bytes.len() > 2 {
            return Err(EncodingError {
                character: text.chars().nth(2).unwrap(),
                character_index: 3,
            });
        }
        let mut name = [b' '; 2];
        name[..bytes.len()].copy_from_slice(&bytes);
        Ok(Self(name))
    }
    pub fn text(self) -> String {
        decode_cp866(&self.0)
    }
    pub fn is_blank(self) -> bool {
        self.0[0] == b' '
    }
}

pub struct LegacyCursor<'a> {
    journal: &'a Journal,
    position: usize,
}
impl<'a> LegacyCursor<'a> {
    pub fn position(&self) -> usize {
        self.position
    }
    pub fn remaining(&self) -> usize {
        self.journal.lines.len() - self.position
    }
    pub fn peek(&self) -> Option<&'a JournalLine> {
        self.journal.lines.get(self.position)
    }
    pub fn read_line(&mut self) -> Result<&'a [u8], JournalError> {
        let line = self.journal.lines.get(self.position).ok_or_else(|| {
            self.error("unexpected end of dialogue; keyboard continuation is required")
        })?;
        self.position += 1;
        Ok(&line.bytes)
    }
    pub fn expect_marker(&mut self, expected: &str) -> Result<(), JournalError> {
        let line_number = self.position + 1;
        let actual = decode_cp866(self.read_line()?);
        if actual == expected {
            Ok(())
        } else {
            Err(JournalError {
                line: line_number,
                message: format!("expected marker {expected:?}, found {actual:?}"),
            })
        }
    }
    pub fn read_character(&mut self) -> Result<u8, JournalError> {
        let line_number = self.position + 1;
        let character = self.read_line()?.first().copied().unwrap_or(b'\r');
        if character == 27 || character == 3 {
            return Err(JournalError { line: line_number, message: "interrupted/undefined legacy input path; explicit user continuation is required".to_owned() });
        }
        Ok(character)
    }
    pub fn read_yes_no(&mut self) -> Result<bool, JournalError> {
        loop {
            match self.read_character()?.to_ascii_uppercase() {
                b'Y' => return Ok(true),
                b'N' => return Ok(false),
                _ => {}
            }
        }
    }
    pub fn read_name(&mut self) -> Result<LegacyName, JournalError> {
        loop {
            let first = loop {
                let character = self.read_character()?;
                if character == b' ' {
                    return Ok(LegacyName::BLANK);
                }
                if (32..=127).contains(&character) {
                    break character;
                }
            };
            let first = if first > b'Z' { first - 32 } else { first };
            let second = loop {
                let character = self.read_character()?;
                if character == 8 || character == b'\r' || (32..=127).contains(&character) {
                    break character;
                }
            };
            if second == 8 {
                continue;
            }
            let second = if second == b'\r' {
                b' '
            } else if second > b'Z' {
                second - 32
            } else {
                second
            };
            // A recorded Enter occupies two empty text records in the legacy
            // name dialogue. Keep both records in Journal, consume the pair as
            // the single accepted second character.
            if second == b' ' && self.peek().is_some_and(|line| line.bytes.is_empty()) {
                self.position += 1;
            }
            return Ok(LegacyName([first, second]));
        }
    }
    pub fn error(&self, message: impl Into<String>) -> JournalError {
        JournalError {
            line: self.position + 1,
            message: message.into(),
        }
    }
    pub fn skip_empty_lines(&mut self) {
        while self.peek().is_some_and(|line| line.bytes.is_empty()) {
            self.position += 1;
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct JournalWriter {
    bytes: Vec<u8>,
}
impl JournalWriter {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn write_line(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
        self.bytes.extend_from_slice(b"\r\n");
    }
    pub fn write_text(&mut self, text: &str) -> Result<(), EncodingError> {
        self.write_line(&encode_cp866(text)?);
        Ok(())
    }
    pub fn write_character(&mut self, character: u8) {
        self.write_line(&[character]);
    }
    pub fn write_yes_no(&mut self, value: bool) {
        self.write_character(if value { b'Y' } else { b'N' });
    }
    pub fn write_name(&mut self, name: LegacyName) {
        if name.is_blank() {
            self.write_character(b' ');
            return;
        }
        self.write_character(name.0[0]);
        if name.0[1] == b' ' {
            self.write_line(b"");
            self.write_line(b"");
        } else {
            self.write_character(name.0[1]);
        }
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
    pub fn into_journal(self) -> Journal {
        Journal::from_bytes(&self.bytes)
    }
}
