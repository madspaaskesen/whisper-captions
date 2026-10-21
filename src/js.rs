//! Helpers that follow JavaScript semantics (`Math.round`, `\s`, truthiness,
//! `String()`). The crate started as a port of a JS implementation and keeps
//! its output byte-identical.

use serde_json::Value;

/// JavaScript's `\s` / `String.prototype.trim` whitespace set.
/// (Differs from `char::is_whitespace`: JS includes U+FEFF, excludes U+0085.)
pub fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// `String(text).replace(/\s+/g, " ").trim()`
pub fn sanitize(text: &str) -> String {
    text.split(is_js_space)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `text.replace(/^\s+/, "")`
pub fn trim_start_js(text: &str) -> &str {
    text.trim_start_matches(is_js_space)
}

/// `Math.round` (rounds .5 towards +∞, unlike `f64::round`).
pub fn js_round(x: f64) -> f64 {
    let floor = x.floor();
    if x - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// JavaScript truthiness for JSON values.
pub fn is_falsy(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        Value::Array(_) | Value::Object(_) => false,
    }
}

/// `String(value)` for JSON values.
pub fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => match (n.as_i64(), n.as_u64(), n.as_f64()) {
            (Some(i), _, _) => i.to_string(),
            (_, Some(u), _) => u.to_string(),
            (_, _, Some(f)) if f.fract() == 0.0 && f.abs() < 1e21 => format!("{f:.0}"),
            (_, _, Some(f)) => f.to_string(),
            _ => n.to_string(),
        },
        Value::String(s) => s.clone(),
        // Array.prototype.join: null/undefined elements become "".
        Value::Array(items) => items
            .iter()
            .map(|v| {
                if v.is_null() {
                    String::new()
                } else {
                    js_string(v)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

/// `String(value || "")`
pub fn js_string_or_empty(value: Option<&Value>) -> String {
    match value {
        Some(v) if !is_falsy(v) => js_string(v),
        _ => String::new(),
    }
}

/// `obj?.[key]` — only objects have properties; everything else is `undefined`.
pub fn get<'a>(obj: &'a Value, key: &str) -> Option<&'a Value> {
    obj.as_object()?.get(key)
}

/// `obj?.a?.b`
pub fn get_path<'a>(obj: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(obj, |current, key| get(current, key))
}

/// Treat JSON `null` like `undefined` for `??` / `!= null` checks.
pub fn nullish(value: Option<&Value>) -> Option<&Value> {
    value.filter(|v| !v.is_null())
}
