# Zenoh の調査メモ

このリポジトリのサンプルで再現した問題の仕組みのまとめ。
ソースの位置は crates.io から取得した各バージョンのもの（`~/.cargo/registry/src/*/zenoh-1.8.0/` など）で、GitHub では <https://github.com/eclipse-zenoh/zenoh/tree/1.8.0> にあたる。

## 目次

1. [peer の参加と宣言の全量 push](01-peer-join-declarations.md) — peer-join-stall / client-join。p2p では参加した peer に全宣言を書き込みロックを持ったまま送る。client は interest に合う宣言しか受け取らない
2. [ExprId の枯渇](02-expr-id-exhaustion.md) — expr-id-exhaustion / expr-id-exhaustion-fork。`get_next_local_id` の線形探索、u16 の上限、セッション側カウンタの折り返し、fork の修正
3. [ワイルドカード querier の matching](03-wildcard-querier-matching.md) — wildcard-querier-matching / wildcard-query-retry。1.9.0 以降 `remote_queryable_matching_status` が包含を要求する
4. [client と scouting / connect](04-client-scouting.md) — client-scouting。`scouting/multicast/autoconnect` と `connect/timeout_ms` の client 既定値
5. [宣言されていないキーへの put のコスト](05-put-route-cost.md) — put-route-cost / put-route-fix。リソースのないキーは経路がキャッシュされず `Resource::get_matches` が毎回走る

## 関連する upstream の issue / PR（2026-11-14 時点）

| 番号 | 内容 | 状態 |
| --- | --- | --- |
| [eclipse-zenoh/zenoh#2825](https://github.com/eclipse-zenoh/zenoh/issues/2825) | face の u16 ExprId を使い切るとルータが固まる | open |
| [eclipse-zenoh/zenoh#2835](https://github.com/eclipse-zenoh/zenoh/pull/2835) | ExprId の回収、枯渇時に回り続けない | open |
| [eclipse-zenoh/zenoh#2709](https://github.com/eclipse-zenoh/zenoh/pull/2709) | 応答しないとして閉じている transport への push を即失敗させる | merged, 1.10.2 に含まれる |
| [logivations/zenoh#8](https://github.com/logivations/zenoh/pull/8) | 別の fork。同じ固まり方を報告し、`decl_key` の送信を非ブロッキングにした | — |
