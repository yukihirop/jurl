//! jev の答えは質問ごとに独立なので、隣接関係(キーの次は値)はコードで直す(設計図 §5)。

use crate::token::{Role, Source, Token};

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

/// jev が「前の語と同じ値の続き」(join.i > 0.5)と言った語を前の語に結合する(`title hello world` → `hello world`)。
/// 前が jev の FieldValue か、規則の `k=v`(Field)のときだけ。後ろから見るので `a b c` も 1 つになる。
/// 結合した語の confidence は min(前の conf, p)。配列は対象外(空白区切りの配列は文字列 1 つになる)。
pub fn merge_joined(tokens: &mut Vec<Token>, answers: &crate::jev::Answers) {
    let mut i = tokens.len();
    while i > 1 {
        i -= 1;
        let Some(p) = answers.get(&format!("join.{i}")).and_then(|a| a.noul()) else { continue };
        if p <= 0.5 {
            continue;
        }
        // 前の語自身が「さらに前の続き」(join.{i-1} > 0.5)なら、jev が中間の語を key と言っていても連鎖を切らない。
        let prev_joins = answers.get(&format!("join.{}", i - 1)).and_then(|a| a.noul()).map(|q| q > 0.5).unwrap_or(false);
        let prev_ok = match tokens[i - 1].role {
            Some(Role::FieldValue) => tokens[i - 1].source == Source::Jev,
            Some(Role::Field) => !tokens[i - 1].typed,
            _ => tokens[i - 1].source == Source::Jev && prev_joins,
        };
        if !prev_ok || tokens[i].source != Source::Jev {
            continue;
        }
        let cur = tokens.remove(i);
        let prev = &mut tokens[i - 1];
        prev.text = format!("{} {}", prev.text, cur.text);
        prev.fixed = None;
        prev.typed = false;
        prev.confidence = prev.confidence.min(p);
        // pair_key_values は probs から confidence を引き直すので、「値である確率」にも join の p を反映しておく。
        if let Some(m) = prev.probs.as_mut() {
            let v = m.entry("field_value".to_string()).or_insert(1.0);
            *v = v.min(p);
        }
        prev.note = Some(format!("joined \"{}\" p={p:.2}{}", cur.text, prev.note.as_deref().map(|n| format!("; {n}")).unwrap_or_default()));
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
