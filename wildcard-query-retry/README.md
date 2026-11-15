# wildcard-query-retry

[wildcard-querier-matching](../wildcard-querier-matching) の解決方法: ワイルドカードの `matching_status()` を問い合わせの条件にしない（zenoh 1.10.2）。

peer A は B より 300 ms 遅れて queryable を宣言する（後から立ち上がるサービスのつもり）。B は `r/fam/*/status` のすべてが欲しい。

1. **retry**: タイムアウト 500 ms の `get` を出し、応答がなければ 200 ms 待って再試行する（最大 5 回）。matching は見ない。
2. **concrete**: 具体的なキーごとの querier。こちらは `matching_status()` が正しい。
3. **gated（やってはいけない書き方）**: ワイルドカードの querier が一致を報告するまで最大 2 秒待ってから問い合わせる。A がとっくに立ち上がった後に試しても、一致が来ないので何も問い合わせない。

127.0.0.1:17561 の TCP、マルチキャスト scouting なし。

もう 1 つの解決方法は 1.8.0 に留まることで、1.8.0 では 3 の書き方でも動く（同じ `main.rs` を 1.8.0 でビルドして 2026-11-15 に確認: `3. gated on matching: 2 replies`、判定は `OK: all three got the replies; the gate works on this version (v1.8.0-release)`。このリポジトリには 1.8.0 用のマニフェストは置いていない）。ただし 1.8.0 には [peer-join-stall](../peer-join-stall) と [expr-id-exhaustion](../expr-id-exhaustion) の問題があり、閉じかけの transport への push を即失敗させる修正 ([#2709](https://github.com/eclipse-zenoh/zenoh/pull/2709)) も 1.10.2 にしか入っていない。詳しくは [docs/03](../docs/03-wildcard-querier-matching.md)。

## 実行

```sh
make run
```

## 出力（2026-11-15, macOS）

```
zenoh v1.10.2-release
1. retry attempt 1 at 264.50µs: 0 replies
1. retry attempt 2 at 201.89ms: 0 replies
1. retry attempt 3 at 404.67ms: 2 replies
2. concrete r/fam/one/status: matching=true replies=1
2. concrete r/fam/two/status: matching=true replies=1
3. gated on matching: 0 replies []
OK: the gated query got nothing, but retrying the wildcard got 2 and concrete keys got 2 (v1.10.2-release)
```

A がまだいない間の `get` は応答 0 ですぐ終わり（タイムアウトを待たない）、A が宣言した後の 3 回目で 2 つとも返っている。
