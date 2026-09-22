//! env + ~/.config/jurl/config.toml(設計図 §10)。全部省略可。

use crate::error::JurlError;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub jev: Jev,
    pub defaults: Defaults,
    pub aliases: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Jev {
    pub enabled: bool,
    /// `jurl setup` が書く。env の OPENROUTER_API_KEY が優先。
    pub api_key: Option<String>,
    pub model: String,
    /// "jev": jev が関わったら必ず確認 / "confidence": confirm_below 未満のときだけ / "never": 確認しない
    pub confirm: String,
    pub confirm_below: f32,
    pub reject_below: f32,
    /// 取り消せないメソッド(PUT / PATCH / DELETE)の確認帯。
    pub confirm_below_unsafe: f32,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Defaults {
    pub content_type: String,
    pub curl_args: Vec<String>,
}

impl Default for Jev {
    fn default() -> Self {
        Jev {
            enabled: true,
            api_key: None,
            confirm: "jev".into(),
            model: crate::jev::client::DEFAULT_MODEL.into(),
            confirm_below: 0.8,
            reject_below: 0.5,
            confirm_below_unsafe: 0.9,
            timeout_ms: 5000,
        }
    }
}

impl Default for Defaults {
    fn default() -> Self {
        Defaults { content_type: "json".into(), curl_args: Vec::new() }
    }
}

pub fn path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("JURL_CONFIG") {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config").join("jurl").join("config.toml"))
}

pub fn load() -> Result<Config, JurlError> {
    let mut cfg = match path() {
        Some(p) if p.exists() => {
            let text = std::fs::read_to_string(&p)?;
            toml::from_str::<Config>(&text).map_err(|e| JurlError::Config(format!("{}: {e}", p.display())))?
        }
        _ => Config::default(),
    };
    if let Ok(m) = std::env::var("JEV_MODEL") {
        cfg.jev.model = m;
    }
    // 短縮名は MIME に直しておく。
    if let Some(ct) = crate::rules::content_type(&cfg.defaults.content_type) {
        cfg.defaults.content_type = ct;
    }
    Ok(cfg)
}

/// aliases を展開し、値の中の `$VAR` を環境変数で置き換える。
pub fn expand_aliases(cfg: &Config, words: &[String]) -> Vec<String> {
    words
        .iter()
        .map(|w| match cfg.aliases.get(w) {
            Some(v) => expand_env(v),
            None => w.clone(),
        })
        .collect()
}

fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            let mut name = String::new();
            while let Some(&n) = chars.peek() {
                if n.is_ascii_alphanumeric() || n == '_' {
                    name.push(n);
                    chars.next();
                } else {
                    break;
                }
            }
            if name.is_empty() {
                out.push('$');
            } else {
                out.push_str(&std::env::var(&name).unwrap_or_default());
            }
        } else {
            out.push(c);
        }
    }
    out
}
