//! 役割付きトークン列 → Request(設計図 §5 隣接結合 + §6 組み立て)。

use crate::body::{self, Body};
use crate::error::JurlError;
use crate::token::{Role, Token};
use crate::url::UrlParts;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub url: UrlParts,
    pub headers: Vec<(String, String)>,
    pub content_type: Option<String>,
    pub body: Option<Body>,
    pub curl_extra: Vec<String>,
    /// 解釈全体の最小 confidence(規則で決めたものは 1.0)。
    pub confidence: f32,
}

pub fn assemble(tokens: &[Token], default_content_type: &str) -> Result<Request, JurlError> {
    let unresolved: Vec<&str> = tokens.iter().filter(|t| !t.resolved()).map(|t| t.text.as_str()).collect();
    if !unresolved.is_empty() {
        return Err(JurlError::Unresolved(unresolved.join(", ")));
    }

    let mut method: Option<String> = None;
    let mut url: Option<UrlParts> = None;
    let mut ports: Vec<u16> = Vec::new();
    let mut paths: Vec<String> = Vec::new();
    let mut content_type: Option<String> = None;
    let mut headers = Vec::new();
    let mut fields: Vec<(String, Value)> = Vec::new();
    let mut query: Vec<(String, String)> = Vec::new();
    let mut body_file: Option<String> = None;
    let mut curl_extra = Vec::new();
    let mut confidence: f32 = 1.0;
    let mut conflicts = Vec::new();

    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        confidence = confidence.min(t.confidence);
        match t.role.unwrap() {
            Role::Method => match &method {
                None => method = Some(t.value().to_string()),
                Some(m) => conflicts.push(format!("method {m} and {}", t.value())),
            },
            Role::Url => match &url {
                None => url = Some(UrlParts::parse(t.value())),
                Some(u) => conflicts.push(format!("url {} and {}", u.host, t.value())),
            },
            Role::Port => {
                if let Ok(p) = t.text.parse::<u16>() {
                    ports.push(p);
                }
            }
            Role::UrlPath => paths.push(t.text.clone()),
            Role::ContentType => match &content_type {
                None => content_type = Some(t.value().to_string()),
                Some(c) => conflicts.push(format!("content-type {c} and {}", t.value())),
            },
            Role::Field => {
                let (k, v) = if t.typed {
                    t.text.split_once(":=").unwrap_or((&t.text, ""))
                } else {
                    t.text.split_once('=').unwrap_or((&t.text, ""))
                };
                if let Some(f) = v.strip_prefix('@') {
                    fields.push((k.to_string(), Value::String(format!("@{f}"))));
                } else {
                    fields.push((k.to_string(), body::parse_value(v, t.typed)));
                }
            }
            Role::FieldKey => {
                // 次の FieldValue と結合。無ければ空文字。
                let key = t.value().to_string();
                let next = tokens.get(i + 1);
                let is_val = next.map(|n| n.role == Some(Role::FieldValue)).unwrap_or(false);
                if is_val {
                    let n = next.unwrap();
                    confidence = confidence.min(n.confidence);
                    fields.push((key, body::parse_value(n.value(), n.typed)));
                    i += 2;
                    continue;
                }
                fields.push((key, Value::String(String::new())));
            }
            Role::FieldValue => {
                // 直前が FieldKey ならそこで消費済み。単独で来た値は捨てずに警告に近い扱い。
                conflicts.push(format!("value \"{}\" has no key", t.text));
            }
            Role::Query => {
                if let Some((k, v)) = t.text.split_once("==") {
                    query.push((k.to_string(), v.to_string()));
                } else {
                    // 裸のクエリキー。次が値なら結合。
                    let next = tokens.get(i + 1);
                    if next.map(|n| n.role == Some(Role::FieldValue)).unwrap_or(false) {
                        let n = next.unwrap();
                        confidence = confidence.min(n.confidence);
                        query.push((t.value().to_string(), n.value().to_string()));
                        i += 2;
                        continue;
                    }
                    query.push((t.value().to_string(), String::new()));
                }
            }
            Role::Header => {
                let (k, v) = t.text.split_once(':').unwrap_or((&t.text, ""));
                headers.push((k.trim().to_string(), v.trim().to_string()));
            }
            Role::BodyFile => body_file = Some(t.text[1..].to_string()),
            Role::CurlFlag | Role::CurlFlagValue => curl_extra.push(t.text.clone()),
            Role::Noise => {}
        }
        i += 1;
    }

    if !conflicts.is_empty() {
        return Err(JurlError::Conflict(conflicts.join("; ")));
    }
    let mut url = url.ok_or_else(|| JurlError::Unresolved("no url".into()))?;
    if let Some(p) = ports.first() {
        if url.port.is_none() {
            url.port = Some(*p);
        }
    }
    for p in &paths {
        url.push_path(p);
    }
    url.query.extend(query);

    // 決定(2026-09-22): METHOD 省略時はボディがあれば POST、無ければ GET。
    let has_body = !fields.is_empty() || body_file.is_some();
    let method = method.unwrap_or_else(|| if has_body { "POST".into() } else { "GET".into() });

    let ct = content_type.clone().unwrap_or_else(|| default_content_type.to_string());
    let body = if let Some(f) = body_file {
        Some(Body::File(f))
    } else if fields.is_empty() {
        None
    } else if ct.starts_with("multipart/") {
        Some(Body::Multipart(fields.iter().map(|(k, v)| (k.clone(), scalar(v))).collect()))
    } else if ct == "application/x-www-form-urlencoded" {
        let mut root = Value::Null;
        for (k, v) in &fields {
            body::insert(&mut root, k, v.clone());
        }
        let mut flat = Vec::new();
        body::flatten(&root, "", &mut flat);
        Some(Body::Form(flat))
    } else {
        let mut root = Value::Null;
        for (k, v) in &fields {
            body::insert(&mut root, k, v.clone());
        }
        Some(Body::Json(root))
    };
    let content_type = if body.is_some() { Some(ct) } else { content_type };

    Ok(Request { method, url, headers, content_type, body, curl_extra, confidence })
}

fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::classify;
    use serde_json::json;

    fn req(words: &[&str]) -> Request {
        let tokens = classify(&words.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assemble(&tokens, "application/json").unwrap()
    }

    #[test]
    fn the_canonical_example() {
        let r = req(&["post", "localhost", "application/json", "profile.first_name=job", "profile.family_name=amanda"]);
        assert_eq!(r.method, "POST");
        assert_eq!(r.url.render(), "http://localhost/");
        assert_eq!(r.body, Some(Body::Json(json!({"profile":{"first_name":"job","family_name":"amanda"}}))));
        assert_eq!(r.confidence, 1.0);
    }

    #[test]
    fn get_with_query_and_header() {
        let r = req(&[":3000/users", "page==2", "Authorization:Bearer tok"]);
        assert_eq!(r.method, "GET");
        assert_eq!(r.url.render(), "http://localhost:3000/users?page=2");
        assert_eq!(r.headers, [("Authorization".to_string(), "Bearer tok".to_string())]);
        assert!(r.body.is_none());
    }

    #[test]
    fn form_and_flags() {
        let r = req(&["localhost/login", "form", "user=me", "pass=x", "-k"]);
        assert_eq!(r.method, "POST");
        assert_eq!(r.body, Some(Body::Form(vec![("user".into(), "me".into()), ("pass".into(), "x".into())])));
        assert_eq!(r.curl_extra, ["-k"]);
    }

    #[test]
    fn split_key_value_and_port() {
        let mut tokens = classify(&["localhost", "3000", "users", "first_name", "job"].map(String::from));
        // jev が返したつもりの分類
        tokens[2].role = Some(Role::UrlPath);
        tokens[2].confidence = 0.8;
        tokens[3].role = Some(Role::FieldKey);
        tokens[3].confidence = 0.9;
        tokens[4].role = Some(Role::FieldValue);
        tokens[4].confidence = 0.9;
        let r = assemble(&tokens, "application/json").unwrap();
        assert_eq!(r.url.render(), "http://localhost:3000/users");
        assert_eq!(r.body, Some(Body::Json(json!({"first_name":"job"}))));
        assert!((r.confidence - 0.8).abs() < 1e-6);
    }

    #[test]
    fn conflicts_are_errors() {
        let tokens = classify(&["localhost", "example.com"].map(String::from));
        // 2 つ目は規則で URL にならない(have_url)ので未解決 → Unresolved
        assert!(matches!(assemble(&tokens, "application/json"), Err(JurlError::Unresolved(_))));
    }
}
