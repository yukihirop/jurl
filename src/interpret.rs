//! jev パス: 規則で決まらなかったトークンを Oracle に聞き、答えを書き戻し、隣接関係を直す。
//! main から切り離してあるのは、Oracle をモックにして live API なしで回帰を取るため。

use crate::error::JurlError;
use crate::jev::{self, Oracle};
use crate::output::JevInfo;
use crate::repair;
use crate::token::{Role, Token};
use std::time::Instant;

pub struct Interpreted {
    pub info: JevInfo,
    /// jev が「読み取り(GET)」と言った確率。メソッドが明示されていたら None。
    pub get_intent: Option<f32>,
}

pub fn interpret(tokens: &mut Vec<Token>, oracle: &dyn Oracle) -> Result<Interpreted, JurlError> {
    let built = jev::prompt::build(tokens);
    let n = built.questions.len();
    let t0 = Instant::now();
    let res = oracle.decide(built.state, built.questions)?;
    let info = JevInfo { model: res.model.clone(), questions: n, ms: t0.elapsed().as_millis(), usage: res.usage.clone() };
    jev::prompt::apply(tokens, &res.answers);
    repair::merge_joined(tokens, &res.answers);

    let mut is_get = tokens.iter().any(|t| t.role == Some(Role::Method) && t.value() == "GET");
    let mut get_intent = None;
    if let Some(p) = res.answers.get("get_intent").and_then(|a| a.noul())
        && p > 0.5 && !tokens.iter().any(|t| t.role == Some(Role::Method)) {
            is_get = true;
            get_intent = Some(p);
        }
    repair::pair_key_values(tokens, is_get);
    Ok(Interpreted { info, get_intent })
}

#[cfg(test)]
mod tests {
    //! Oracle をモックにした jev パスの統合テスト。
    //! 答えは実機(typesafe/jev-1.13, 2026-09-22)で観測した値を丸めたもの。ワイヤ形式のまま JSON で書く。

    use super::*;
    use crate::assemble::assemble;
    use crate::body::Body;
    use crate::curl;
    use crate::jev::{Answers, DecisionsResponse, Questions};
    use crate::rules::classify;
    use crate::token::Source;
    use serde_json::{json, Value};
    use std::cell::RefCell;

    /// 質問に無いキーへ答えたら panic する(フィクスチャが実際の質問設計とずれたら気づくため)。
    struct Mock {
        answers: Value,
        seen: RefCell<Option<(Value, Questions)>>,
    }

    impl Mock {
        fn new(answers: Value) -> Self {
            Mock { answers, seen: RefCell::new(None) }
        }
        fn state(&self) -> Value {
            self.seen.borrow().as_ref().unwrap().0.clone()
        }
        fn questions(&self) -> Questions {
            self.seen.borrow().as_ref().unwrap().1.clone()
        }
    }

    impl Oracle for Mock {
        fn decide(&self, state: Value, questions: Questions) -> Result<DecisionsResponse, JurlError> {
            for k in self.answers.as_object().unwrap().keys() {
                assert!(questions.contains_key(k), "mock answers `{k}` but jurl did not ask it; asked: {:?}", questions.keys().collect::<Vec<_>>());
            }
            *self.seen.borrow_mut() = Some((state, questions));
            let answers: Answers = serde_json::from_value(self.answers.clone()).expect("answer fixture must be wire-shaped");
            Ok(DecisionsResponse { model: "typesafe/jev-1.13-test".into(), answers, usage: None })
        }
    }

