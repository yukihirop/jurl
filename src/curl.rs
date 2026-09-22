//! Request → curl の argv。表示用の shell quote と、spawn(設計図 §8)。

use crate::assemble::Request;
use crate::body::Body;
use crate::error::JurlError;
use std::process::{Command, Stdio};

pub const STATUS_MARK: &str = "\n\u{1}jurl\u{1}";

pub fn argv(req: &Request, status: bool, passthrough: &[String], default_args: &[String]) -> Vec<String> {
    let mut a: Vec<String> = vec!["curl".into(), "-sS".into()];
    a.extend(default_args.iter().cloned());
    a.extend(req.curl_extra.iter().cloned());
    if req.method != "GET" {
        a.push("-X".into());
        a.push(req.method.clone());
    }
    a.push(req.url.render());
    if let Some(ct) = &req.content_type {
        if !req.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
            a.push("-H".into());
            a.push(format!("Content-Type: {ct}"));
        }
        if ct == "application/json" && !req.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("accept")) {
            a.push("-H".into());
            a.push("Accept: application/json".into());
        }
    }
    for (k, v) in &req.headers {
        a.push("-H".into());
        a.push(format!("{k}: {v}"));
    }
    match &req.body {
        Some(Body::Json(v)) => {
            a.push("--data".into());
            a.push(v.to_string());
        }
        Some(Body::Form(kv)) => {
            for (k, v) in kv {
                a.push("--data-urlencode".into());
                a.push(format!("{k}={v}"));
            }
        }
        Some(Body::Multipart(kv)) => {
            for (k, v) in kv {
                a.push("-F".into());
                a.push(format!("{k}={v}"));
            }
        }
        Some(Body::File(f)) => {
            a.push("--data-binary".into());
            a.push(format!("@{f}"));
        }
        None => {}
    }
    a.extend(passthrough.iter().cloned());
    with_status(a, status)
}

/// 出力整形用の `-w` を末尾に付ける(既に -w があれば付けない)。
pub fn with_status(mut a: Vec<String>, on: bool) -> Vec<String> {
    if on && !a.iter().any(|x| x == "-w" || x == "--write-out") {
        a.push("-w".into());
        a.push(format!("{STATUS_MARK}%{{http_code}} %{{time_total}}"));
    }
    a
}

/// `--dry-run` 用。1 引数 1 行、値を取るオプションはその値と同じ行、POSIX shell 用に quote。
#[cfg(test)]
pub fn render(argv: &[String]) -> String {
    render_with(argv, false)
}

/// 色付き: `curl` 太字、オプションはシアン、URL は太字青、値はそのまま。
pub fn render_with(argv: &[String], color: bool) -> String {
    use crate::color::{paint, paint2, C};
    let mut lines: Vec<String> = Vec::new();
    let mut expect_value = false;
    for (i, a) in argv.iter().enumerate() {
        let q = quote(a);
        let q = if i == 0 {
            paint(color, C::Bold, &q)
        } else if expect_value {
            q
        } else if a.starts_with('-') {
            paint(color, C::Cyan, &q)
        } else if a.contains("://") {
            paint2(color, C::Bold, C::Blue, &q)
        } else {
            q
        };
        if i == 0 {
            lines.push(q);
            continue;
        }
        if expect_value {
            let last = lines.last_mut().unwrap();
            last.push(' ');
            last.push_str(&q);
            expect_value = false;
            continue;
        }
        lines.push(q);
        expect_value = takes_value(a);
    }
    lines.join(" \\\n  ")
}

fn takes_value(opt: &str) -> bool {
    const OWN: &[&str] = &["-X", "-H", "--data", "--data-urlencode", "--data-binary", "-F", "-w"];
    OWN.contains(&opt) || crate::rules::CURL_FLAGS.iter().any(|(f, v)| *f == opt && *v)
}

pub fn quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '=' | '@' | '%' | '+' | ',')) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

pub struct Output {
    pub stdout: Vec<u8>,
    pub status: i32,
}

pub fn run(argv: &[String]) -> Result<Output, JurlError> {
    let out = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { JurlError::CurlMissing } else { JurlError::Io(e) })?;
    Ok(Output { stdout: out.stdout, status: out.status.code().unwrap_or(1) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble::assemble;
    use crate::rules::classify;

    #[test]
    fn dry_run_of_canonical_example() {
        let t = classify(&["post", "localhost", "application/json", "profile.first_name=job"].map(String::from));
        let r = assemble(&t, "application/json").unwrap();
        let s = render(&argv(&r, false, &[], &[]));
        assert_eq!(
            s,
            "curl \\\n  -sS \\\n  -X POST \\\n  http://localhost/ \\\n  -H 'Content-Type: application/json' \\\n  -H 'Accept: application/json' \\\n  --data '{\"profile\":{\"first_name\":\"job\"}}'"
        );
    }

    #[test]
    fn quoting() {
        assert_eq!(quote("abc"), "abc");
        assert_eq!(quote("it's"), "'it'\\''s'");
    }
}
