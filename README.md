<h1 align="center">jurl</h1>
<p align="center"><b>jev × curl</b> — 順番も表記もバラバラな単語を投げると、curl になって返ってくる。</p>

```sh
$ jurl psot localhsot 3000 users first_name amanda

curl \
  -sS \
  -X POST \
  http://localhost:3000/users \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json' \
  --data '{"first_name":"amanda"}'

run this? (interpreted by jev, confidence 0.98) [Y/n/e]
```

タイポ(`psot` `localhsot`)、ポートだけの `3000`、分かれたキーと値(`first_name amanda`)。
どの順で並べても同じ curl になる。

## どう動くか

```
words ──▶ rules ──▶ 全部決まった? ──yes──▶ curl を組み立てて実行
                         │
                         no
                         ▼
                   jev に 1 回聞く ──▶ 確認 [Y/n/e] ──▶ 実行
```

- **rules** — `post` `k=v` `k==v` `Name:value` `@file` `-k` のような形はコードで決める。全部決まればオフラインで即実行
- **jev** — 決まらない単語があれば、全単語の役割を jev(TypeSafe System One、OpenRouter 経由)に **1 リクエスト**で聞く。jev は選択肢から選んで確率を返すだけで、curl 文字列は生成させない
- **確認** — jev が関わった解釈は curl を見せてから実行。`e` で `$EDITOR` を開いて直せる。confidence が低ければ実行しない

1 回 200–600 ms、$0.0002 以下。

## Setup

```sh
cargo install --path .
jurl setup        # OpenRouter の API キーを ~/.config/jurl/config.toml に保存(0600)
jurl demo         # public API を叩く 12 例を ↑↓ で選んで試す
```

`OPENROUTER_API_KEY` があればそちらが優先。curl が PATH にあること。

## 書き方

| こう書く | こうなる |
|---|---|
| `get` `post` `put` … | メソッド(省略時: ボディがあれば POST、無ければ GET) |
| `localhost` `:3000/users` `api.example.com/x` | URL(https 既定、localhost / 私有 IP は http) |
| `json` `form` `multipart` | Content-Type(既定 json) |
| `a.b=1` `a[0]=x` | ボディ(文字列) |
| `a:=1` `a:=true` | ボディ(JSON リテラル) |
| `k==v` | クエリ |
| `Name:value` | ヘッダ(jev には送らない) |
| `@file` | ボディをファイルから |
| `-k` `--max-time 5` | curl にそのまま |
| それ以外 | jev が決める。`title hello world` のように分かれた値も 1 つに結合 |

| フラグ | |
|---|---|
| `-n` | curl を表示して終了 |
| `--explain` | 各単語の役割・confidence・rule か jev か |
| `-y` | 確認を省略 |
| `--body` / `--raw` | ヘッダなし / 整形なし |

## 出力

端末では httpie と同じ並び: status line、ヘッダ、空行、ボディ(JSON は整形・色付き)。パイプに繋ぐとボディだけ。exit code は curl のもの。

## 設定(省略可)

`~/.config/jurl/config.toml`

```toml
[jev]
confirm = "jev"          # "jev" | "confidence" | "never"
confirm_below = 0.8
reject_below = 0.5

[defaults]
curl_args = ["--max-time", "30"]

[aliases]
lh = "localhost"
tok = "Authorization:Bearer $TOKEN"
```

---

<p align="center"><sub>例は <a href="EXAMPLES.md">EXAMPLES.md</a>、各モジュールの役割は <code>src/</code> の冒頭コメントに。jev のワイヤ形式は eg-jev の <code>packages/recipes/src/lib/{openrouter,questions}.ts</code> に合わせている。</sub></p>
