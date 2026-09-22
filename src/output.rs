//! レスポンスの整形と `--explain` の表(設計図 §8)。

use crate::color::{self, paint, C};
use crate::curl::STATUS_MARK;
use crate::jev::Usage;
use crate::token::{Source, Token};
use std::io::{IsTerminal, Write};

pub struct Split<'a> {
    pub body: &'a [u8],
    pub status: Option<(String, String)>,
}

/// `-w` で末尾に付けた `\x01jurl\x01<code> <time>` を切り離す。
pub fn split(stdout: &[u8]) -> Split<'_> {
    let mark = STATUS_MARK.as_bytes();
    if let Some(pos) = stdout.windows(mark.len()).rposition(|w| w == mark) {
        let tail = String::from_utf8_lossy(&stdout[pos + mark.len()..]).trim().to_string();
        let mut it = tail.split_whitespace();
        let code = it.next().unwrap_or("").to_string();
        let time = it.next().unwrap_or("").to_string();
        return Split { body: &stdout[..pos], status: Some((code, time)) };
    }
    Split { body: stdout, status: None }
}

pub fn print_response(stdout: &[u8], raw: bool) -> std::io::Result<()> {
    let out = std::io::stdout();
    let tty = out.is_terminal();
    let s = split(stdout);
    let mut o = out.lock();
    let mut e = std::io::stderr().lock();

    if let Some((code, time)) = s.status.as_ref().filter(|(c, _)| c != "000") {
        let ms = time.parse::<f64>().map(|t| (t * 1000.0).round() as u64).unwrap_or(0);
        let to_stdout = tty && !raw;
        let on = if to_stdout { color::stdout_enabled() } else { color::stderr_enabled() };
        let c = match code.chars().next() {
            Some('2') => C::Green,
            Some('3') => C::Cyan,
            Some('4') => C::Yellow,
            _ => C::Red,
        };
        let line = format!("{} {}", paint(on, c, &format!("HTTP {code}")), paint(on, C::Dim, &format!("· {ms}ms")));
        if to_stdout {
            writeln!(o, "{line}")?;
        } else {
            writeln!(e, "{line}")?;
        }
    }
    if tty && !raw {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(s.body) {
            let pretty = serde_json::to_string_pretty(&v).unwrap_or_default();
            writeln!(o, "{}", color::json(color::stdout_enabled(), &pretty))?;
            return Ok(());
        }
    }
    o.write_all(s.body)?;
    if tty && !s.body.ends_with(b"\n") && !s.body.is_empty() {
        writeln!(o)?;
    }
    Ok(())
}

pub fn explain(tokens: &[Token], jev: Option<&JevInfo>) {
    use crate::token::Role;
    let on = color::stderr_enabled();
    let mut e = std::io::stderr().lock();
    let w = tokens.iter().map(|t| t.text.len()).max().unwrap_or(4).clamp(4, 40);
    let _ = writeln!(e, "{}", paint(on, C::Dim, &format!("{:<w$}  {:<16} {:<5} {:<4}  note", "word", "role", "conf", "by", w = w)));
    for t in tokens {
        let role_s = t.role.map(|r| r.key().to_string()).unwrap_or_else(|| "?".into());
        let role_c = match t.role {
            None => C::Red,
            Some(Role::Url | Role::UrlPath | Role::Port) => C::Blue,
            Some(Role::Method) => C::Magenta,
            Some(Role::Field | Role::FieldKey | Role::FieldValue | Role::Query | Role::BodyFile) => C::Green,
            Some(Role::Header | Role::ContentType) => C::Cyan,
            Some(Role::CurlFlag | Role::CurlFlagValue | Role::Noise) => C::Dim,
        };
        let role = format!("{}{}", paint(on, role_c, &role_s), " ".repeat(16usize.saturating_sub(role_s.len())));
        let by = match t.source {
            Source::Rule => paint(on, C::Dim, "rule"),
            Source::Jev => paint(on, C::Blue, "jev "),
        };
        let mut note = t.note.clone().unwrap_or_default();
        if let Some(f) = &t.fixed {
            if f != &t.text {
                note = format!("→ {f}{}{note}", if note.is_empty() { "" } else { "; " });
            }
        }
        if t.typed {
            note.push_str(" (typed)");
        }
        let _ = writeln!(e, "{:<w$}  {} {}  {}  {}", t.text, role, color::conf(on, t.confidence), by, paint(on, C::Dim, &note), w = w);
    }
    let _ = match jev {
        Some(j) => {
            let u = j.usage.clone().unwrap_or_default();
            let line = format!(
                "jev: {} · {} questions · {} ms · {} in / {} out tokens · ${}",
                j.model,
                j.questions,
                j.ms,
                u.input_tokens,
                u.output_tokens,
                u.cost.map(|c| format!("{c:.6}")).unwrap_or_else(|| "?".into())
            );
            writeln!(e, "{}", paint(on, C::Dim, &line))
        }
        None => writeln!(e, "{}", paint(on, C::Dim, "jev: not called (fast path)")),
    };
}

