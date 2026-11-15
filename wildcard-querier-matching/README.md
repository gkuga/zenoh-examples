# wildcard-querier-matching

ワイルドカードの querier が、zenoh 1.9.0 以降 `matching_status()` で一致を報告しない問題の再現。

peer A が `r/fam/one/status` と `r/fam/two/status` に queryable を宣言し、peer B が具体的なキー 1 つとワイルドカード 3 つの querier を宣言する。それぞれについて `matching_status()`、`matching_listener` が 1 秒以内に発火したか、`get` の応答数を表示する。
A と B は 1 プロセス内の 2 セッションで、127.0.0.1:17551 の TCP で繋ぎ、マルチキャスト scouting は切っている。

仕組みは [docs/03](../docs/03-wildcard-querier-matching.md)、解決方法は [wildcard-query-retry](../wildcard-query-retry)。

## 実行

```sh
make run         # zenoh 1.10.2 (Cargo.toml)
make run-1.8.0   # 同じ src/main.rs を zenoh 1.8.0 で (zenoh-1.8.0/Cargo.toml)
make compare     # 両方
```

Cargo は semver 互換な zenoh の 2 バージョンを 1 つのプロジェクトに入れられないので、バージョンの切り替えは feature ではなく 2 つ目のマニフェスト `zenoh-1.8.0/Cargo.toml` にしている（`[[bin]] path = "../src/main.rs"`）。

## 出力（2026-11-15, macOS）

zenoh 1.10.2:

```
zenoh v1.10.2-release
querier r/fam/one/status   matching_status=true  listener_event=Some(true)  get replies=1
querier r/fam/*/status     matching_status=false listener_event=None        get replies=2
querier r/**               matching_status=false listener_event=None        get replies=2
querier r/fam/one/*        matching_status=false listener_event=None        get replies=1
REPRODUCED: 3 querier(s) get replies but report no match: ["r/fam/*/status", "r/**", "r/fam/one/*"] (v1.10.2-release)
```

zenoh 1.8.0:

```
zenoh v1.8.0-release
querier r/fam/one/status   matching_status=true  listener_event=Some(true)  get replies=1
querier r/fam/*/status     matching_status=true  listener_event=Some(true)  get replies=2
querier r/**               matching_status=true  listener_event=Some(true)  get replies=2
querier r/fam/one/*        matching_status=true  listener_event=Some(true)  get replies=1
OK: every querier that gets replies reports a match (v1.8.0-release)
```

1.10.2 ではワイルドカードの 3 つが `matching_status=false` で listener も発火しないのに、`get` には 1.8.0 と同じ数の応答が返っている。
