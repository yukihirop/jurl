# jurl

jev × curl。順番も表記もバラバラな単語列から「言いたかった HTTP リクエスト」を復元して curl を叩く。

```sh
jurl post localhost application/json profile.first_name=job profile.family_name=amanda
jurl profile.first_name=job localhost json POST          # 順不同
jurl psot localhsot 3000 users first_name job             # タイポ・分離・省略
jurl example.com/items page 2 get                         # GET なら key value はクエリ
jurl localhost/login form user=me pass=x -k               # curl のフラグはそのまま通す
```

- 規則で全部決まる入力は jev を呼ばない(高速パス、オフライン)
- 決まらない単語が 1 つでもあれば、全単語の役割を jev(TypeSafe System One)に 1 リクエストで聞く
- jev は選択肢から選ぶだけ。curl への変換はコード側で決定的
- `--dry-run` で組み立てた curl を表示、`--explain` で各単語の分類と jev のコストを表示

## Setup

```sh
cargo install --path .
jurl setup                      # OpenRouter の API キーを聞いて ~/.config/jurl/config.toml に 0600 で保存し、jev に疎通確認
```

環境変数 `OPENROUTER_API_KEY` があればそちらが優先(CI やシェルで既に入れている人向け)。

curl が PATH にあること。

## 使い方

```
jurl [words ...] [flags] [-- curl args]
```

| 単語の形 | 意味 |
|---|---|
| `get` `post` `put` … | メソッド(省略時: ボディがあれば POST、無ければ GET) |
| `localhost` `:3000/users` `example.com/x` `http://…` | URL(スキーム省略時は https、localhost / 私有 IP は http) |
| `json` `form` `multipart` `application/json` | Content-Type(既定 json) |
| `a.b=1` `a[0]=x` | ボディ(値は文字列) |
| `a:=1` `a:=true` | ボディ(JSON リテラル) |
| `k==v` | クエリ |
| `Name:value` `-H "Name: value"` | ヘッダ(この形だけ。jev には送らない) |
| `@file` | ボディをファイルから |
| `-k` `--max-time 5` … | curl にそのまま渡す |

上のどれにも当てはまらない単語(`psot`、`users`、`first_name` `job` のような分離したキー/値、裸の `3000`)は jev が役割を決める。

| フラグ | |
|---|---|
| `-n, --dry-run` | curl コマンドを表示して終了 |
| `--explain` | 各単語の役割・confidence・規則/jev どちらで決めたか、jev のコストを stderr に |
| `--no-jev` | jev を呼ばない(`JURL_NO_JEV=1` でも可) |
| `-y, --yes` | 確認を省略 |
| `--raw` | レスポンスを整形せずそのまま |

jev が関わった解釈は、組み立てた curl をそのまま見せて `[Y/n/e]` を聞いてから実行する(規則だけで決まった入力は即実行)。`e` で `$EDITOR`(無ければ `vi`)が開くので、間違っていればそこで直して保存すればその内容で実行される。confidence(解釈全体の最小値)が 0.5 未満なら実行はせず、curl を見せて `[e/N]`(直すか止めるか)だけ聞く。`confirm = "confidence"` にすると 0.8 未満(PUT / PATCH / DELETE は 0.9 未満)のときだけ確認、`"never"` で確認なし。

## 設定(全部省略可)

`~/.config/jurl/config.toml`(`JURL_CONFIG` で変更):

```toml
[jev]
enabled = true
api_key = "..."                 # jurl setup が書く。env の OPENROUTER_API_KEY が優先
model = "typesafe/jev-1.13"     # JEV_MODEL でも上書き可
confirm = "jev"                 # "jev" | "confidence" | "never"
confirm_below = 0.8
reject_below = 0.5
confirm_below_unsafe = 0.9
timeout_ms = 5000

[defaults]
content_type = "json"
curl_args = ["--max-time", "30"]

[aliases]
lh = "localhost"
tok = "Authorization:Bearer $TOKEN"   # $VAR は環境変数で展開
```

## 出力

- TTY: `HTTP 201 · 12ms` の行 + JSON なら整形
- パイプ / `--raw`: ボディだけ stdout、ステータス行は stderr
- exit code は curl のものを継承。jev 側の失敗・低 confidence は 2、curl が無ければ 127

## 構成

```
src/
  cli.rs        jurl 自身のフラグ
  rules.rs      規則による役割分類(同義語表、curl オプション表)
  jev/          OpenRouter Decisions router への問い合わせ(state / questions の組み立て、answers の書き戻し)
  repair.rs     jev の答えは質問ごとに独立なので、key value の交互配置をコードで直す
  assemble.rs   役割付きトークン → Request
  body.rs       a.b[0].c → nested JSON / form 平坦化
  url.rs        スキーム補完、:port、path、query
  curl.rs       Request → argv、dry-run 表示、spawn
  output.rs     ステータス行、整形、--explain
  config.rs     env + config.toml
```

jev のワイヤ形式は [eg-jev](../../JavaScriptProjects/eg-jev) の `packages/recipes/src/lib/{openrouter,questions}.ts` に合わせている。
