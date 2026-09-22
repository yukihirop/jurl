# 動作確認用の例

[public-apis/public-apis](https://github.com/public-apis/public-apis) にある認証不要の API を jurl で叩く例。
全部 2026-09-22 に実際に叩いて通ったもの(API 側のレスポンスは変わりうる)。

`jev` と書いてあるものは `OPENROUTER_API_KEY` が必要(1 回 $0.0001〜0.0002)。それ以外はオフラインの高速パス。
まず `-n`(dry-run)で組み立てた curl を見て、外して実行するのがおすすめ。`--explain` で各単語の分類が見える。

## きれいな入力(jev なし)

```sh
jurl api.agify.io name==michael                       # クエリ
jurl dog.ceo/api/breeds/image/random
jurl catfact.ninja/fact
jurl api.zippopotam.us/jp/100-0001
jurl api.ipify.org format==json
jurl pokeapi.co/api/v2/pokemon/pikachu                # 長い JSON。TTY なら整形される
jurl api.chucknorris.io/jokes/random
jurl api.open-meteo.com/v1/forecast latitude==35.68 longitude==139.69 current_weather==true
jurl api.github.com/repos/rust-lang/rust 'Accept:application/vnd.github+json'
jurl restcountries.com/v3.1/name/japan fields==name,capital -L      # 301 を追う。curl のフラグはそのまま通る

jurl post httpbin.org/post json profile.first_name=job profile.family_name=amanda
jurl httpbin.org/post form user=me pass=x             # application/x-www-form-urlencoded
jurl put jsonplaceholder.typicode.com/posts/1 id:=1 title=changed   # := は JSON リテラル
```

## 崩れた入力(jev が役割を決める)

```sh
# キーと値が分かれている。メソッド無し → jev が「読み取り」と判断して GET + クエリ
jurl api.agify.io name michael --explain
#   → GET https://api.agify.io/?name=michael

# パスも分かれている + 数値 / 真偽
jurl api.open-meteo.com/v1 forecast latitude 35.68 longitude 139.69 current_weather true
#   → GET https://api.open-meteo.com/v1/forecast?latitude=35.68&longitude=139.69&current_weather=true

# タイポ + 分離したキー値 → POST の JSON
jurl httpbin.org/anything psot user me role admin
#   → POST https://httpbin.org/anything  {"user":"me","role":"admin"}

# メソッドが最後、値は型付け(userId 1 → 1)
jurl jsonplaceholder.typicode.com/posts title hello body world userId 1 post
#   → POST  {"title":"hello","body":"world","userId":1}  (201)

# パスが 2 つに分かれている。confidence が 0.57 なので確認プロンプトが出る(-y で省略)
jurl api.zippopotam.us jp 100-0001
#   → GET https://api.zippopotam.us/jp/100-0001
```

## 見てほしいところ

- `--explain` の `by` 列: `rule` はオフラインで決めた、`jev` は jev が決めた
- `note` の `field_key → field_value (paired)`: jev は単語ごとに独立に答えるので、`key value key value` の交互配置はコードで直している
- `jev:` 行の ms と $: 1 回 250〜600 ms、$0.0002 以下
- confidence が 0.8 未満なら解釈を見せて確認、0.5 未満なら実行しない
