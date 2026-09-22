mod assemble;
mod body;
mod cli;
mod color;
mod config;
mod curl;
mod error;
mod interpret;
mod jev;
mod output;
mod repair;
mod rules;
mod setup;
mod token;
mod url;

use error::JurlError;

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
        let out = interpret::interpret(&mut tokens, &oracle)?;
        jev_info = Some(out.info);
        get_intent = out.get_intent;
    }

    if opts.explain {
        output::explain(&tokens, jev_info.as_ref());
        if let Some(p) = get_intent {
            eprintln!("{}", color::paint(color::stderr_enabled(), color::C::Dim, &format!("jev: read-only lookup p={p:.2} → GET, key/value words as query")));
        }
    }

    // 4–5. 組み立て。
    let req = assemble::assemble(&tokens, &cfg.defaults.content_type)?;

    // 6. confidence で 実行 / 確認 / 中止。
    let unsafe_method = matches!(req.method.as_str(), "PUT" | "PATCH" | "DELETE");
    let confirm_below = if unsafe_method { cfg.jev.confirm_below_unsafe } else { cfg.jev.confirm_below };
    // 低すぎる confidence: 実行はしない。curl を見せて、直す(e)か止めるか。
    let mut edited_low: Option<Vec<String>> = None;
    if req.confidence < cfg.jev.reject_below {
        if !opts.explain {
            output::explain(&tokens, jev_info.as_ref());
        }
        if opts.dry_run || opts.yes {
            return Err(JurlError::LowConfidence(req.confidence, cfg.jev.reject_below));
        }
        let plain = curl::argv(&req, false, &passthrough, &cfg.defaults.curl_args);
        eprintln!("\n{}\n", curl::render_with(&plain, color::stderr_enabled()));
        match output::confirm_edit(&format!("confidence {:.2} is too low to run as is. edit it?", req.confidence)) {
            output::Choice::Edit => {
                let Some(edited) = output::edit_command(&curl::render_with(&plain, false))? else {
                    return Err(JurlError::Aborted);
                };
                eprintln!("\n{}\n", curl::render_with(&edited, color::stderr_enabled()));
                edited_low = Some(edited);
            }
            _ => return Err(JurlError::LowConfidence(req.confidence, cfg.jev.reject_below)),
        }
    }

    let argv = curl::argv(&req, !opts.raw, &passthrough, &cfg.defaults.curl_args);

    // 7. dry-run か実行。
    if opts.dry_run {
        println!("{}", curl::render_with(&curl::argv(&req, false, &passthrough, &cfg.defaults.curl_args), color::stdout_enabled()));
        return Ok(0);
    }
    let need_confirm = match cfg.jev.confirm.as_str() {
        "never" => false,
        "confidence" => req.confidence < confirm_below,
        _ => jev_info.is_some() || req.confidence < confirm_below,
    };
    let mut argv = argv;
    if let Some(e) = edited_low {
        argv = curl::with_status(e, !opts.raw);
    } else if need_confirm && !opts.yes {
        let plain = curl::argv(&req, false, &passthrough, &cfg.defaults.curl_args);
        eprintln!("\n{}\n", curl::render_with(&plain, color::stderr_enabled()));
        let why = if jev_info.is_some() { format!("interpreted by jev, confidence {:.2}", req.confidence) } else { format!("confidence {:.2}", req.confidence) };
        match output::confirm(&format!("run this? ({why})")) {
            output::Choice::Yes => {}
            output::Choice::No => return Err(JurlError::Aborted),
            output::Choice::Edit => {
                let Some(edited) = output::edit_command(&curl::render_with(&plain, false))? else {
                    return Err(JurlError::Aborted);
                };
                eprintln!("\n{}\n", curl::render_with(&edited, color::stderr_enabled()));
                // ステータス行用の -w は jurl が付け直す。
                argv = curl::with_status(edited, !opts.raw);
            }
        }
    }
    let out = curl::run(&argv)?;
    output::print_response(&out.stdout, opts.raw)?;
    Ok(out.status)
}
