# peer の参加と宣言の全量 push

サンプル: [peer-join-stall](../peer-join-stall)（再現）、[client-join](../client-join)（解決）

## 何が起きるか

queryable などの宣言をたくさん持つセッション A に、別のセッション B が **peer** として TCP で参加すると:

- B の最初の `get` に応答が返らない（N が大きいと何秒待っても返らない）
- A 自身の `put` が止まる
- 5 秒後に A がリンクを閉じ（`Unable to push non droppable network message to ... Closing transport!`）、B が繋ぎ直すと同じことを繰り返す

同じ A に **client** として参加すると、4 万 queryable でも数 ms で応答する。

## 仕組み（zenoh 1.8.0）

1. **p2p では参加した peer に全宣言を送る。** `zenoh/src/net/routing/hat/p2p_peer/mod.rs` には "at connection … peers act as if they received an interest CurrentFuture with id 0" とあり、`queries_new_face` などが既存の queryable をすべて新しい face に送る。
2. **その送信はルーティングテーブルの書き込みロックを持ったまま行われる。** 宣言の送信は `Resource::decl_key` が `DeclareKeyExpr` を出し、宣言は `CongestionControl::Block` なので、制御用キュー（`transport/link/tx/queue/size/control`、既定 2 バッチ）が空くまでブロックする。
3. **新しいリンクの受信ループは参加処理が終わるまで始まらない。** `zenoh-transport/src/unicast/manager.rs` に "the read loop … remains blocked until new_unicast() returns" とある。キューが詰まると、参加処理が終わらないまま待ち続ける。
4. **ローカルの `put` も同じロックを待つ。** `route_data` がテーブルの読み込みロックを取るので、参加処理の間は A の `put` が返らない。
5. **キューへの push が `wait_before_close`（5 秒）以上詰まると transport を閉じる**（`zenoh-transport/src/unicast/universal/tx.rs` の `handle_push_result`）。B は `connect/endpoints` に従って繋ぎ直し、また最初からになる。
6. さらに、ExprId の割り当て `FaceState::get_next_local_id` は 1 から線形に探すので、N 個の宣言を送ると O(N²) になる（[02](02-expr-id-exhaustion.md)）。

`transport/link/tx/queue/size/control`（最大 16）や `wait_before_close` を大きくすると詰まり始める N は動くが、曲線そのものは変わらない。

## client だと速い理由

zenoh 1.0 から client と peer の subsystem には宣言を流さなくなり、client は自分が送った interest に合う宣言だけを受け取る（[zenoh 1.0 のリリース記事](https://zenoh.io/blog/2024-10-21-zenoh-firesong/): "subscribers, queryables and liveliness tokens declarations are not propagated any more to clients and peers subsystems"）。
client-join の B は宣言も interest も持たずに `get` するだけなので、参加時に A から送られる宣言はない。

client にするには:

- A は固定のエンドポイントで listen する（`listen/endpoints: ["tcp/127.0.0.1:17511"]`）
- B は `mode: "client"` で、`connect/endpoints` に A を書く
- `scouting/multicast/enabled: false`（client は既定では scouting で見つけた peer に繋がらない。[04](04-client-scouting.md)）

## 計測（2026-11-15, macOS, release）

B が参加してから最初の応答が返るまで:

| N (queryable 数) | peer | client |
| --- | --- | --- |
| 1 000 | 0.50 s | — |
| 10 000 | 1.01 s | 3 ms |
| 20 000 | 5.40 s | 3 ms |
| 30 000 | 30 s 以内に応答なし、リンクを 6 回閉じた | — |
| 40 000 | 30 s 以内に応答なし、A の `put` が 30 s 止まり、リンクを 6 回閉じた | 5〜8 ms |

peer の 0.5 s は、peer の `zenoh::open` が scouting の待ち（`scouting/delay` 500 ms）で戻るまでの時間。
別の環境（arm64 のエッジ機）では 10 000 で約 11 s、20 000 で 15 s 以内に応答なしだった。どの N から詰まるかは機械によって違う。

なお A が N 個を宣言する時間そのものも O(N²) だった（10 000 で 1.1 s、20 000 で 5.2 s、40 000 で 21 s）。1.8.0 の `Session::declare_prefix` が `local_resources` を線形に探しているため（`zenoh/src/api/session.rs`）。
