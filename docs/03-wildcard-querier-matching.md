# ワイルドカード querier の matching

サンプル: [wildcard-querier-matching](../wildcard-querier-matching)（再現、`make compare` で 1.8.0 と比較）、[wildcard-query-retry](../wildcard-query-retry)（解決）

## 何が起きるか

peer A が `r/fam/one/status` と `r/fam/two/status` に queryable を宣言し、peer B が querier を宣言する。

| querier のキー | 1.8.0 の `matching_status()` | 1.10.2 の `matching_status()` | `get` の応答 (両方) |
| --- | --- | --- | --- |
| `r/fam/one/status` | true | true | 1 |
| `r/fam/*/status` | true | **false** | 2 |
| `r/**` | true | **false** | 2 |
| `r/fam/one/*` | true | **false** | 1 |

1.10.2 ではワイルドカードの querier は `matching_status()` が false で、`matching_listener` も発火しない。`get` そのものには応答が返る。
1.9.0, 1.10.0, 1.10.1, 1.10.2 で同じ結果で、1.8.0 だけが true になる。このリポジトリの Makefile で比べるのは 1.10.2 と 1.8.0 だけで、1.9.0〜1.10.1 は同じ構成の別の検証プログラムで 2026-11-15 に確かめた（3 つのワイルドカードすべて `status=false listener_event=None`）。

## 原因

1.10.2 の `zenoh/src/net/routing/hat/broker/queries.rs` の `remote_queryable_matching_status`:

```rust
let is_matching_res = |qabl: &Resource| {
    ...
    ke.includes(key_expr)          // queryable のキーが querier のキーを含むか
};
let is_matching_info = |info: &QueryableInfoType| !complete || info.complete;
```

queryable のキー `ke` が querier のキーを **含む** ことを、querier が `complete` を求めていなくても要求している。`r/fam/one/status` は `r/fam/*/status` を含まないので、false になる。

1.8.0 の `hat/p2p_peer/queries.rs` の `get_matching_queryables` は、キーが **交差する** リソース（`ctx.matches`）を集め、包含は `complete` のときだけ調べていた:

```rust
for mres in matches.iter() {
    if complete && !KeyExpr::keyexpr_include(mres.expr(), key_expr) {
        continue;
    }
    ...
}
```

`get` のルーティングは交差で決まるので、1.10.2 では「matching は false なのに `get` には応答がある」というずれになる。

## 解決

1. **ワイルドカードの matching を条件にして問い合わせない。** `get` をタイムアウト付きで出し、応答が来なければ少し待って再試行する。応答そのものが「誰がいるか」の答えになる。
2. **具体的なキーで問い合わせる。** 具体的なキーの querier なら matching は正しい。
3. **1.8.0 に留まる。** 1.8.0 では交差で判定するので、ワイルドカードでも true になる。ただし 1.8.0 には [01](01-peer-join-declarations.md) と [02](02-expr-id-exhaustion.md) の問題があり、[#2709](https://github.com/eclipse-zenoh/zenoh/pull/2709)（閉じかけの transport への push を即失敗させる）も 1.10.2 にしか入っていない。どちらを取るかはトレードオフ。

wildcard-query-retry は 1 と 2 を示し、比較のために「matching を 2 秒待ってから問い合わせる」書き方が何も得られないことも示す。
