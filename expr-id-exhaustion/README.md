# expr-id-exhaustion

1 本のリンクで u16 の ExprId を使い切ると、セッションの宣言が二度と返らなくなる問題の再現（zenoh 1.8.0、release ビルド）。

- peer B（親プロセス）が先にセッション A（子プロセス）に `tcp/127.0.0.1:17541` で繋ぐ（マルチキャスト scouting なし）。
- その後 A が N = 70 000 個の liveliness token を別々のキー `demo/token/{i}` に宣言する。宣言のたびに B 向けの face で ExprId が 1 つ使われる。
- A の別スレッドの見張りが、10 s 以上返らない宣言を報告する。30 s 返らなければ親が `REPRODUCED` を出し、子を kill して終わる（全体の上限は 600 s）。
- 全部返った場合は、B が最後のトークン `demo/token/69999` を liveliness の `get` で見られるかを確かめる（fork 版で使う）。

参加の後で宣言するので、[peer-join-stall](../peer-join-stall) の参加時の全量 push は起きない。宣言が 1 つずつ B に送られる。

queryable ではなく token を使う理由（queryable だとセッション側のカウンタが先に折り返し、固まらずに黙って壊れる）と仕組みは [docs/02](../docs/02-expr-id-exhaustion.md)、修正は [expr-id-exhaustion-fork](../expr-id-exhaustion-fork)。

## 実行

```sh
make run              # N = 70000、release
```

debug ビルド（`cargo run -- 70000`）だと、止まる代わりに `get_next_local_id` の `id += 1` のオーバーフローで panic するはず（ここでは試していない）。

## 出力（2026-11-15, macOS, release）

```
[B] connected as peer via tcp/127.0.0.1:17541
[A] B connected, declaring 70000 liveliness tokens
[A] declared 5000 at 363.3ms
[A] declared 10000 at 1.2s
[A] declared 15000 at 2.8s
[A] declared 20000 at 5.3s
[A] declared 25000 at 8.7s
[A] declared 30000 at 12.8s
[A] declared 35000 at 17.6s
[A] declared 40000 at 23.2s
[A] declared 45000 at 33.2s
[A] declared 50000 at 42.1s
[A] declared 55000 at 52.4s
[A] declared 60000 at 63.1s
[A] declared 65000 at 75.6s
[A] declaration #65537 has not returned for 10 s
[A] declaration #65537 has not returned for 20 s
[A] declaration #65537 has not returned for 30 s
REPRODUCED: A hung at declaration #65537 of 70000; it has not returned for 30s (65 535 ExprIds) (after 108.8s)
```

65 535 個の番号を使い切った後、65 536 個目には予約済みの 0 が振られ（release では `id += 1` が 0 に折り返す）、**65 537 個目**で `get_next_local_id` のループが終わらなくなる。そのままルーティングテーブルの書き込みロックを持ち続けるので、A はもう何もできない。
同じ日に 3 回実行し、3 回とも #65537 で止まった（100〜317 s。遅い回は別のビルドが並行していた）。
宣言 1 個あたりの時間は N とともに伸びる（線形探索なので O(N²)）。