    fn words(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    fn choice(c: &str, probs: &[(&str, f64)]) -> Value {
        let top = probs.iter().map(|p| p.1).fold(0.0, f64::max);
        json!({"type": "choice", "choice": c, "confidence": top, "probabilities": probs.iter().map(|(k, p)| (k.to_string(), json!(p))).collect::<serde_json::Map<_, _>>()})
    }

    fn noul(p: f64) -> Value {
        json!({"type": "noul", "noul": p})
    }

    #[test]
    fn typo_split_and_omitted_scheme() {
        // jurl psot localhsot 3000 users first_name job
        let mut ts = classify(&words("psot localhsot 3000 users first_name job"));
        let mock = Mock::new(json!({
            "role.0": choice("method", &[("method", 0.93), ("url_path", 0.03)]),
            "typo.0": choice("post", &[("post", 0.97), ("none", 0.02)]),
            "ct.0": choice("none", &[("none", 0.99)]),
            "host.0": choice("none", &[("none", 0.99)]),
            "role.1": choice("url", &[("url", 0.88), ("url_path", 0.07)]),
            "typo.1": choice("none", &[("none", 0.99)]),
            "ct.1": choice("none", &[("none", 0.99)]),
            "host.1": choice("localhost", &[("localhost", 0.95), ("127.0.0.1", 0.03), ("none", 0.02)]),
            "role.2": choice("port", &[("port", 0.91), ("field_value", 0.05)]),
            "typo.2": choice("none", &[("none", 0.99)]),
            "ct.2": choice("none", &[("none", 0.99)]),
            "host.2": choice("none", &[("none", 0.99)]),
            "typed.2": noul(0.8),
            "role.3": choice("url_path", &[("url_path", 0.85), ("field_key", 0.10)]),
            "typo.3": choice("none", &[("none", 0.99)]),
            "ct.3": choice("none", &[("none", 0.99)]),
            "host.3": choice("none", &[("none", 0.99)]),
            "role.4": choice("field_key", &[("field_key", 0.97), ("field_value", 0.02)]),
            "typo.4": choice("none", &[("none", 0.99)]),
            "ct.4": choice("none", &[("none", 0.99)]),
            "host.4": choice("none", &[("none", 0.99)]),
            // 実機で観測: `job` を field_key と誤答する。repair が直すはず。
            "role.5": choice("field_key", &[("field_key", 0.90), ("field_value", 0.06)]),
            "typo.5": choice("none", &[("none", 0.99)]),
            "ct.5": choice("none", &[("none", 0.99)]),
            "host.5": choice("none", &[("none", 0.99)]),
            "get_intent": noul(0.2),
        }));
        let out = interpret(&mut ts, &mock).unwrap();
        assert!(out.get_intent.is_none());
        assert_eq!(out.info.questions, 31, "6 role + 6x3 typo/ct/host + 5 join + typed.2 + get_intent");

        let req = assemble(&ts, "application/json").unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.url.render(), "http://localhost:3000/users");
        assert_eq!(req.body, Some(Body::Json(json!({"first_name": "job"}))));
        assert_eq!(ts[5].role, Some(Role::FieldValue));
        // 配置で直した語は配置の確からしさ: key-first 0.97*0.06 vs value-first 0.02*0.90 → 0.76。
        assert!((ts[5].confidence - 0.76).abs() < 0.01, "{}", ts[5].confidence);
        assert!((req.confidence - ts[5].confidence).abs() < 1e-6);
        // 3000 は port なので typed は body に影響しない。
        assert!(ts.iter().all(|t| t.source == Source::Jev));
    }

    #[test]
    fn get_intent_turns_pairs_into_query() {
        // jurl api.agify.io name michael
        let mut ts = classify(&words("api.agify.io name michael"));
        assert_eq!(ts[0].role, Some(Role::Url));
        let mock = Mock::new(json!({
            "role.1": choice("field_key", &[("field_key", 0.55), ("query", 0.40)]),
            "typo.1": choice("none", &[("none", 0.99)]),
            "ct.1": choice("none", &[("none", 0.99)]),
            "host.1": choice("none", &[("none", 0.99)]),
            "role.2": choice("field_value", &[("field_value", 0.9), ("field_key", 0.05)]),
            "typo.2": choice("none", &[("none", 0.99)]),
            "ct.2": choice("none", &[("none", 0.99)]),
            "host.2": choice("none", &[("none", 0.99)]),
            "get_intent": noul(0.89),
        }));
        let out = interpret(&mut ts, &mock).unwrap();
        assert_eq!(out.get_intent, Some(0.89));
        let req = assemble(&ts, "application/json").unwrap();
        assert_eq!(req.method, "GET");
        assert!(req.body.is_none());
        assert_eq!(req.url.render(), "https://api.agify.io/?name=michael");
        let argv = curl::argv(&req, false, &[], &[]);
        assert!(!argv.iter().any(|a| a == "-X"), "{argv:?}");
    }

