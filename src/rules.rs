//! 規則による役割分類(設計図 §4)。上から順に試し、決められないものは role = None のまま jev へ。

use crate::token::{Role, Token};

pub const METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

/// Content-Type の短縮名 → MIME。
pub const CT_SYNONYMS: &[(&str, &str)] = &[
    ("json", "application/json"),
    ("form", "application/x-www-form-urlencoded"),
    ("urlencoded", "application/x-www-form-urlencoded"),
    ("multipart", "multipart/form-data"),
    ("xml", "application/xml"),
    ("text", "text/plain"),
];

const MIME_TOP: &[&str] = &["application", "text", "multipart", "image", "audio", "video"];

/// スキーム無しで `host.tld` を URL と見なす TLD。これ以外(`profile.name` など)は jev に回す。
const KNOWN_TLDS: &[&str] = &[
    "com", "net", "org", "io", "dev", "app", "ai", "co", "jp", "me", "xyz", "info", "biz", "us", "uk", "de", "fr",
    "local", "localhost", "test", "internal", "lan", "home", "example", "invalid",
];

/// curl のオプション表: (名前, 値を取るか)。`-H` と `-X` は jurl 側で意味を持つので別扱い。
pub const CURL_FLAGS: &[(&str, bool)] = &[
    ("-k", false), ("--insecure", false),
    ("-v", false), ("--verbose", false),
    ("-L", false), ("--location", false),
    ("-i", false), ("--include", false),
    ("-I", false), ("--head", false),
    ("-s", false), ("--silent", false), ("-S", false), ("--show-error", false),
    ("-f", false), ("--fail", false), ("--fail-with-body", false),
    ("--compressed", false), ("--http1.1", false), ("--http2", false), ("--http3", false),
    ("-4", false), ("-6", false), ("-N", false), ("--no-buffer", false),
    ("-m", true), ("--max-time", true), ("--connect-timeout", true),
    ("-u", true), ("--user", true),
    ("-A", true), ("--user-agent", true),
    ("-b", true), ("--cookie", true), ("-c", true), ("--cookie-jar", true),
    ("-e", true), ("--referer", true),
    ("-x", true), ("--proxy", true),
    ("--retry", true), ("--retry-delay", true),
    ("-o", true), ("--output", true), ("-D", true), ("--dump-header", true),
    ("-w", true), ("--write-out", true),
    ("--cacert", true), ("--cert", true), ("--key", true),
    ("--resolve", true), ("--interface", true), ("--unix-socket", true),
    ("-d", true), ("--data", true), ("--data-raw", true), ("--data-binary", true), ("--data-urlencode", true),
    ("-F", true), ("--form", true),
    ("-T", true), ("--upload-file", true),
];

pub fn classify(words: &[String]) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(words.len());
    let mut i = 0;
    let mut have_method = false;
    let mut have_url = false;
    while i < words.len() {
        let w = &words[i];
        let mut t = Token::new(w.clone());

        // -H "Name: value" / -X POST は次のトークンごと jurl の意味に読み替える。
        if w == "-H" || w == "--header" {
            if let Some(next) = words.get(i + 1) {
                t.set_rule(Role::Noise);
                t.note = Some("header flag".into());
                out.push(t);
                let mut h = Token::new(next.clone());
                h.set_rule(Role::Header);
                out.push(h);
                i += 2;
                continue;
            }
        }
        if w == "-X" || w == "--request" {
            if let Some(next) = words.get(i + 1) {
                t.set_rule(Role::Noise);
                t.note = Some("method flag".into());
                out.push(t);
                let mut m = Token::new(next.clone());
                m.set_rule(Role::Method);
                m.fixed = Some(next.to_uppercase());
                out.push(m);
                have_method = true;
                i += 2;
                continue;
            }
        }

        if let Some((_, takes_value)) = CURL_FLAGS.iter().find(|(f, _)| f == w) {
            t.set_rule(Role::CurlFlag);
            out.push(t);
            if *takes_value {
                if let Some(next) = words.get(i + 1) {
                    let mut v = Token::new(next.clone());
                    v.set_rule(Role::CurlFlagValue);
                    out.push(v);
                    i += 2;
                    continue;
                }
            }
            i += 1;
            continue;
        }
        if w.starts_with('-') && w.len() > 1 && !w.contains('=') {
            // 知らない curl オプション。値を取るかは分からないので単独で素通し。
            t.set_rule(Role::CurlFlag);
            t.note = Some("unknown curl option, passed through".into());
            out.push(t);
            i += 1;
            continue;
        }

        if !have_method && METHODS.contains(&w.to_ascii_lowercase().as_str()) {
            t.set_rule(Role::Method);
            t.fixed = Some(w.to_ascii_uppercase());
            have_method = true;
        } else if w.contains(":=") && is_path(w.split(":=").next().unwrap_or("")) {
            t.set_rule(Role::Field);
            t.typed = true;
        } else if w.contains("==") && is_path(w.split("==").next().unwrap_or("")) {
            t.set_rule(Role::Query);
        } else if let Some((k, _)) = w.split_once('=') {
            if is_path(k) {
                t.set_rule(Role::Field);
            }
        } else if w.starts_with('@') && w.len() > 1 {
            t.set_rule(Role::BodyFile);
        } else if let Some(ct) = content_type(w) {
            t.set_rule(Role::ContentType);
            t.fixed = Some(ct);
        } else if is_header(w) {
            t.set_rule(Role::Header);
        } else if !have_url && looks_like_url(w) {
            t.set_rule(Role::Url);
            have_url = true;
        } else if let Some(stem) = w.strip_suffix(':') {
            // `first_name: job` のように分かれたキー。
            if is_ident(stem) {
                t.set_rule(Role::FieldKey);
                t.fixed = Some(stem.to_string());
            }
        } else if w == "=" || w == ":" {
            t.set_rule(Role::Noise);
        }

        out.push(t);
        i += 1;
    }
    adjacency_pass(&mut out);
    out
}

