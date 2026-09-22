//! ANSI 色。出力先が端末で NO_COLOR が無いときだけ付ける。

use std::io::IsTerminal;

#[derive(Clone, Copy)]
pub enum C {
    Dim,
    Bold,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
}

impl C {
    fn code(self) -> &'static str {
        match self {
            C::Dim => "2",
            C::Bold => "1",
            C::Red => "31",
            C::Green => "32",
            C::Yellow => "33",
            C::Blue => "34",
            C::Magenta => "35",
            C::Cyan => "36",
        }
    }
}

pub fn stderr_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stderr().is_terminal()
}

pub fn stdout_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

pub fn paint(on: bool, c: C, s: &str) -> String {
    if on {
        format!("\x1b[{}m{s}\x1b[0m", c.code())
    } else {
        s.to_string()
    }
}

pub fn paint2(on: bool, a: C, b: C, s: &str) -> String {
    if on {
        format!("\x1b[{};{}m{s}\x1b[0m", a.code(), b.code())
    } else {
        s.to_string()
    }
}

/// confidence の帯で色を変える: ≥0.8 緑、≥0.5 黄、それ未満 赤。
pub fn conf(on: bool, v: f32) -> String {
    let s = format!("{v:.2}");
    let c = if v >= 0.8 { C::Green } else if v >= 0.5 { C::Yellow } else { C::Red };
    paint(on, c, &s)
}

/// 整形済み JSON にキー / 文字列 / 数値 / 真偽・null の色を付ける。
pub fn json(on: bool, pretty: &str) -> String {
    if !on {
        return pretty.to_string();
    }
    let mut out = String::with_capacity(pretty.len() * 2);
    let b = pretty.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' => {
                let start = i;
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
                let s = &pretty[start..i.min(b.len())];
                // 直後が `:` ならキー。
                let mut j = i;
                while j < b.len() && b[j] == b' ' {
                    j += 1;
                }
                let is_key = j < b.len() && b[j] == b':';
                out.push_str(&paint(true, if is_key { C::Blue } else { C::Green }, s));
            }
            b'0'..=b'9' | b'-' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_digit() || matches!(b[i], b'.' | b'-' | b'+' | b'e' | b'E')) {
                    i += 1;
                }
                out.push_str(&paint(true, C::Magenta, &pretty[start..i]));
            }
            b't' | b'f' | b'n' => {
                let rest = &pretty[i..];
                let word = ["true", "false", "null"].iter().find(|w| rest.starts_with(*w));
                match word {
                    Some(w) => {
                        out.push_str(&paint(true, C::Yellow, w));
                        i += w.len();
                    }
                    None => {
                        out.push(b[i] as char);
                        i += 1;
                    }
                }
            }
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    out
}
