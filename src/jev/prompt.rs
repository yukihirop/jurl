//! jev に渡す state / questions の組み立てと、answers のトークンへの書き戻し(設計図 §7)。
//! 候補はすべてコード側の固定表。jev は候補から選ぶだけで、入力に無い値は作らない。

use super::{choice, noul, Answers, Questions};
use crate::rules::{CT_SYNONYMS, METHODS};
use crate::token::{Role, Source, Token};
use serde_json::{json, Value};

const TOOL_DESC: &str = "jurl: an httpie-like wrapper that turns loosely ordered command words into one curl command. \
Words may be misspelled, split (key and value as separate words), or out of order.";

pub struct Built {
    pub state: Value,
    pub questions: Questions,
}

/// 規則で決まらなかったトークンについて質問を作る。state には全トークンを入れる(文脈のため)。
/// Header は値を外に出さないので `<header>` に置き換える。
pub fn build(tokens: &[Token]) -> Built {
    let words: Vec<String> = tokens
        .iter()
        .map(|t| if t.role == Some(Role::Header) { "<header>".to_string() } else { t.text.clone() })
        .collect();
    let mut hints = serde_json::Map::new();
    for (i, t) in tokens.iter().enumerate() {
        if let Some(r) = t.role {
            hints.insert(i.to_string(), Value::String(r.key().to_string()));
        }
    }
    let method_known = tokens.iter().find(|t| t.role == Some(Role::Method)).map(|t| t.value().to_string());

    let mut questions = Questions::new();
    let role_criteria: Vec<(&str, Option<&str>)> = Role::JEV_CHOICES.iter().map(|(_, k, d)| (*k, Some(*d))).collect();

    for (i, t) in tokens.iter().enumerate() {
        if t.resolved() {
            continue;
        }
        let w = &t.text;
        questions.insert(
            format!("role.{i}"),
            choice(format!("What is the role of `tokens[{i}]` (\"{w}\") in this command?"), &role_criteria),
        );
        if could_be_word(w) {
            let mut c: Vec<(&str, Option<&str>)> = METHODS.iter().map(|m| (*m, None)).collect();
            c.push(("none", Some("Not an HTTP method")));
            questions.insert(
                format!("typo.{i}"),
                choice(format!("If `tokens[{i}]` (\"{w}\") is a possibly misspelled HTTP method, which one was intended?"), &c),
            );
            let mut c: Vec<(&str, Option<&str>)> = CT_SYNONYMS.iter().map(|(k, m)| (*k, Some(*m))).collect();
            c.push(("none", Some("Not a content type")));
            questions.insert(
                format!("ct.{i}"),
                choice(format!("If `tokens[{i}]` (\"{w}\") is a possibly misspelled content type, which one was intended?"), &c),
            );
            questions.insert(
                format!("host.{i}"),
                choice(
                    format!("If `tokens[{i}]` (\"{w}\") is a misspelled well-known local host name, which one?"),
                    &[("localhost", None), ("127.0.0.1", None), ("none", Some("Not a local host name, or spelled correctly"))],
                ),
            );
        }
        // 直前も jev 行き(または k=v)なら「同じ値の続きか」を聞く。`title hello world` の world 用。
        if i > 0 && joinable_prev(&tokens[i - 1]) {
            let prev = &tokens[i - 1].text;
            questions.insert(
                format!("join.{i}"),
                noul(format!(
                    "Is `tokens[{i}]` (\"{w}\") a continuation of the same value as `tokens[{}]` (\"{prev}\"), i.e. do the two words together form one multi-word value (a title, a sentence, a name), rather than `tokens[{i}]` starting a new key or being a separate item?",
                    i - 1
                )),
            );
        }
        if looks_typed(w) {
            questions.insert(
                format!("typed.{i}"),
                noul(format!(
                    "If `tokens[{i}]` (\"{w}\") is a body field value, should it be sent as a JSON number, boolean or null rather than a string?"
                )),
            );
        }
    }

    // メソッドが無く、jev に聞く単語があるときだけ「これは読み取り(GET + クエリ)か」を同梱する。
    // 決定(2026-09-22)の「ボディあり → POST」は、jev が GET 意図と言わなかったときの既定になる。
    if method_known.is_none() && !questions.is_empty() {
        questions.insert(
            "get_intent".into(),
            noul("Is this command a read-only lookup (an HTTP GET whose key/value words are query parameters), rather than sending or creating data (POST)?"),
        );
    }

    let state = json!({
        "tool": TOOL_DESC,
        "tokens": words,
        "hints": hints,
        "method_known": method_known,
    });
    Built { state, questions }
}

fn joinable_prev(t: &Token) -> bool {
    !t.resolved() || (t.role == Some(Role::Field) && !t.typed)
}

fn could_be_word(w: &str) -> bool {
    !w.is_empty() && w.len() <= 40 && w.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '.' | '_'))
}

fn looks_typed(w: &str) -> bool {
    matches!(w, "true" | "false" | "null") || w.parse::<f64>().is_ok()
}

/// answers をトークンに書き戻す。規則で決まっていたものは触らない。
pub fn apply(tokens: &mut [Token], answers: &Answers) {
    for (i, t) in tokens.iter_mut().enumerate() {
        if t.resolved() {
            continue;
        }
        let Some(a) = answers.get(&format!("role.{i}")) else { continue };
        let Some(role) = a.choice().and_then(Role::from_key) else { continue };
        t.role = Some(role);
        t.confidence = a.certainty();
        t.source = Source::Jev;
        t.note = Some(a.top2());
        t.probs = a.probabilities().cloned();

        match role {
            Role::Method => {
                let lw = t.text.to_ascii_lowercase();
                if METHODS.contains(&lw.as_str()) {
                    t.fixed = Some(lw.to_ascii_uppercase());
                } else if let Some(a2) = answers.get(&format!("typo.{i}")) {
                    match a2.choice() {
                        Some("none") | None => t.confidence = t.confidence.min(0.3),
                        Some(m) => {
                            t.fixed = Some(m.to_ascii_uppercase());
                            t.confidence = t.confidence.min(a2.certainty());
                        }
                    }
                }
            }
            Role::ContentType => {
                if let Some(ct) = crate::rules::content_type(&t.text) {
                    t.fixed = Some(ct);
                } else if let Some(a2) = answers.get(&format!("ct.{i}")) {
                    match a2.choice().and_then(|k| CT_SYNONYMS.iter().find(|(s, _)| *s == k)) {
                        Some((_, mime)) => {
                            t.fixed = Some((*mime).to_string());
                            t.confidence = t.confidence.min(a2.certainty());
                        }
                        None => t.confidence = t.confidence.min(0.3),
                    }
                }
            }
            Role::Url => {
                if !crate::rules::looks_like_url(&t.text)
                    && let Some(a2) = answers.get(&format!("host.{i}")) {
                        match a2.choice() {
                            Some("none") | None => {}
                            Some(h) => {
                                t.fixed = Some(h.to_string());
                                t.confidence = t.confidence.min(a2.certainty());
                            }
                        }
                    }
            }
            Role::FieldValue => {
                if let Some(p) = answers.get(&format!("typed.{i}")).and_then(|a| a.noul()) {
                    t.typed = p > 0.5;
                }
            }
            _ => {}
        }
    }
}