/// 隣接だけで決まるもの: URL の直後の裸の数字はポート。
fn adjacency_pass(tokens: &mut [Token]) {
    for i in 1..tokens.len() {
        if tokens[i].resolved() {
            continue;
        }
        let prev_is_bare_host = tokens[i - 1].role == Some(Role::Url) && host_has_no_port_or_path(&tokens[i - 1].text);
        if prev_is_bare_host && is_port(&tokens[i].text) {
            tokens[i].set_rule(Role::Port);
        }
    }
}

fn host_has_no_port_or_path(u: &str) -> bool {
    let u = u.split("://").last().unwrap_or(u);
    !u.contains(':') && !u.contains('/')
}

pub fn is_port(s: &str) -> bool {
    !s.is_empty() && s.len() <= 5 && s.bytes().all(|b| b.is_ascii_digit()) && s.parse::<u32>().map(|n| n > 0 && n <= 65535).unwrap_or(false)
}

pub fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// ボディのパス: `a.b`, `a[0].b`, `a/b`。
pub fn is_path(s: &str) -> bool {
    !s.is_empty()
        && (s.chars().next().unwrap().is_ascii_alphabetic() || s.starts_with('_'))
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '[' | ']' | '/'))
}

pub fn content_type(w: &str) -> Option<String> {
    let lw = w.to_ascii_lowercase();
    if let Some((_, mime)) = CT_SYNONYMS.iter().find(|(k, _)| *k == lw) {
        return Some((*mime).to_string());
    }
    let (top, sub) = lw.split_once('/')?;
    if MIME_TOP.contains(&top) && !sub.is_empty() && sub.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '+' | '.')) {
        return Some(lw);
    }
    None
}

pub fn is_header(w: &str) -> bool {
    let Some((name, value)) = w.split_once(':') else { return false };
    if name.is_empty() || value.is_empty() || w.contains("://") {
        return false;
    }
    if !name.chars().next().unwrap().is_ascii_alphabetic() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return false;
    }
    // `localhost:3000` は URL。ヘッダ値が数字で始まることは稀なので数字始まりは除外。
    !value.chars().next().unwrap().is_ascii_digit()
}

pub fn looks_like_url(w: &str) -> bool {
    if w.contains("://") {
        return true;
    }
    if w.starts_with(':') && w.len() > 1 && w[1..].chars().next().unwrap().is_ascii_digit() {
        return true; // :3000/users
    }
    let host = w.split('/').next().unwrap_or("");
    let (host, port) = match host.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => (h, Some(p)),
        _ => (host, None),
    };
    if host == "localhost" || is_ipv4(host) {
        return true;
    }
    if !host.contains('.') || !host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return false;
    }
    let tld = host.rsplit('.').next().unwrap_or("");
    // 既知の TLD か、ポート/パスが付いていれば URL。
    KNOWN_TLDS.contains(&tld.to_ascii_lowercase().as_str()) || port.is_some() || w.contains('/')
}

fn is_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 4 && parts.iter().all(|p| !p.is_empty() && p.parse::<u8>().is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roles(words: &[&str]) -> Vec<Option<Role>> {
        classify(&words.iter().map(|s| s.to_string()).collect::<Vec<_>>()).iter().map(|t| t.role).collect()
    }

    #[test]
    fn clean_input_is_fully_resolved() {
        let r = roles(&["post", "localhost", "application/json", "profile.first_name=job", "profile.family_name=amanda"]);
        assert_eq!(r, [Some(Role::Method), Some(Role::Url), Some(Role::ContentType), Some(Role::Field), Some(Role::Field)]);
    }

    #[test]
    fn order_does_not_matter() {
        let r = roles(&["profile.first_name=job", "localhost", "json", "POST"]);
        assert_eq!(r, [Some(Role::Field), Some(Role::Url), Some(Role::ContentType), Some(Role::Method)]);
    }

    #[test]
    fn localhost_with_port_is_url_not_header() {
        assert_eq!(roles(&["localhost:3000"]), [Some(Role::Url)]);
        assert_eq!(roles(&[":3000/users"]), [Some(Role::Url)]);
        assert_eq!(roles(&["Authorization:Bearer x"]), [Some(Role::Header)]);
    }

    #[test]
    fn bare_port_after_host() {
        assert_eq!(roles(&["localhost", "3000"]), [Some(Role::Url), Some(Role::Port)]);
    }

    #[test]
    fn unknown_words_stay_unresolved() {
        assert_eq!(roles(&["psot", "localhsot", "users"]), [None, None, None]);
        assert_eq!(roles(&["profile.name"]), [None]);
    }

    #[test]
    fn curl_flags_pass_through_with_values() {
        let r = roles(&["-k", "--max-time", "5", "localhost"]);
        assert_eq!(r, [Some(Role::CurlFlag), Some(Role::CurlFlag), Some(Role::CurlFlagValue), Some(Role::Url)]);
    }

    #[test]
    fn dash_h_and_dash_x() {
        let t = classify(&["-X", "put", "-H", "X-Foo: 1"].map(String::from));
        assert_eq!(t[1].role, Some(Role::Method));
        assert_eq!(t[1].value(), "PUT");
        assert_eq!(t[3].role, Some(Role::Header));
    }

    #[test]
    fn typed_and_query() {
        let t = classify(&["price:=120", "page==2"].map(String::from));
        assert_eq!(t[0].role, Some(Role::Field));
        assert!(t[0].typed);
        assert_eq!(t[1].role, Some(Role::Query));
    }
}
