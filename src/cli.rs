//! jurl 自身のフラグを取り出す。残りは全部トークンとして規則分類に回す。

pub const HELP: &str = "\
jurl — jev x curl. Turn loosely ordered words into a curl command and run it.

usage: jurl [words ...] [flags] [-- curl args]

  words   any order: method, url, content-type, key=value, key:=json,
          key==query, Name:value (header), @file, curl flags (-k, --max-time 5)

commands:
  setup           save your OpenRouter API key to ~/.config/jurl/config.toml (0600)

flags:
  -n, --dry-run   print the curl command instead of running it
      --explain   show how each word was classified (stderr)
      --no-jev    never call jev; unresolved words are an error
  -y, --yes       skip the [Y/n/e] confirmation (shown whenever jev interpreted the words;
                  e opens the curl command in $EDITOR)
      --raw       print the response body untouched, no status line
  -h, --help
  -V, --version

env:
  OPENROUTER_API_KEY   required for jev
  JEV_MODEL            default typesafe/jev-1.13
  JURL_NO_JEV=1        same as --no-jev
  JURL_CONFIG          config path (default ~/.config/jurl/config.toml)
";

#[derive(Debug, Default, Clone)]
pub struct Opts {
    pub dry_run: bool,
    pub explain: bool,
    pub no_jev: bool,
    pub yes: bool,
    pub raw: bool,
    pub help: bool,
    pub version: bool,
}

pub struct Parsed {
    pub opts: Opts,
    pub words: Vec<String>,
    pub passthrough: Vec<String>,
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Parsed {
    let mut opts = Opts::default();
    let mut words = Vec::new();
    let mut passthrough = Vec::new();
    let mut after_dashdash = false;
    for a in args {
        if after_dashdash {
            passthrough.push(a);
            continue;
        }
        match a.as_str() {
            "--" => after_dashdash = true,
            "-n" | "--dry-run" => opts.dry_run = true,
            "--explain" => opts.explain = true,
            "--no-jev" => opts.no_jev = true,
            "-y" | "--yes" => opts.yes = true,
            "--raw" => opts.raw = true,
            "-h" | "--help" => opts.help = true,
            "-V" | "--version" => opts.version = true,
            _ => words.push(a),
        }
    }
    if std::env::var("JURL_NO_JEV").map(|v| v == "1").unwrap_or(false) {
        opts.no_jev = true;
    }
    Parsed { opts, words, passthrough }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_flags_words_and_passthrough() {
        let p = parse(["post", "-n", "localhost", "--", "-k", "--max-time", "5"].map(String::from));
        assert!(p.opts.dry_run);
        assert_eq!(p.words, ["post", "localhost"]);
        assert_eq!(p.passthrough, ["-k", "--max-time", "5"]);
    }
}
