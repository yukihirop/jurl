//! `jurl demo`: public-apis の認証不要 API を叩く例を並べて、番号で選んで実行する。
//! 例は EXAMPLES.md と同じもの(2026-09-22 に実機で通したもの)。

use crate::color::{self, paint, C};
use crate::error::JurlError;
use std::io::{IsTerminal, Write};

pub struct Example {
    pub what: &'static str,
    pub words: &'static [&'static str],
}

/// 前半は規則だけで決まる入力(jev を呼ばない)、後半は jev が役割を決める崩れた入力。
pub const EXAMPLES: &[Example] = &[
    Example { what: "GET: random dog picture", words: &["dog.ceo/api/breeds/image/random"] },
    Example { what: "GET: cat fact", words: &["catfact.ninja/fact"] },
    Example { what: "GET with query (k==v)", words: &["api.agify.io", "name==michael"] },
    Example { what: "GET with several query params", words: &["api.open-meteo.com/v1/forecast", "latitude==35.68", "longitude==139.69", "current_weather==true"] },
    Example { what: "GET, curl -L passes through (follows 301)", words: &["restcountries.com/v3.1/name/japan", "fields==name,capital", "-L"] },
    Example { what: "POST nested JSON (a.b=v)", words: &["post", "httpbin.org/post", "json", "profile.first_name=job", "profile.family_name=amanda"] },
    Example { what: "PUT with JSON literal (k:=v)", words: &["put", "jsonplaceholder.typicode.com/posts/1", "id:=1", "title=changed"] },
    Example { what: "jev: key and value split, no method → GET + query", words: &["api.agify.io", "name", "michael"] },
    Example { what: "jev: split path, numbers and booleans typed", words: &["api.open-meteo.com/v1", "forecast", "latitude", "35.68", "longitude", "139.69", "current_weather", "true"] },
    Example { what: "jev: typo `psot`, split key/values → POST JSON", words: &["httpbin.org/anything", "psot", "user", "me", "role", "admin"] },
    Example { what: "jev: method last, userId 1 sent as a number", words: &["jsonplaceholder.typicode.com/posts", "title", "hello", "body", "world", "userId", "1", "post"] },
    Example { what: "jev: two-word value breaks → press e and fix --data", words: &["httpbin.org/anything", "post", "title", "hello", "world"] },
];

pub fn menu() {
    let on = color::stderr_enabled();
    let mut e = std::io::stderr().lock();
    let _ = writeln!(e, "{}", paint(on, C::Bold, "jurl demo — public APIs, no auth needed"));
    for (i, ex) in EXAMPLES.iter().enumerate() {
        if i == 7 {
            let _ = writeln!(e, "{}", paint(on, C::Dim, "  -- the rest need jev (OPENROUTER_API_KEY) --"));
        }
        let _ = writeln!(
            e,
            "  {}  {:<52} {}",
            paint(on, C::Cyan, &format!("{:>2}", i + 1)),
            ex.what,
            paint(on, C::Dim, &format!("jurl {}", join(ex.words)))
        );
    }
}

/// 番号 → 例。`jurl demo 3` の引数か、メニューで打った文字列。
pub fn pick(s: &str) -> Result<&'static Example, JurlError> {
    let n: usize = s.trim().parse().map_err(|_| JurlError::Usage(format!("demo: expected a number 1-{}, got `{s}`", EXAMPLES.len())))?;
    EXAMPLES.get(n.wrapping_sub(1)).ok_or_else(|| JurlError::Usage(format!("demo: no example {n} (1-{})", EXAMPLES.len())))
}

/// メニューを出して番号を読む。q / 空行 / EOF で None。
pub fn ask() -> Result<Option<&'static Example>, JurlError> {
    if !std::io::stdin().is_terminal() {
        return Err(JurlError::Usage(format!("demo: not a terminal. pick one directly: jurl demo <1-{}>", EXAMPLES.len())));
    }
    let on = color::stderr_enabled();
    loop {
        menu();
        eprint!("{} {} ", paint(on, C::Yellow, "which one?"), paint(on, C::Dim, &format!("[1-{}, q]", EXAMPLES.len())));
        let _ = std::io::stderr().flush();
        let mut s = String::new();
        if std::io::stdin().read_line(&mut s)? == 0 {
            return Ok(None);
        }
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("q") {
            return Ok(None);
        }
        match pick(s) {
            Ok(ex) => return Ok(Some(ex)),
            Err(e) => eprintln!("jurl: {e}\n"),
        }
    }
}

/// 表示用: `k=v` や `a.b` をクォートしない shell 風の結合。
pub fn join<S: AsRef<str>>(words: &[S]) -> String {
    words.iter().map(|w| crate::curl::quote(w.as_ref())).collect::<Vec<_>>().join(" ")
}
