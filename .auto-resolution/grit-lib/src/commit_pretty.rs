//! Human-oriented commit one-line formats shared by porcelain commands.

use crate::objects::ObjectId;

/// Abbreviate `oid` to at most `abbrev_len` hex characters (minimum 4, maximum 40).
///
/// # Parameters
///
/// - `oid` — full commit object id.
/// - `abbrev_len` — desired abbreviation length (clamped to 4..=40 and to the hex length).
#[must_use]
pub fn abbrev_hex(oid: &ObjectId, abbrev_len: usize) -> String {
    let hex = oid.to_hex();
    let n = abbrev_len.clamp(4, 40).min(hex.len());
    hex[..n].to_owned()
}

/// Return the pretty subject for a commit or tag message.
///
/// The subject is the first non-empty paragraph with embedded line breaks
/// collapsed to spaces. Both LF and CRLF line endings are recognized.
///
/// # Parameters
///
/// - `message` — raw commit or tag message text.
#[must_use]
pub fn message_subject(message: &str) -> String {
    let mut subject_lines = Vec::new();
    for line in MessageLines::new(message) {
        if line.text.is_empty() {
            if !subject_lines.is_empty() {
                break;
            }
            continue;
        }
        subject_lines.push(line.text);
    }
    subject_lines.join(" ")
}

/// Return the body slice after the first message paragraph.
///
/// Leading blank lines before the first paragraph are ignored. The returned
/// body starts after the blank-line separator and any additional blank lines,
/// preserving the original body line endings and trailing newline bytes.
///
/// # Parameters
///
/// - `message` — raw commit or tag message text.
#[must_use]
pub fn message_body(message: &str) -> &str {
    let mut saw_subject = false;
    let mut body_start = message.len();
    let mut iter = MessageLines::new(message).peekable();

    while let Some(line) = iter.next() {
        if line.text.is_empty() {
            if saw_subject {
                body_start = line.next_start;
                while let Some(next) = iter.peek() {
                    if !next.text.is_empty() {
                        break;
                    }
                    body_start = next.next_start;
                    iter.next();
                }
                break;
            }
            continue;
        }
        saw_subject = true;
    }

    &message[body_start..]
}

#[derive(Clone, Copy)]
struct MessageLine<'a> {
    text: &'a str,
    next_start: usize,
}

struct MessageLines<'a> {
    message: &'a str,
    pos: usize,
}

impl<'a> MessageLines<'a> {
    fn new(message: &'a str) -> Self {
        Self { message, pos: 0 }
    }
}

impl<'a> Iterator for MessageLines<'a> {
    type Item = MessageLine<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.message.len() {
            return None;
        }
        let start = self.pos;
        let tail = &self.message[start..];
        let newline_rel = tail.find('\n');
        let (mut end, next_start) = match newline_rel {
            Some(rel) => (start + rel, start + rel + 1),
            None => (self.message.len(), self.message.len()),
        };
        if self.message.as_bytes().get(end.wrapping_sub(1)) == Some(&b'\r') && end > start {
            end -= 1;
        }
        self.pos = next_start;
        Some(MessageLine {
            text: &self.message[start..end],
            next_start,
        })
    }
}

fn parse_tz_offset_seconds(offset: &str) -> i64 {
    if offset.len() < 5 {
        return 0;
    }
    let sign = if offset.starts_with('-') { -1i64 } else { 1i64 };
    let hours: i64 = offset[1..3].parse().unwrap_or(0);
    let minutes: i64 = offset[3..5].parse().unwrap_or(0);
    sign * (hours * 3600 + minutes * 60)
}

/// Format the author/committer date as `YYYY-MM-DD` in the commit's local timezone.
///
/// Matches Git's `DATE_SHORT` mode used by `--pretty=reference` (e.g. `2005-04-07`).
#[must_use]
pub fn format_short_date_from_ident(ident: &str) -> String {
    let parts: Vec<&str> = ident.rsplitn(3, ' ').collect();
    if parts.len() < 2 {
        return ident.to_owned();
    }
    let ts_str = parts[1];
    let offset_str = parts[0];
    let Ok(ts) = ts_str.parse::<i64>() else {
        return ident.to_owned();
    };
    let offset_secs = parse_tz_offset_seconds(offset_str);
    let Ok(dt) = time::OffsetDateTime::from_unix_timestamp(ts + offset_secs) else {
        return ident.to_owned();
    };
    let format = time::format_description::parse("[year]-[month]-[day]");
    let Ok(fmt) = format else {
        return ident.to_owned();
    };
    dt.format(&fmt).unwrap_or_else(|_| ident.to_owned())
}

/// One-line `reference` format: `abbrev (subject, YYYY-MM-DD)`.
///
/// Matches upstream `git show -s --pretty=reference` / sequencer `refer_to_commit` output.
///
/// # Parameters
///
/// - `subject_first_line` — first line of the commit message (no trailing newline).
/// - `committer_ident` — raw `committer` header line (`Name <email> epoch tz`).
/// - `abbrev_len` — abbreviation length for the hash (typically 7).
#[must_use]
pub fn format_reference_line(
    oid: &ObjectId,
    subject_first_line: &str,
    committer_ident: &str,
    abbrev_len: usize,
) -> String {
    let abbrev = abbrev_hex(oid, abbrev_len);
    let date = format_short_date_from_ident(committer_ident);
    format!("{abbrev} ({subject_first_line}, {date})")
}