pub struct JevInfo {
    pub model: String,
    pub questions: usize,
    pub ms: u128,
    pub usage: Option<Usage>,
}

pub enum Choice {
    Yes,
    No,
    Edit,
}

/// Y / n / e。端末でなければ聞けないので No(--yes が無い限り実行しない)。
pub fn confirm(prompt: &str) -> Choice {
    if !std::io::stdin().is_terminal() {
        eprintln!("{prompt} — not a terminal, refusing to guess (use --yes)");
        return Choice::No;
    }
    let on = color::stderr_enabled();
    eprint!("{} {} ", paint(on, C::Yellow, prompt), paint(on, C::Dim, "[Y/n/e]"));
    let _ = std::io::stderr().flush();
    let mut s = String::new();
    if std::io::stdin().read_line(&mut s).is_err() {
        return Choice::No;
    }
    match s.trim().to_ascii_lowercase().as_str() {
        "" | "y" | "yes" => Choice::Yes,
        "e" | "edit" => Choice::Edit,
        _ => Choice::No,
    }
}

/// e / N。既定は No。
pub fn confirm_edit(prompt: &str) -> Choice {
    if !std::io::stdin().is_terminal() {
        return Choice::No;
    }
    let on = color::stderr_enabled();
    eprint!("{} {} ", paint(on, C::Red, prompt), paint(on, C::Dim, "[e/N]"));
    let _ = std::io::stderr().flush();
    let mut s = String::new();
    if std::io::stdin().read_line(&mut s).is_err() {
        return Choice::No;
    }
    match s.trim().to_ascii_lowercase().as_str() {
        "e" | "edit" => Choice::Edit,
        _ => Choice::No,
    }
}

/// 閉じるまで待つためのフラグ。既知の GUI エディタだけ。
fn wait_flag(prog: &str) -> Option<&'static str> {
    let name = std::path::Path::new(prog).file_name().and_then(|n| n.to_str()).unwrap_or(prog);
    match name {
        "code" | "code-insiders" | "codium" | "cursor" | "windsurf" | "subl" | "zed" | "atom" | "mate" => Some("--wait"),
        "bbedit" => Some("-w"),
        _ => None,
    }
}

/// $EDITOR(無ければ vi)で curl コマンドを編集させ、shell の語分割で argv に戻す。
/// `typed` は元の入力(コメントとして表示するだけ)。
/// 空にして保存したら None。
pub fn edit_command(rendered: &str, typed: &str) -> Result<Option<Vec<String>>, crate::error::JurlError> {
    use crate::error::JurlError;
    let editor = std::env::var("VISUAL").or_else(|_| std::env::var("EDITOR")).unwrap_or_else(|_| "vi".into());
    let path = std::env::temp_dir().join(format!("jurl-{}.sh", std::process::id()));
    // 元の入力もコメントで見せる(何を打ったか覚えていない前提で、curl と見比べられるように)。
    let text = format!(
        "# you typed:\n#   jurl {typed}\n\n{rendered}\n\n# jurl: edit the command above, save and quit to run it.\n# Lines starting with # are ignored. Empty the file to abort.\n"
    );
    std::fs::write(&path, text)?;
    let mut words = shell_words::split(&editor).map_err(|e| JurlError::Usage(format!("bad $EDITOR: {e}")))?;
    if words.is_empty() {
        return Err(JurlError::Usage("empty $EDITOR".into()));
    }
    // GUI エディタはファイルを開いてすぐ戻るので、閉じるまで待つフラグを足す(無ければ編集前に実行してしまう)。
    if let Some(flag) = wait_flag(&words[0]) {
        if !words.iter().any(|w| w == flag || w == "-w" || w == "--wait") {
            words.push(flag.to_string());
        }
    }
    eprintln!("{}", paint(color::stderr_enabled(), C::Dim, &format!("editing with: {} {}", shell_words::join(&words), path.display())));
    let (prog, args) = words.split_first().unwrap();
    let status = std::process::Command::new(prog).args(args).arg(&path).status()?;
    let edited = std::fs::read_to_string(&path).unwrap_or_default();
    let _ = std::fs::remove_file(&path);
    if !status.success() {
        return Err(JurlError::Aborted);
    }
    let script: String = edited.lines().filter(|l| !l.trim_start().starts_with('#')).collect::<Vec<_>>().join("\n");
    let argv = shell_words::split(&script).map_err(|e| JurlError::Usage(format!("cannot parse edited command: {e}")))?;
    Ok(if argv.is_empty() { None } else { Some(argv) })
}

/// 既定が No の確認。
pub fn confirm_no(prompt: &str) -> bool {
    if !std::io::stdin().is_terminal() {
        return false;
    }
    eprint!("{prompt} [y/N] ");
    let _ = std::io::stderr().flush();
    let mut s = String::new();
    if std::io::stdin().read_line(&mut s).is_err() {
        return false;
    }
    let s = s.trim().to_ascii_lowercase();
    s == "y" || s == "yes"
}
