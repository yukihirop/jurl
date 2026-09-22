//! `jurl demo`: public-apis の認証不要 API を叩く例を並べて、番号で選んで実行する。
//! 例は EXAMPLES.md と同じもの(2026-09-22 に実機で通したもの)。

use crate::color::{self, paint, paint2, C};
use crate::error::JurlError;
use std::io::{IsTerminal, Write};

pub struct Example {
    pub what: &'static str,
    pub words: &'static [&'static str],
    /// jev が役割を決める例か(false なら規則だけで決まり、オフラインで即実行)。
    pub jev: bool,
}

/// 前半は規則だけで決まる入力(jev を呼ばない)、後半は jev が役割を決める崩れた入力。
pub const EXAMPLES: &[Example] = &[
    Example { what: "GET: random dog picture", words: &["dog.ceo/api/breeds/image/random"], jev: false },
    Example { what: "GET: cat fact", words: &["catfact.ninja/fact"], jev: false },
    Example { what: "GET with query (k==v)", words: &["api.agify.io", "name==michael"], jev: false },
    Example { what: "GET with several query params", words: &["api.open-meteo.com/v1/forecast", "latitude==35.68", "longitude==139.69", "current_weather==true"], jev: false },
    Example { what: "GET, curl -L passes through (follows 301)", words: &["restcountries.com/v3.1/name/japan", "fields==name,capital", "-L"], jev: false },
    Example { what: "POST nested JSON (a.b=v)", words: &["post", "httpbin.org/post", "json", "profile.first_name=job", "profile.family_name=amanda"], jev: false },
    Example { what: "PUT with JSON literal (k:=v)", words: &["put", "jsonplaceholder.typicode.com/posts/1", "id:=1", "title=changed"], jev: false },
    Example { what: "key and value split, no method → GET + query", words: &["api.agify.io", "name", "michael"], jev: true },
    Example { what: "split path, numbers and booleans typed", words: &["api.open-meteo.com/v1", "forecast", "latitude", "35.68", "longitude", "139.69", "current_weather", "true"], jev: true },
    Example { what: "typo `psot`, split key/values → POST JSON", words: &["httpbin.org/anything", "psot", "user", "me", "role", "admin"], jev: true },
    Example { what: "method last, userId 1 sent as a number", words: &["jsonplaceholder.typicode.com/posts", "title", "hello", "body", "world", "userId", "1", "post"], jev: true },
    Example { what: "two-word value joined into one (\"hello world\")", words: &["httpbin.org/anything", "post", "title", "hello", "world"], jev: true },
];

/// 1 行分の表示(番号・説明・コマンド)。`sel` の行は `>` と反転で目立たせる。
fn line(i: usize, sel: bool, on: bool, cols: usize) -> String {
    let ex = &EXAMPLES[i];
    let mark = if sel { ">" } else { " " };
    let tag = if ex.jev { "jev " } else { "rule" };
    let plain = format!("{mark} {:>2}  {tag}  {:<48} jurl {}", i + 1, ex.what, join(ex.words));
    let plain: String = plain.chars().take(cols.saturating_sub(1)).collect();
    if sel {
        // 反転 + 太字。色なしなら `>` だけで示す。
        if on { format!("\x1b[1;7m{plain}\x1b[0m") } else { plain }
    } else {
        // 番号はシアン、rule/jev は --explain の by 列と同じ緑/青、コマンドは薄く。
        let n_end = 5.min(plain.len());
        let tag_end = (n_end + 6).min(plain.len());
        let rest = &plain[tag_end..];
        let cmd_at = rest.find(" jurl ").map(|p| p + 1).unwrap_or(rest.len());
        format!(
            "{}{}{}{}",
            paint(on, C::Cyan, &plain[..n_end]),
            paint(on, if ex.jev { C::Blue } else { C::Green }, &plain[n_end..tag_end]),
            &rest[..cmd_at],
            paint(on, C::Dim, &rest[cmd_at..])
        )
    }
}

fn draw(sel: usize, on: bool, cols: usize, redraw: bool) {
    let mut e = std::io::stderr().lock();
    let rows = EXAMPLES.len();
    if redraw {
        let _ = write!(e, "\x1b[{rows}A");
    }
    for i in 0..EXAMPLES.len() {
        let _ = writeln!(e, "\x1b[2K{}", line(i, i == sel, on, cols));
    }
    let _ = e.flush();
}

/// 番号 → 例。`jurl demo 3` の引数か、メニューで打った文字列。
pub fn pick(s: &str) -> Result<&'static Example, JurlError> {
    let n: usize = s.trim().parse().map_err(|_| JurlError::Usage(format!("demo: expected a number 1-{}, got `{s}`", EXAMPLES.len())))?;
    EXAMPLES.get(n.wrapping_sub(1)).ok_or_else(|| JurlError::Usage(format!("demo: no example {n} (1-{})", EXAMPLES.len())))
}

/// 端末を raw(行編集・エコー・シグナルなし)にして、drop で戻す。
/// ISIG も切る: SIGINT で死ぬと drop が走らずエコー無しの端末が残るので、Ctrl-C は read_key で quit にする。
struct Raw {
    orig: libc::termios,
}

