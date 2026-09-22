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

    let cfg = config::load()?;
    let words = config::expand_aliases(&cfg, &words);

    // 1–2. 規則で分類。全部決まれば高速パス。
    let mut tokens = rules::classify(&words);
    let mut jev_info: Option<output::JevInfo> = None;

    // 3. 決まらなかったものがあれば jev に全トークンを渡す。
    if tokens.iter().any(|t| !t.resolved()) {
        if opts.no_jev || !cfg.jev.enabled {
            let bad: Vec<&str> = tokens.iter().filter(|t| !t.resolved()).map(|t| t.text.as_str()).collect();
            return Err(JurlError::Unresolved(format!("{} (jev disabled)", bad.join(", "))));
        }
        let api_key = std::env::var("OPENROUTER_API_KEY")
            .map_err(|_| JurlError::Jev("OPENROUTER_API_KEY is not set (needed to interpret ambiguous words)".into()))?;
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
        let is_get = tokens.iter().any(|t| t.role == Some(Role::Method) && t.value() == "GET");
        repair::pair_key_values(&mut tokens, is_get);
    }

    if opts.explain {
        output::explain(&tokens, jev_info.as_ref());
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
    if req.confidence < confirm_below && !opts.yes {
        let summary = summarize(&req);
        if !output::confirm(&format!("{summary} (confidence {:.2}) — continue?", req.confidence)) {
            return Err(JurlError::Aborted);
        }
    }
    let out = curl::run(&argv)?;
    output::print_response(&out.stdout, opts.raw)?;
    Ok(out.status)
}

fn summarize(req: &assemble::Request) -> String {
    let mut s = format!("{} {}", req.method, req.url.render());
    match &req.body {
        Some(body::Body::Json(v)) => s.push_str(&format!(" {v}")),
        Some(body::Body::Form(kv)) | Some(body::Body::Multipart(kv)) => {
            s.push(' ');
            s.push_str(&kv.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&"));
        }
        Some(body::Body::File(f)) => s.push_str(&format!(" @{f}")),
        None => {}
    }
    s
}
