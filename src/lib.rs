//! Turn whisper.cpp output into clean, caption-ready segments.
//!
//! whisper.cpp can emit a JSON file (`-oj` / `-ojf`) or plain stdout lines like
//! `[00:00:01.000 --> 00:00:03.500]  Hello there`. This crate normalizes both
//! into [`Segment`]s with millisecond timestamps and, when token-level data is
//! present, whole-word [`Word`] timings.
//!
//! whisper.cpp's special tokens (`[_BEG_]`, `[_TT_150]`, `<|endoftext|>`, …)
//! are dropped, and [`to_srt`] / [`to_vtt`] turn segments into subtitle files.
//!
//! No whisper dependency: it works on output files from any whisper.cpp
//! version, and [`merge_bpe_tokens`] also accepts tokens from `whisper-rs`.
//!
//! ```
//! let stdout = "[00:00:01.000 --> 00:00:02.500]  Hello   world\n";
//! let segments = whisper_captions::parse_whisper_stdout(stdout);
//! assert_eq!(segments[0].text, "Hello world");
//! assert_eq!(segments[0].start_ms, 1000);
//! assert_eq!(segments[0].duration_ms, 1500);
//!
//! let srt = whisper_captions::to_srt(&segments);
//! assert!(srt.starts_with("1\n00:00:01,000 --> 00:00:02,500\nHello world\n"));
//! ```

mod js;

use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use js::{
    get, get_path, is_falsy, is_js_space, js_round, js_string, js_string_or_empty, nullish,
    sanitize, trim_start_js,
};

/// A whole word with its timing, reassembled from whisper's BPE tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// One caption-ready transcript segment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub duration_ms: i64,
    pub text: String,
    /// Empty when the source had no usable word/token timings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<Word>,
}

/// A raw whisper token before BPE merging. The text is kept untouched so the
/// leading-space "starts a new word" signal survives until merging.
#[derive(Debug, Clone, PartialEq)]
pub struct RawToken {
    pub raw_text: String,
    pub start_ms: f64,
    pub end_ms: f64,
}

/// Parse a whisper timestamp into milliseconds.
///
/// * numbers above 100000 are treated as milliseconds, smaller ones as seconds
/// * strings accept `HH:MM:SS.mmm`, `MM:SS,mmm`, `H:MM:SS` and similar
pub fn parse_timestamp_ms(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => {
            let v = n.as_f64()?;
            Some(if v > 100000.0 {
                js_round(v)
            } else {
                js_round(v * 1000.0)
            })
        }
        Value::String(s) => parse_timestamp_str(s),
        _ => None,
    }
}

fn parse_timestamp_str(s: &str) -> Option<f64> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^(?:([0-9]{1,2}):)?([0-9]{1,2}):([0-9]{1,2})(?:[.,]([0-9]{1,3}))?$").unwrap()
    });
    let trimmed = s.trim_matches(is_js_space);
    let caps = re.captures(trimmed)?;
    let num = |i: usize| {
        caps.get(i)
            .map_or(0.0, |m| m.as_str().parse::<f64>().unwrap_or(0.0))
    };
    let ms = caps.get(4).map_or(0.0, |m| {
        format!("{:0<3}", m.as_str()).parse::<f64>().unwrap_or(0.0)
    });
    Some(num(1) * 3_600_000.0 + num(2) * 60_000.0 + num(3) * 1000.0 + ms)
}

/// Build a segment, or `None` if its timing is invalid or it has no content.
fn normalize_segment(
    start: Option<f64>,
    end: Option<f64>,
    text: &str,
    words: Vec<Word>,
) -> Option<Segment> {
    let (start, end) = (start?, end?);
    if !start.is_finite() || !end.is_finite() || end <= start {
        return None;
    }
    let safe_text = sanitize(text);
    if safe_text.is_empty() && words.is_empty() {
        return None;
    }
    let text = if safe_text.is_empty() {
        words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        safe_text
    };
    Some(Segment {
        start_ms: js_round(start).max(0.0) as i64,
        end_ms: js_round(end).max(0.0) as i64,
        duration_ms: js_round(end - start) as i64,
        text,
        words,
    })
}

