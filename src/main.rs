mod assemble;
mod body;
mod cli;
mod color;
mod config;
mod curl;
mod demo;
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
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let typed = shell_words::join(&argv);
    let parsed = cli::parse(argv);
    match run(parsed, &typed) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("jurl: {e}");
            std::process::exit(e.exit_code());
        }
    }
}

/// `typed` は打ち込まれた引数そのもの(edit 画面のコメント用)。
fn run(parsed: cli::Parsed, typed: &str) -> Result<i32, JurlError> {
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
    // `jurl demo [N]`: 例を選んで、残りのフラグ(-n, --explain, -y …)はそのまま効かせる。
    if words[0] == "demo" {
        // demo は解釈を見せるのが目的なので、--explain を常に付ける。
        let opts = cli::Opts { explain: true, ..opts };
        if let Some(n) = words.get(1) {
            let ex = demo::pick(n)?;
            let w: Vec<String> = ex.words.iter().map(|s| s.to_string()).collect();
            eprintln!("{}\n", color::paint(color::stderr_enabled(), color::C::Dim, &format!("$ jurl {}", demo::join(&w))));
            return execute(&opts, w, passthrough, &format!("demo {n}  ({})", demo::join(ex.words)));
        }
        let mut last = 0;
        let mut at = 0;
        while let Some((i, ex)) = demo::ask(at)? {
            at = i;
            let w: Vec<String> = ex.words.iter().map(|s| s.to_string()).collect();
            eprintln!("\n{}\n", color::paint(color::stderr_enabled(), color::C::Dim, &format!("$ jurl {}", demo::join(&w))));
            // 1 例の失敗(中止・低 confidence・HTTP エラー)でメニューを抜けない。
            match execute(&opts, w, passthrough.clone(), &format!("demo  ({})", demo::join(ex.words))) {
                Ok(code) => last = code,
                Err(e) => {
                    eprintln!("jurl: {e}");
                    last = e.exit_code();
                }
            }
            eprintln!();
        }
        return Ok(last);
    }

    execute(&opts, words, passthrough, typed)
}

fn execute(opts: &cli::Opts, words: Vec<String>, passthrough: Vec<String>, typed: &str) -> Result<i32, JurlError> {
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
                let Some(edited) = output::edit_command(&curl::render_with(&plain, false), typed)? else {
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
                let Some(edited) = output::edit_command(&curl::render_with(&plain, false), typed)? else {
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