    #[test]
    fn explicit_method_suppresses_get_intent_question() {
        let mut ts = classify(&words("post localhost first_name job"));
        let mock = Mock::new(json!({
            "role.2": choice("field_key", &[("field_key", 0.97)]),
            "role.3": choice("field_value", &[("field_value", 0.95)]),
        }));
        interpret(&mut ts, &mock).unwrap();
        let q = mock.questions();
        assert!(!q.contains_key("get_intent"));
        assert!(!q.contains_key("role.0") && !q.contains_key("role.1"), "rule-resolved tokens must not be asked");
        assert_eq!(mock.state()["method_known"], json!("POST"));
        assert_eq!(mock.state()["hints"]["0"], json!("method"));
    }

    #[test]
    fn header_value_never_reaches_jev() {
        let mut ts = classify(&words("localhost Authorization:Bearer-s3cr3t first_name job"));
        assert_eq!(ts[1].role, Some(Role::Header));
        let mock = Mock::new(json!({
            "role.2": choice("field_key", &[("field_key", 0.97)]),
            "role.3": choice("field_value", &[("field_value", 0.95)]),
            "get_intent": noul(0.1),
        }));
        interpret(&mut ts, &mock).unwrap();
        let s = serde_json::to_string(&mock.state()).unwrap();
        let q = serde_json::to_string(&mock.questions()).unwrap();
        assert!(!s.contains("s3cr3t") && !q.contains("s3cr3t"), "{s}");
        assert_eq!(mock.state()["tokens"][1], json!("<header>"));
        let req = assemble(&ts, "application/json").unwrap();
        assert_eq!(req.headers, vec![("Authorization".to_string(), "Bearer-s3cr3t".to_string())]);
    }

    #[test]
    fn typed_noul_sends_number() {
        let mut ts = classify(&words("post localhost age 30 active true"));
        let mock = Mock::new(json!({
            "role.2": choice("field_key", &[("field_key", 0.97)]),
            "role.3": choice("field_value", &[("field_value", 0.9), ("port", 0.05)]),
            "typed.3": noul(0.92),
            "role.4": choice("field_key", &[("field_key", 0.97)]),
            "role.5": choice("field_value", &[("field_value", 0.95)]),
            "typed.5": noul(0.3),
        }));
        interpret(&mut ts, &mock).unwrap();
        let req = assemble(&ts, "application/json").unwrap();
        assert_eq!(req.body, Some(Body::Json(json!({"age": 30, "active": "true"}))));
    }

    #[test]
    fn misspelled_content_type_is_fixed_from_table() {
        let mut ts = classify(&words("post localhost jsno a=1"));
        let mock = Mock::new(json!({
            "role.2": choice("content_type", &[("content_type", 0.8), ("url_path", 0.1)]),
            "typo.2": choice("none", &[("none", 0.99)]),
            "ct.2": choice("json", &[("json", 0.9), ("none", 0.05)]),
            "host.2": choice("none", &[("none", 0.99)]),
        }));
        interpret(&mut ts, &mock).unwrap();
        assert_eq!(ts[2].value(), "application/json");
        assert!((ts[2].confidence - 0.8).abs() < 1e-6, "min(role, ct) = {}", ts[2].confidence);
        let req = assemble(&ts, "text/plain").unwrap();
        assert_eq!(req.content_type.as_deref(), Some("application/json"));
    }

