# peer-join-stall

宣言を大量に持つセッションに **peer** として参加すると止まる問題の再現（zenoh 1.8.0）。

- セッション A（子プロセス）が N 個の queryable を `demo/decl/{i}` に宣言し、加えて `demo/target` に応答する queryable を 1 つ置く。宣言後は 100 ms ごとに `demo/tick` に put し続け、1 秒以上返らない put があれば報告する（別スレッドの見張り）。
- セッション B（親プロセス）が **peer** として `tcp/127.0.0.1:17501` に繋ぎ、`demo/target` に `get`（タイムアウト 2 s）を繰り返す。30 s 以内に最初の応答が来るまでの時間を測る。
- マルチキャスト scouting は切っている。A を別プロセスにしているのは、同じプロセスだと zenoh のランタイムのスレッドを共有してしまうため。

p2p では参加した peer に全宣言を送り、その間ルーティングテーブルの書き込みロックを持ったままになる。仕組みは [docs/01](../docs/01-peer-join-declarations.md)、解決方法は [client-join](../client-join)。

## 実行

```sh
make run            # N = 40000
make run N=20000
```

## 出力（2026-11-15, macOS, release）

```
[A] READY 40000 queryables declared in 23.62s
[B] joining as peer via tcp/127.0.0.1:17501
[B] session open after 501.70ms
[A] put on demo/tick blocked for 1 s so far
[A] put on demo/tick blocked for 2 s so far
[B] no reply yet at 2.50s
[A] put on demo/tick blocked for 3 s so far
[A] put on demo/tick blocked for 4 s so far
[B] no reply yet at 4.61s
[A] put on demo/tick blocked for 5 s so far
[A] 2026-11-15T09:05:05.230857Z ERROR acc-0 ThreadId(16) zenoh_transport::unicast::universal::tx: Unable to push non droppable network message to 44e0537e904e7afbb38ef9cc8fc8b5be. Closing transport!
[A] put on demo/tick blocked for 6 s so far
...
REPRODUCED: peer B got no reply within 30s with N=40000; A's put blocked for up to 30 s; A closed the link 6 time(s)
```

- B は 30 秒待っても応答を得られない。
- A の `put` は B が参加した直後から 30 秒間返らない（ログの `put ... blocked for N s so far`）。
- A は 5 秒ごとに `Unable to push non droppable network message ... Closing transport!` でリンクを閉じ、B が繋ぎ直すたびに同じことが起きる。

N を変えたときの結果（同じ日、同じ機械）:

| N | B の最初の応答 | A の put の停止 | A がリンクを閉じた回数 |
| --- | --- | --- | --- |
| 1 000 | 0.50 s | なし | 0 |
| 10 000 | 1.01 s | なし | 0 |
| 20 000 | 5.40 s | なし | 0 |
| 30 000 | 30 s 以内になし | 1 s 以上の停止なし | 6 |
| 40 000 | 30 s 以内になし | 30 s | 6 |

0.5 s は peer の `zenoh::open` が `scouting/delay`（500 ms）を待って戻るまでの時間で、それより遅い分が宣言の push による。
この Mac では詰まり始めるのが 20 000 から 30 000 の間だった。別の環境（arm64 のエッジ機）では 10 000 で約 11 s、20 000 で 15 s 以内に応答なしと、もっと小さい N で詰まった。

A が N 個を宣言する時間自体も N の 2 乗で伸びる（40 000 で約 22 s）。[docs/01](../docs/01-peer-join-declarations.md) の最後を参照。