impl Raw {
    fn enable() -> std::io::Result<Raw> {
        // SAFETY: termios は POD。fd 0 が端末であることは呼ぶ側が確認済み。
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(0, &mut t) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            let orig = t;
            t.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG);
            t.c_cc[libc::VMIN] = 1;
            t.c_cc[libc::VTIME] = 0;
            if libc::tcsetattr(0, libc::TCSANOW, &t) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(Raw { orig })
        }
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        // SAFETY: enable() で取った値をそのまま戻すだけ。
        unsafe {
            libc::tcsetattr(0, libc::TCSANOW, &self.orig);
        }
    }
}

fn term_cols() -> usize {
    // SAFETY: winsize は POD。失敗したら 80。
    unsafe {
        let mut w: libc::winsize = std::mem::zeroed();
        if libc::ioctl(2, libc::TIOCGWINSZ, &mut w) == 0 && w.ws_col > 0 {
            w.ws_col as usize
        } else {
            80
        }
    }
}

/// fd 0 を直接 1 バイト読む。`std::io::stdin()` はバッファ付きで、矢印の `ESC [ A` を先読みしてしまい
/// 後段の poll が「続きなし」と見て単独 ESC に化けるので使わない。
fn read_byte() -> Option<u8> {
    let mut b = 0u8;
    // SAFETY: 1 バイト分のバッファへの read。
    let n = unsafe { libc::read(0, &mut b as *mut u8 as *mut libc::c_void, 1) };
    if n == 1 { Some(b) } else { None }
}

/// ESC の後に続きが来ているか(単独の ESC と矢印を区別する)。
fn pending_within(ms: i32) -> bool {
    let mut p = libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 };
    // SAFETY: pollfd 1 個、タイムアウト付き。
    unsafe { libc::poll(&mut p, 1, ms) > 0 }
}

enum Key {
    Up,
    Down,
    Enter,
    Quit,
    Digit(u8),
    Other,
}

fn read_key() -> Key {
    match read_byte() {
        None | Some(0x03) | Some(0x04) => Key::Quit, // EOF, Ctrl-C, Ctrl-D
        Some(b'q') | Some(b'Q') => Key::Quit,
        Some(b'\r') | Some(b'\n') => Key::Enter,
        Some(b'k') => Key::Up,
        Some(b'j') => Key::Down,
        Some(d @ b'0'..=b'9') => Key::Digit(d - b'0'),
        Some(0x1b) => {
            if !pending_within(50) {
                return Key::Quit; // 単独の ESC
            }
            match (read_byte(), read_byte()) {
                (Some(b'['), Some(b'A')) | (Some(b'O'), Some(b'A')) => Key::Up,
                (Some(b'['), Some(b'B')) | (Some(b'O'), Some(b'B')) => Key::Down,
                _ => Key::Other,
            }
        }
        Some(_) => Key::Other,
    }
}

/// メニューを出して上下(または j/k、番号)で選ばせる。q / Esc / Ctrl-C / EOF で None。
/// 戻り値の usize は選んだ位置(次回の initial に渡す)。
pub fn ask(initial: usize) -> Result<Option<(usize, &'static Example)>, JurlError> {
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(JurlError::Usage(format!("demo: not a terminal. pick one directly: jurl demo <1-{}>", EXAMPLES.len())));
    }
    let on = color::stderr_enabled();
    let cols = term_cols();
    eprintln!(
        "{}  {}\n{} {}   {} {}",
        paint2(on, C::Bold, C::Magenta, "jurl demo — public APIs, no auth needed"),
        paint(on, C::Dim, "↑↓ / j k / number, Enter to run, q to quit"),
        paint(on, C::Green, "rule"),
        paint(on, C::Dim, "= words resolved by rules, runs offline"),
        paint(on, C::Blue, "jev"),
        paint(on, C::Dim, "= jev decides the roles (needs OPENROUTER_API_KEY, asks before running)")
    );
    let mut sel = initial.min(EXAMPLES.len() - 1);
    let mut typed = String::new();
    draw(sel, on, cols, false);
    let raw = Raw::enable()?;
    let picked = loop {
        match read_key() {
            Key::Quit => break None,
            Key::Enter => break Some((sel, &EXAMPLES[sel])),
            Key::Up => {
                typed.clear();
                sel = if sel == 0 { EXAMPLES.len() - 1 } else { sel - 1 };
            }
            Key::Down => {
                typed.clear();
                sel = (sel + 1) % EXAMPLES.len();
            }
            Key::Digit(d) => {
                // "1" のあと "2" で 12 に。それ以外は打った数字へ。
                let two = format!("{typed}{d}");
                let n = match two.parse::<usize>() {
                    Ok(n) if (1..=EXAMPLES.len()).contains(&n) => {
                        typed = two;
                        n
                    }
                    _ => {
                        typed = d.to_string();
                        d as usize
                    }
                };
                if (1..=EXAMPLES.len()).contains(&n) {
                    sel = n - 1;
                }
            }
            Key::Other => {}
        }
        draw(sel, on, cols, true);
    };
    drop(raw);
    Ok(picked)
}

/// 表示用: `k=v` や `a.b` をクォートしない shell 風の結合。
pub fn join<S: AsRef<str>>(words: &[S]) -> String {
    words.iter().map(|w| crate::curl::quote(w.as_ref())).collect::<Vec<_>>().join(" ")
}
