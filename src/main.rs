mod assemble;
mod body;
mod cli;
mod config;
mod curl;
mod error;
mod jev;
mod output;
mod repair;
mod rules;
mod setup;
mod token;
mod url;

use error::JurlError;
use jev::Oracle;
use token::Role;
use std::time::Instant;

fn main() {
    let parsed = cli::parse(std::env::args().skip(1));
    match run(parsed) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("jurl: {e}");
            std::process::exit(e.exit_code());
        }
    }
}

fn run(parsed: cli::Parsed) -> Result<i32, JurlError> {
    let cli::Parsed { opts, words, passthrough } = parsed;
    if opts.help {
        print!("{}", cli::HELP);
        return Ok(0);
    }
    if opts.version {
        println!("jurl {}", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }
    if words.is_empty() {
        return Err(JurlError::Usage("nothing to do. try: jurl post localhost json name=job".into()));
    }
    // 第 1 引数がちょうど `setup` のときだけサブコマンド。
    if words.len() == 1 && words[0] == "setup" {
        return setup::run();
    }

    let cfg = config::load()?;
    let words = config::expand_aliases(&cfg, &words);

    // 1–2. 規則で分類。全部決まれば高速パス。
    let mut tokens = rules::classify(&words);
    let mut jev_info: Option<output::JevInfo> = None;
    let mut get_intent: Option<f32> = None;

    // 3. 決まらなかったものがあれば jev に全トークンを渡す。
    if tokens.iter().any(|t| !t.resolved()) {
        if opts.no_jev || !cfg.jev.enabled {
            let bad: Vec<&str> = tokens.iter().filter(|t| !t.resolved()).map(|t| t.text.as_str()).collect();
            return Err(JurlError::Unresolved(format!("{} (jev disabled)", bad.join(", "))));
        }
        let api_key = std::env::var("OPENROUTER_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .or_else(|| cfg.jev.api_key.clone())
            .ok_or_else(|| JurlError::Jev("no API key. run `jurl setup` or set OPENROUTER_API_KEY (needed to interpret ambiguous words)".into()))?;
        let oracle = jev::client::OpenRouter {
            api_key,
            model: cfg.jev.model.clone(),
            timeout: std::time::Duration::from_millis(cfg.jev.timeout_ms),
            max_retries: 3,
        };
        let built = jev::prompt::build(&tokens);
        let n = built.questions.len();
        let t0 = Instant::now();
        let res = oracle.decide(built.state, built.questions)?;
        jev_info = Some(output::JevInfo { model: res.model.clone(), questions: n, ms: t0.elapsed().as_millis(), usage: res.usage.clone() });
        jev::prompt::apply(&mut tokens, &res.answers);
        let mut is_get = tokens.iter().any(|t| t.role == Some(Role::Method) && t.value() == "GET");
        if let Some(p) = res.answers.get("get_intent").and_then(|a| a.noul()) {
            if p > 0.5 && !tokens.iter().any(|t| t.role == Some(Role::Method)) {
                is_get = true;
                get_intent = Some(p);
            }
        }
        repair::pair_key_values(&mut tokens, is_get);
    }

    if opts.explain {
        output::explain(&tokens, jev_info.as_ref());
        if let Some(p) = get_intent {
            eprintln!("jev: read-only lookup p={p:.2} → GET, key/value words as query");
        }
    }

    // 4–5. 組み立て。
    let req = assemble::assemble(&tokens, &cfg.defaults.content_type)?;

    // 6. confidence で 実行 / 確認 / 中止。
    let unsafe_method = matches!(req.method.as_str(), "PUT" | "PATCH" | "DELETE");
    let confirm_below = if unsafe_method { cfg.jev.confirm_below_unsafe } else { cfg.jev.confirm_below };
    if req.confidence < cfg.jev.reject_below {
        if !opts.explain {
            output::explain(&tokens, jev_info.as_ref());
        }
        return Err(JurlError::LowConfidence(req.confidence, cfg.jev.reject_below));
    }

    let argv = curl::argv(&req, !opts.raw, &passthrough, &cfg.defaults.curl_args);

    // 7. dry-run か実行。
    if opts.dry_run {
        println!("{}", curl::render(&curl::argv(&req, false, &passthrough, &cfg.defaults.curl_args)));
        return Ok(0);
    }
    let need_confirm = match cfg.jev.confirm.as_str() {
        "never" => false,
        "confidence" => req.confidence < confirm_below,
        _ => jev_info.is_some() || req.confidence < confirm_below,
    };
    if need_confirm && !opts.yes {
        eprintln!("{}", curl::render(&curl::argv(&req, false, &passthrough, &cfg.defaults.curl_args)));
        let why = if jev_info.is_some() { format!("interpreted by jev, confidence {:.2}", req.confidence) } else { format!("confidence {:.2}", req.confidence) };
        if !output::confirm(&format!("run this? ({why})")) {
            return Err(JurlError::Aborted);
        }
    }
    let out = curl::run(&argv)?;
    output::print_response(&out.stdout, opts.raw)?;
    Ok(out.status)
}
