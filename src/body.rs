//! ボディの組み立て(設計図 §6)。`a.b[0].c` / `a/b` / `a[b]` を nested JSON に。

use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Json(Value),
    Form(Vec<(String, String)>),
    Multipart(Vec<(String, String)>),
    File(String),
}

#[derive(Debug, Clone, PartialEq)]
enum Seg {
    Key(String),
    Index(usize),
}

/// `profile.first_name` → [Key(profile), Key(first_name)]、`tags[0]` → [Key(tags), Index(0)]、
/// `profile[first_name]` → [Key(profile), Key(first_name)]、`a/b` → [Key(a), Key(b)]。
fn segments(path: &str) -> Vec<Seg> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = path.chars().peekable();
    let flush = |cur: &mut String, out: &mut Vec<Seg>| {
        if !cur.is_empty() {
            out.push(Seg::Key(std::mem::take(cur)));
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '.' | '/' => flush(&mut cur, &mut out),
            '[' => {
                flush(&mut cur, &mut out);
                let mut inner = String::new();
                for c2 in chars.by_ref() {
                    if c2 == ']' {
                        break;
                    }
                    inner.push(c2);
                }
                if let Ok(n) = inner.parse::<usize>() {
                    out.push(Seg::Index(n));
                } else if !inner.is_empty() {
                    out.push(Seg::Key(inner));
                }
            }
            _ => cur.push(c),
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// `=` の値は文字列。`typed` なら JSON リテラルとして読み、読めなければ文字列。
pub fn parse_value(raw: &str, typed: bool) -> Value {
    if typed {
        serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
    } else {
        Value::String(raw.to_string())
    }
}

pub fn insert(root: &mut Value, path: &str, value: Value) {
    let segs = segments(path);
    if segs.is_empty() {
        return;
    }
    let mut cur = root;
    for (i, seg) in segs.iter().enumerate() {
        let last = i + 1 == segs.len();
        match seg {
            Seg::Key(k) => {
                if !cur.is_object() {
                    *cur = Value::Object(Map::new());
                }
                let map = cur.as_object_mut().unwrap();
                if last {
                    map.insert(k.clone(), value);
                    return;
                }
                cur = map.entry(k.clone()).or_insert(Value::Null);
            }
            Seg::Index(n) => {
                if !cur.is_array() {
                    *cur = Value::Array(Vec::new());
                }
                let arr = cur.as_array_mut().unwrap();
                while arr.len() <= *n {
                    arr.push(Value::Null);
                }
                if last {
                    arr[*n] = value;
                    return;
                }
                cur = &mut arr[*n];
            }
        }
    }
}

/// form 用: ネストを `profile[first_name]=job` の形に平坦化。
pub fn flatten(v: &Value, prefix: &str, out: &mut Vec<(String, String)>) {
    match v {
        Value::Object(m) => {
            for (k, v) in m {
                let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}[{k}]") };
                flatten(v, &key, out);
            }
        }
        Value::Array(a) => {
            for (i, v) in a.iter().enumerate() {
                flatten(v, &format!("{prefix}[{i}]"), out);
            }
        }
        Value::String(s) => out.push((prefix.to_string(), s.clone())),
        Value::Null => out.push((prefix.to_string(), String::new())),
        other => out.push((prefix.to_string(), other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nested_and_arrays() {
        let mut v = Value::Null;
        insert(&mut v, "profile.first_name", json!("job"));
        insert(&mut v, "profile.family_name", json!("amanda"));
        insert(&mut v, "tags[0]", json!("a"));
        insert(&mut v, "tags[1]", json!("b"));
        insert(&mut v, "price", parse_value("120", true));
        insert(&mut v, "zip", parse_value("01234", false));
        assert_eq!(
            v,
            json!({"profile":{"first_name":"job","family_name":"amanda"},"tags":["a","b"],"price":120,"zip":"01234"})
        );
    }

    #[test]
    fn bracket_and_slash_paths() {
        let mut v = Value::Null;
        insert(&mut v, "profile[first_name]", json!("job"));
        insert(&mut v, "a/b", json!(1));
        assert_eq!(v, json!({"profile":{"first_name":"job"},"a":{"b":1}}));
    }

    #[test]
    fn form_flatten() {
        let v = json!({"profile":{"first_name":"job"},"tags":["a"]});
        let mut out = Vec::new();
        flatten(&v, "", &mut out);
        assert_eq!(out, [("profile[first_name]".to_string(), "job".to_string()), ("tags[0]".to_string(), "a".to_string())]);
    }
}
