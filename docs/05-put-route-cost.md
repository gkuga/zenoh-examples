# 宣言されていないキーへの put のコスト

サンプル: [put-route-cost](../put-route-cost)（再現）、[put-route-fix](../put-route-fix)（解決）

## 何が起きるか

あるデーモンを 2026-11-15 にプロファイルしたところ、CPU の約 3 分の 1 が `Session::put` の下の `Resource::get_matches` → `zenoh_keyexpr ... Intersector::intersect` に使われていた。
そのデーモンは 30 秒ごとに N 個の別々の、**宣言されていない** キー（`<key>/@state`）に put し、同じセッションと繋がった peer が、キーごとに subscriber 2 つと queryable 2 つ程度を具体的なキーで宣言していた。

## 仕組み（zenoh 1.8.0）

`zenoh/src/net/routing/dispatcher/pubsub.rs` の `get_data_route`:

```rust
let compute_route = || hat_code.compute_data_route(tables, expr, local_context, face.whatami);
if let Some(data_routes) = expr
    .resource()
    .as_ref()
    .and_then(|res| res.context.as_ref())
    .map(|ctx| &ctx.data_routes)
{
    return get_or_set_route(data_routes, tables.routes_version, ...);
}
compute_route()
```

- キーにリソースがあれば、経路はそのリソースの `data_routes` にキャッシュされ、`routes_version` が変わったときだけ計算し直す。
- 宣言されていないキーにはリソースがないので、put のたびに `compute_route` が走る。p2p の `compute_data_route` は、リソースがないキーについて `Resource::get_matches` で宣言済みのリソースすべてとキー式を交差させる。だから put 1 回のコストが宣言の数 D に比例し、N 個のキーへの一巡は N × D になる。

## 解決

- **(a) キーごとに `Publisher` を一度だけ宣言し、それを通して put する。** 宣言でリソースができるので、経路がキャッシュされる。1.8.0 でも効く（put-route-fix で、D を 0 から 20 000 に増やしても 1 回あたりのコストは変わらない）。ただし publisher も宣言なので、[01](01-peer-join-declarations.md) / [02](02-expr-id-exhaustion.md) の数え方では 1 キー 1 宣言が増える。
- **(b) 多数の値を 1 回の put にまとめる。** 交差の計算は put 1 回分だけになる。その 1 回はまだ D に比例するが、一巡あたり K 回が 1 回になる。宣言も増えない。

計測値は各サンプルの README にある。
