//! URL の正規化(設計図 §5)。スキーム補完、`:3000` 展開、path / port / query の結合。

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UrlParts {
    pub scheme: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    pub path: String,
    pub query: Vec<(String, String)>,
}

impl UrlParts {
    pub fn parse(raw: &str) -> UrlParts {
        let mut p = UrlParts::default();
        let rest = match raw.split_once("://") {
            Some((s, r)) => {
                p.scheme = Some(s.to_ascii_lowercase());
                r
            }
            None => raw,
        };
        let rest = if rest.starts_with(':') { format!("localhost{rest}") } else { rest.to_string() };
        let (hostport, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest.as_str(), ""),
        };
        let (path, q) = match path.split_once('?') {
            Some((a, b)) => (a, Some(b)),
            None => (path, None),
        };
        match hostport.rsplit_once(':') {
            Some((h, port)) if port.parse::<u16>().is_ok() => {
                p.host = h.to_string();
                p.port = port.parse().ok();
            }
            _ => p.host = hostport.to_string(),
        }
        p.path = path.to_string();
        if let Some(q) = q {
            for kv in q.split('&').filter(|s| !s.is_empty()) {
                let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
                p.query.push((k.to_string(), v.to_string()));
            }
        }
        p
    }

    pub fn push_path(&mut self, seg: &str) {
        let seg = seg.trim_matches('/');
        if seg.is_empty() {
            return;
        }
        if !self.path.ends_with('/') {
            self.path.push('/');
        }
        self.path.push_str(seg);
    }

    /// 決定(2026-09-22): https 既定、localhost / 127.0.0.1 / 私有 IP だけ http。
    fn default_scheme(&self) -> &'static str {
        let h = self.host.to_ascii_lowercase();
        let local = h == "localhost"
            || h.ends_with(".localhost")
            || h.ends_with(".local")
            || h.ends_with(".test")
            || h.ends_with(".internal")
            || h == "0.0.0.0"
            || is_private_ipv4(&h);
        if local { "http" } else { "https" }
    }

    pub fn render(&self) -> String {
        let scheme = self.scheme.clone().unwrap_or_else(|| self.default_scheme().to_string());
        let mut s = format!("{scheme}://{}", self.host);
        if let Some(p) = self.port {
            s.push_str(&format!(":{p}"));
        }
        if self.path.is_empty() {
            s.push('/');
        } else {
            if !self.path.starts_with('/') {
                s.push('/');
            }
            s.push_str(&self.path);
        }
        if !self.query.is_empty() {
            s.push('?');
            let q: Vec<String> = self.query.iter().map(|(k, v)| format!("{}={}", encode(k), encode(v))).collect();
            s.push_str(&q.join("&"));
        }
        s
    }
}

fn is_private_ipv4(h: &str) -> bool {
    let p: Vec<u8> = h.split('.').filter_map(|s| s.parse().ok()).collect();
    if p.len() != 4 {
        return false;
    }
    p[0] == 127 || p[0] == 10 || (p[0] == 172 && (16..=31).contains(&p[1])) || (p[0] == 192 && p[1] == 168)
}

/// クエリ用の最小限のパーセントエンコード。
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme_defaults() {
        assert_eq!(UrlParts::parse("localhost").render(), "http://localhost/");
        assert_eq!(UrlParts::parse(":3000/users").render(), "http://localhost:3000/users");
        assert_eq!(UrlParts::parse("example.com/x").render(), "https://example.com/x");
        assert_eq!(UrlParts::parse("192.168.1.2:8080").render(), "http://192.168.1.2:8080/");
        assert_eq!(UrlParts::parse("http://example.com").render(), "http://example.com/");
    }

    #[test]
    fn port_path_query() {
        let mut u = UrlParts::parse("localhost");
        u.port = Some(3000);
        u.push_path("users");
        u.query.push(("page".into(), "2".into()));
        u.query.push(("q".into(), "a b".into()));
        assert_eq!(u.render(), "http://localhost:3000/users?page=2&q=a+b");
    }
}
