// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! JSON text in the exact layout of Foundation's `JSONEncoder` with
//! `[.prettyPrinted, .sortedKeys]`: two-space indent, `"key" : value`, `/` escaped as `\/`,
//! integral doubles written without a fraction, keys sorted by code point.
//! This lets the Rust core write `library.json` and commit manifests that are byte-compatible
//! with the Swift app (plan §5).

use serde_json::Value;

/// Renders `v` the way Swift's pretty, sorted `JSONEncoder` does.
pub fn to_string(v: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, v, 0);
    out
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn write_value(out: &mut String, v: &Value, level: usize) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => write_number(out, n),
        Value::String(s) => write_string(out, s),
        Value::Array(a) => {
            if a.is_empty() {
                out.push_str("[\n\n");
                indent(out, level);
                out.push(']');
                return;
            }
            out.push_str("[\n");
            for (i, item) in a.iter().enumerate() {
                indent(out, level + 1);
                write_value(out, item, level + 1);
                if i + 1 < a.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            indent(out, level);
            out.push(']');
        }
        Value::Object(m) => {
            if m.is_empty() {
                out.push_str("{\n\n");
                indent(out, level);
                out.push('}');
                return;
            }
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push_str("{\n");
            for (i, k) in keys.iter().enumerate() {
                indent(out, level + 1);
                write_string(out, k);
                out.push_str(" : ");
                write_value(out, &m[k.as_str()], level + 1);
                if i + 1 < keys.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            indent(out, level);
            out.push('}');
        }
    }
}

fn write_number(out: &mut String, n: &serde_json::Number) {
    if let Some(f) = n.as_f64().filter(|_| n.is_f64()) {
        if f.fract() == 0.0 && f.abs() < 1e15 {
            out.push_str(&format!("{}", f as i64));
        } else {
            out.push_str(&format!("{f}"));
        }
    } else {
        out.push_str(&n.to_string());
    }
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '/' => out.push_str("\\/"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn layout_matches_foundation() {
        let v = json!({"b": [1, 2], "a": "x/y", "c": {}, "d": [], "e": 36.0, "f": 1.5, "g": null, "h": true, "i": false});
        let s = to_string(&v);
        assert_eq!(
            s,
            "{\n  \"a\" : \"x\\/y\",\n  \"b\" : [\n    1,\n    2\n  ],\n  \"c\" : {\n\n  },\n  \"d\" : [\n\n  ],\n  \"e\" : 36,\n  \"f\" : 1.5,\n  \"g\" : null,\n  \"h\" : true,\n  \"i\" : false\n}"
        );
        let back: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(back["a"], "x/y");
    }

    #[test]
    fn escapes() {
        let s = to_string(&json!("q\"b\\n\nr\rt\tb\u{08}f\u{0C}c\u{01}é"));
        assert_eq!(s, "\"q\\\"b\\\\n\\nr\\rt\\tb\\bf\\fc\\u0001é\"");
        assert_eq!(to_string(&json!(-3)), "-3");
        assert_eq!(
            to_string(&json!(18446744073709551615u64)),
            "18446744073709551615"
        );
        assert_eq!(to_string(&json!(1e20)), "100000000000000000000");
    }
}