/// Read a start or end time from a whisper segment/token object, trying the
/// field names used by the different whisper.cpp versions and output modes.
fn read_time(obj: &Value, which: Edge) -> Option<f64> {
    let (plain, ms_key, short) = match which {
        Edge::Start => ("start", "start_ms", "from"),
        Edge::End => ("end", "end_ms", "to"),
    };
    let value = [
        get(obj, plain),
        get(obj, ms_key),
        get(obj, short),
        get_path(obj, &["offsets", short]),
        get_path(obj, &["timestamps", short]),
    ]
    .into_iter()
    .find_map(nullish)?;

    // Explicit millisecond fields are used as-is; everything else is parsed.
    let explicit_ms = nullish(get(obj, ms_key)).is_some()
        || nullish(get_path(obj, &["offsets", short])).is_some();
    match value {
        Value::Number(n) if explicit_ms => n.as_f64(),
        other => parse_timestamp_ms(other),
    }
}

#[derive(Clone, Copy)]
enum Edge {
    Start,
    End,
}

/// Normalize one raw whisper word/token, keeping its text untouched.
pub fn normalize_raw_token(token: &Value) -> Option<RawToken> {
    let start_ms = read_time(token, Edge::Start);
    let end_ms = read_time(token, Edge::End);
    let raw = [get(token, "text"), get(token, "word"), get(token, "token")]
        .into_iter()
        .find_map(nullish)?;
    if is_falsy(raw) {
        return None;
    }
    let raw_text = js_string(raw);
    let (start_ms, end_ms) = (start_ms?, end_ms?);
    if !start_ms.is_finite() || !end_ms.is_finite() || end_ms <= start_ms {
        return None;
    }
    Some(RawToken {
        raw_text,
        start_ms,
        end_ms,
    })
}

/// Is this a whisper control token rather than speech?
///
/// Covers whisper.cpp's `[_BEG_]`, `[_TT_150]`, `[_SOT_]`, `[_EOT_]`, … and
/// the `<|endoftext|>` / `<|0.00|>` style used by OpenAI / Hugging Face.
pub fn is_special_token(text: &str) -> bool {
    let t = text.trim_matches(is_js_space);
    (t.starts_with("[_") && t.ends_with(']')) || (t.starts_with("<|") && t.ends_with("|>"))
}

/// Reassemble whisper's BPE tokens into whole words.
///
/// A token that starts a new word carries a leading space (`" Mul"`), a
/// continuation does not (`"do"`, `"on"`). Sanitizing tokens before merging
/// destroys that signal and produces `"M uld oon"` or `"friend ."`.
/// Special tokens (see [`is_special_token`]) are skipped.
///
/// ```
/// use whisper_captions::{merge_bpe_tokens, RawToken};
/// let t = |text: &str, start_ms: f64, end_ms: f64| RawToken { raw_text: text.into(), start_ms, end_ms };
/// let words = merge_bpe_tokens([t("[_BEG_]", 0.0, 10.0), t(" Mul", 0.0, 300.0), t("do", 300.0, 500.0), t("on", 500.0, 800.0)]);
/// assert_eq!(words[0].text, "Muldoon");
/// assert_eq!((words[0].start_ms, words[0].end_ms), (0, 800));
/// ```
pub fn merge_bpe_tokens(tokens: impl IntoIterator<Item = RawToken>) -> Vec<Word> {
    let mut merged: Vec<RawToken> = Vec::new();
    for token in tokens {
        if is_special_token(&token.raw_text) {
            continue;
        }
        let starts_with_space = token.raw_text.chars().next().is_some_and(is_js_space);
        match merged.last_mut() {
            Some(previous) if !starts_with_space => {
                previous.raw_text.push_str(&token.raw_text);
                previous.end_ms = token.end_ms;
            }
            _ => merged.push(RawToken {
                raw_text: trim_start_js(&token.raw_text).to_string(),
                ..token
            }),
        }
    }

    merged
        .into_iter()
        .filter_map(|w| {
            let text = sanitize(&w.raw_text);
            let start_ms = js_round(w.start_ms).max(0.0);
            let end_ms = js_round(w.end_ms).max(0.0);
            (!text.is_empty() && end_ms > start_ms).then_some(Word {
                text,
                start_ms: start_ms as i64,
                end_ms: end_ms as i64,
            })
        })
        .collect()
}

