//! jev の答えは質問ごとに独立なので、隣接関係(キーの次は値)はコードで直す(設計図 §5)。

use crate::token::{Role, Token};

/// `key value key value …` の並びを強制する。
/// 連続する {FieldKey, FieldValue, 裸の Query} の run ごとに、
/// 「キー始まり」と「値始まり」の 2 通りの交互配置を jev の確率で比べ、良い方を採る。
/// run の confidence は P(採った配置) / (P(採った配置) + P(もう一方))。
pub fn pair_key_values(tokens: &mut [Token], method_is_get: bool) {
    let mut i = 0;
    while i < tokens.len() {
        if !is_kv(&tokens[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < tokens.len() && is_kv(&tokens[i]) {
            i += 1;
        }
        let run = start..i;
        if run.len() < 2 {
            continue;
        }
        let as_query = method_is_get || tokens[run.clone()].iter().any(|t| t.role == Some(Role::Query));

        // 2 通りの配置の尤度。確率が無い(規則で決めた)トークンは 1.0 / 0.0 として扱う。
        let p = |t: &Token, key: bool| -> f32 {
            match &t.probs {
                Some(m) => {
                    let k = m.get("field_key").copied().unwrap_or(0.0) + m.get("query").copied().unwrap_or(0.0);
                    let v = m.get("field_value").copied().unwrap_or(0.0);
                    (if key { k } else { v }).max(1e-3)
                }
                None => {
                    let is_key = matches!(t.role, Some(Role::FieldKey) | Some(Role::Query));
                    if is_key == key { 1.0 } else { 1e-3 }
                }
            }
        };
        let mut p_key_first = 1.0f32;
        let mut p_val_first = 1.0f32;
        for (n, t) in tokens[run.clone()].iter().enumerate() {
            p_key_first *= p(t, n % 2 == 0);
            p_val_first *= p(t, n % 2 == 1);
        }
        let key_first = p_key_first >= p_val_first;
        let conf = if key_first { p_key_first / (p_key_first + p_val_first) } else { p_val_first / (p_key_first + p_val_first) };

        for (n, idx) in run.clone().enumerate() {
            let is_key = (n % 2 == 0) == key_first;
            let t = &mut tokens[idx];
            let new_role = if is_key { if as_query { Role::Query } else { Role::FieldKey } } else { Role::FieldValue };
            if t.role != Some(new_role) {
                let old = t.role.map(|r| r.key()).unwrap_or("?");
                t.note = Some(format!("{old} → {} (paired){}", new_role.key(), t.note.as_deref().map(|n| format!("; {n}")).unwrap_or_default()));
            }
            t.role = Some(new_role);
            // jev が付けた役割の確率と、run 全体の配置の確からしさの小さい方。
            // (配置だけで 1.00 にすると、jev が field_key 0.7 と見ていた事実が消える)
            if let Some(m) = &t.probs {
                let role_p = if is_key {
                    m.get("field_key").copied().unwrap_or(0.0) + m.get("query").copied().unwrap_or(0.0)
                } else {
                    m.get("field_value").copied().unwrap_or(0.0)
                };
                t.confidence = role_p.min(conf);
            }
        }
    }
}

fn is_kv(t: &Token) -> bool {
    match t.role {
        Some(Role::FieldKey) | Some(Role::FieldValue) => true,
        Some(Role::Query) => !t.text.contains("=="),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Source;
    use std::collections::BTreeMap;

    fn jev(text: &str, role: Role, key: f32, val: f32) -> Token {
        let mut t = Token::new(text);
        t.role = Some(role);
        t.confidence = key.max(val);
        t.source = Source::Jev;
        t.probs = Some(BTreeMap::from([("field_key".to_string(), key), ("field_value".to_string(), val)]));
        t
    }

    #[test]
    fn forces_alternation() {
        let mut ts = vec![
            jev("first_name", Role::FieldKey, 0.99, 0.01),
            jev("job", Role::FieldKey, 0.90, 0.06),
            jev("family_name", Role::FieldKey, 0.99, 0.01),
            jev("amanda", Role::FieldValue, 0.02, 0.95),
        ];
        pair_key_values(&mut ts, false);
        let roles: Vec<_> = ts.iter().map(|t| t.role.unwrap()).collect();
        assert_eq!(roles, [Role::FieldKey, Role::FieldValue, Role::FieldKey, Role::FieldValue]);
        assert!((ts[1].confidence - 0.06).abs() < 1e-6, "{}", ts[1].confidence);
    }

    #[test]
    fn get_pairs_become_query() {
        let mut ts = vec![jev("page", Role::Query, 0.67, 0.33), jev("2", Role::FieldValue, 0.43, 0.53)];
        pair_key_values(&mut ts, true);
        assert_eq!(ts[0].role, Some(Role::Query));
        assert_eq!(ts[1].role, Some(Role::FieldValue));
    }
}
