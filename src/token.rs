//! 引数 1 つ 1 つに割り当てる役割。位置には意味を持たせない(設計図 §4)。

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Method,
    Url,
    UrlPath,
    Port,
    ContentType,
    Field,
    FieldKey,
    FieldValue,
    Query,
    Header,
    BodyFile,
    CurlFlag,
    CurlFlagValue,
    Noise,
}

impl Role {
    /// jev に選ばせる役割。Header は規則でしか付けない(秘密を jev に送らないため)。
    pub const JEV_CHOICES: &'static [(Role, &'static str, &'static str)] = &[
        (Role::Method, "method", "HTTP method, possibly misspelled (get/post/put/patch/delete/head/options)"),
        (Role::Url, "url", "Host or full URL (localhost, example.com/path, http://...)"),
        (Role::UrlPath, "url_path", "A path segment that belongs after the host (users, v1/items)"),
        (Role::Port, "port", "A bare port number that belongs to the host"),
        (Role::ContentType, "content_type", "A MIME type or its short name, possibly misspelled (json, form, application/json)"),
        (Role::FieldKey, "field_key", "A body field name whose value is the next token"),
        (Role::FieldValue, "field_value", "A body field value whose name is the previous token"),
        (Role::Query, "query", "A query-string key=value pair"),
        (Role::BodyFile, "body_file", "A file reference starting with @"),
        (Role::CurlFlag, "curl_flag", "A curl option such as -k or --max-time"),
        (Role::CurlFlagValue, "curl_flag_value", "The argument of the previous curl option"),
        (Role::Noise, "noise", "Filler with no meaning for the request"),
    ];

    pub fn from_key(key: &str) -> Option<Role> {
        Role::JEV_CHOICES.iter().find(|(_, k, _)| *k == key).map(|(r, _, _)| *r)
    }

    pub fn key(self) -> &'static str {
        match self {
            Role::Header => "header",
            Role::Field => "field",
            _ => Role::JEV_CHOICES.iter().find(|(r, _, _)| *r == self).map(|(_, k, _)| *k).unwrap_or("?"),
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Rule,
    Jev,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub text: String,
    pub role: Option<Role>,
    pub confidence: f32,
    pub source: Source,
    /// 補正後の値(タイポ補正した method など)。無ければ text をそのまま使う。
    pub fixed: Option<String>,
    /// `:=` で型付きにする / jev が「数値・真偽・null として送れ」と言った。
    pub typed: bool,
    /// `--explain` に出す一言。
    pub note: Option<String>,
    /// jev が返した役割ごとの確率(規則で決めたものは無し)。repair の根拠に使う。
    pub probs: Option<std::collections::BTreeMap<String, f32>>,
}

impl Token {
    pub fn new(text: impl Into<String>) -> Self {
        Token { text: text.into(), role: None, confidence: 0.0, source: Source::Rule, fixed: None, typed: false, note: None, probs: None }
    }

    pub fn resolved(&self) -> bool {
        self.role.is_some()
    }

    pub fn set_rule(&mut self, role: Role) {
        self.role = Some(role);
        self.confidence = 1.0;
        self.source = Source::Rule;
    }

    pub fn value(&self) -> &str {
        self.fixed.as_deref().unwrap_or(&self.text)
    }
}