fn normalize_json_segment(segment: &Value) -> Option<Segment> {
    let tokens = [
        get(segment, "words"),
        get(segment, "tokens"),
        get_path(segment, &["result", "words"]),
    ]
    .into_iter()
    .flatten()
    .find_map(Value::as_array);
    let words = merge_bpe_tokens(tokens.into_iter().flatten().filter_map(normalize_raw_token));

    let start =
        read_time(segment, Edge::Start).or_else(|| words.first().map(|w| w.start_ms as f64));
    let end = read_time(segment, Edge::End).or_else(|| words.last().map(|w| w.end_ms as f64));
    let text = js_string_or_empty(get(segment, "text"));

    normalize_segment(start, end, &text, words)
}

/// Parse a whisper.cpp JSON result (any of the known layouts).
pub fn parse_whisper_json(json: &Value) -> Vec<Segment> {
    if is_falsy(json) {
        return Vec::new();
    }
    let candidates = [
        get(json, "transcription"),
        get(json, "segments"),
        get_path(json, &["result", "segments"]),
        get(json, "results"),
    ]
    .into_iter()
    .flatten()
    .find_map(Value::as_array);

    candidates
        .map(|list| list.iter().filter_map(normalize_json_segment).collect())
        .unwrap_or_default()
}

/// Parse a whisper.cpp JSON string. Invalid JSON yields an error.
pub fn parse_whisper_json_str(json: &str) -> Result<Vec<Segment>, serde_json::Error> {
    Ok(parse_whisper_json(&serde_json::from_str(json)?))
}

/// Parse whisper.cpp's human-readable stdout (`[start --> end] text` lines).
pub fn parse_whisper_stdout(stdout: &str) -> Vec<Segment> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        // `S` mirrors JavaScript's `\s` exactly (Rust's `\s` differs on U+0085 / U+FEFF).
        const S: &str = r"[\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]";
        const TS: &str = r"[0-9]{1,2}:[0-9]{2}:[0-9]{2}[.,][0-9]{1,3}";
        Regex::new(&format!(r"\[({TS}){S}*-->{S}*({TS})\]{S}*([^\n\r]+)")).unwrap()
    });

    re.captures_iter(stdout)
        .filter_map(|c| {
            normalize_segment(
                parse_timestamp_str(&c[1]),
                parse_timestamp_str(&c[2]),
                &c[3],
                Vec::new(),
            )
        })
        .collect()
}

/// Prefer JSON segments; fall back to stdout when JSON is missing or empty.
pub fn parse_transcript(json: Option<&Value>, stdout: Option<&str>) -> Vec<Segment> {
    let from_json = json.map(parse_whisper_json).unwrap_or_default();
    if !from_json.is_empty() {
        return from_json;
    }
    stdout.map(parse_whisper_stdout).unwrap_or_default()
}

fn format_timestamp(ms: i64, separator: char) -> String {
    let ms = ms.max(0);
    format!(
        "{:02}:{:02}:{:02}{separator}{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

/// Render segments as SubRip (`.srt`).
pub fn to_srt(segments: &[Segment]) -> String {
    segments
        .iter()
        .enumerate()
        .map(|(i, s)| {
            format!(
                "{}\n{} --> {}\n{}\n",
                i + 1,
                format_timestamp(s.start_ms, ','),
                format_timestamp(s.end_ms, ','),
                s.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render segments as WebVTT (`.vtt`).
pub fn to_vtt(segments: &[Segment]) -> String {
    let mut out = String::from("WEBVTT\n");
    for s in segments {
        out.push_str(&format!(
            "\n{} --> {}\n{}\n",
            format_timestamp(s.start_ms, '.'),
            format_timestamp(s.end_ms, '.'),
            s.text
        ));
    }
    out
}

#[cfg(test)]
mod tests;

/// Compiles and runs the README examples as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;