    #[test]
    fn unanswered_token_stays_unresolved() {
        let mut ts = classify(&words("post localhost first_name job"));
        let mock = Mock::new(json!({
            "role.2": choice("field_key", &[("field_key", 0.97)]),
        }));
        interpret(&mut ts, &mock).unwrap();
        assert!(!ts[3].resolved());
        match assemble(&ts, "application/json") {
            Err(JurlError::Unresolved(s)) => assert_eq!(s, "job"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn join_merges_multi_word_value() {
        // jurl httpbin.org/anything post title hello world  (live 2026-09-22: join.4 = 0.81)
        let mut ts = classify(&words("httpbin.org/anything post title hello world"));
        let mock = Mock::new(json!({
            "role.2": choice("field_key", &[("field_key", 1.0)]),
            "role.3": choice("field_value", &[("field_value", 0.93), ("field_key", 0.07)]),
            "join.3": noul(0.05),
            "role.4": choice("field_key", &[("field_key", 0.6), ("field_value", 0.4)]),
            "join.4": noul(0.81),
        }));
        interpret(&mut ts, &mock).unwrap();
        assert!(!mock.questions().contains_key("join.2"), "post is rule-resolved, title cannot continue it");
        assert_eq!(ts.len(), 4);
        assert_eq!(ts[3].text, "hello world");
        assert_eq!(ts[3].role, Some(Role::FieldValue));
        assert!((ts[3].confidence - 0.81).abs() < 1e-6, "min(0.93, p) = {}", ts[3].confidence);
        let req = assemble(&ts, "application/json").unwrap();
        assert_eq!(req.body, Some(Body::Json(json!({"title": "hello world"}))));
    }

    #[test]
    fn join_chains_and_extends_a_rule_field() {
        // jurl post httpbin.org/anything title=hello world again
        let mut ts = classify(&words("post httpbin.org/anything title=hello world again"));
        let mock = Mock::new(json!({
            "role.3": choice("field_key", &[("field_key", 0.5)]),
            "join.3": noul(0.7),
            "role.4": choice("field_value", &[("field_value", 0.5)]),
            "join.4": noul(0.9),
        }));
        interpret(&mut ts, &mock).unwrap();
        assert_eq!(ts.len(), 3);
        assert_eq!(ts[2].text, "title=hello world again");
        assert_eq!(ts[2].role, Some(Role::Field));
        let req = assemble(&ts, "application/json").unwrap();
        assert_eq!(req.body, Some(Body::Json(json!({"title": "hello world again"}))));
    }

    #[test]
    fn join_never_swallows_a_key() {
        // jev が join と言っても、前が値でなければ結合しない(first_name job: job は first_name の続きではない)。
        let mut ts = classify(&words("post localhost first_name job"));
        let mock = Mock::new(json!({
            "role.2": choice("field_key", &[("field_key", 0.98)]),
            "role.3": choice("field_value", &[("field_value", 0.9)]),
            "join.3": noul(0.9),
        }));
        interpret(&mut ts, &mock).unwrap();
        assert_eq!(ts.len(), 4);
        assert_eq!(ts[3].text, "job");
    }

    #[test]
    fn oracle_error_propagates() {
        struct Down;
        impl Oracle for Down {
            fn decide(&self, _: Value, _: Questions) -> Result<DecisionsResponse, JurlError> {
                Err(JurlError::Jev("HTTP 503".into()))
            }
        }
        let mut ts = classify(&words("post localhost first_name job"));
        match interpret(&mut ts, &Down) {
            Err(JurlError::Jev(m)) => assert_eq!(m, "HTTP 503"),
            other => panic!("{:?}", other.map(|_| ())),
        }
        assert!(!ts[2].resolved(), "tokens untouched on failure");
    }
}
