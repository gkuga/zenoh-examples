# ExprId の枯渇

サンプル: [expr-id-exhaustion](../expr-id-exhaustion)（再現）、[expr-id-exhaustion-fork](../expr-id-exhaustion-fork)（fork の修正）

## ExprId とは

Zenoh はキー式を毎回文字列で送らず、リンク（face）ごとに番号を振って `DeclareKeyExpr` で相手に教え、以後は番号で送る。
この番号が `ExprId` で、`zenoh-protocol/src/core/wire_expr.rs` の `pub type ExprId = u16;`。0 は「接頭辞なし」に予約されているので、1 本のリンクで使えるのは 1〜65 535。
番号が解放されるのは相手が undeclare したときと face が閉じたときだけ（[#2825](https://github.com/eclipse-zenoh/zenoh/issues/2825)）。

## 1.8.0 の割り当て

`zenoh/src/net/routing/dispatcher/face.rs`:

```rust
pub(crate) fn get_next_local_id(&self) -> ExprId {
    let mut id = 1;
    while self.local_mappings.contains_key(&id) || self.remote_mappings.contains_key(&id) {
        id += 1;
    }
    id
}
```

- 毎回 1 から探すので、k 個使用中なら k 回調べる。N 個送ると O(N²)。これがルーティングテーブルの書き込みロックの中で走る（`Resource::decl_key` から呼ばれる）。
- 65 535 個すべて使用中のとき:
  - debug ビルドは `id += 1` のオーバーフローで panic する。
  - release ビルドは 0 に折り返す。0 はマップに入っていないので、65 536 個目には予約済みの 0 が振られてしまう。
  - 65 537 個目では 0 も使用中なので、ループが終わらない。書き込みロックを持ったままなので、そのセッションの宣言も `put` もすべて止まる。

expr-id-exhaustion はちょうど **65 537 個目の宣言**で止まることを確かめている。

## queryable ではなく liveliness token を使う理由

1.8.0 の `declare_queryable` と `declare_subscriber` は、キーの非ワイルドカード部分を **セッション自身の** u16 カウンタ（`Session` の `expr_id_counter`、`// @TODO: manage rollover and uniqueness`）で先に宣言する（`declare_nonwild_prefix` → `declare_prefix`）。
このカウンタは単純な `fetch_add` なので 65 536 個目で折り返し、ルータ側が `Resource 1 remapped. Remapping unsupported!` を出して以後の宣言を既存のリソースに重ねてしまう。face 側の新しい番号は要らなくなるので、face の枯渇には届かない。

実際に 1 つのセッションで queryable を 70 000 個宣言すると（2026-11-15, release）:

- 宣言は 108 s ですべて返った（止まらない）
- `Remapping unsupported!` が 4 464 回（= 70 000 − 65 536）出た
- B から最後の `demo/decl/69999` に `get` しても応答がなかった

つまり 1 セッションから queryable を 65 536 個以上宣言すると、固まりはしないが黙って壊れる。
face の枯渇は、複数のセッション（たとえばルータや peer に繋がった複数の client）からの宣言が 1 本のリンクに集まると起きる。liveliness token は接頭辞を宣言せず完全なキーで送られるので（`declare_liveliness_inner`）、1 セッションでもそれを再現できる。

## fork の修正

[gkuga/zenoh@25c6174](https://github.com/gkuga/zenoh/commit/25c6174e6477c355c4db6fd09e2044523fb73b37)（タグ `nue-1.8.0-expr-id-2`）は 1.8.0 に 2 つのコミットを足したもの:

- **ラップするカーソル。** 0 を飛ばすカーソルから番号を出し、相手の undeclare で解放された番号の集合を優先して再利用する。候補は今までどおり両方のマップと照合する。償却 O(1)。
- **枯渇の扱い。** 全部使用中なら `decl_key` は番号を振らずに完全なキー式を送る。face は使い切ったことを覚えておき、以後は探し直さない。警告は face ごとに 1 回（`No expression id left, sending ... and later key expressions in full`）。相手の undeclare でフラグを戻す。

wire は変わらない（`ExprId` は u16 のまま）。使い方は Cargo.toml に次を足すだけ:

```toml
[patch.crates-io]
zenoh = { git = "https://github.com/gkuga/zenoh", tag = "nue-1.8.0-expr-id-2" }
```

zenoh はリポジトリ内の他の `zenoh-*` クレートに path で依存しているので、`Cargo.lock` の `zenoh-*` はすべてこの commit のものになる。

この修正は枯渇でノードが固まらないようにするもので、[01](01-peer-join-declarations.md) のロックを持ったままの push は残る。fork 側の計測（制御キュー 16）では、peer が N 個の宣言を持つセッションに参加して最初の応答を得るまで、10k で 4.55 s → 3.17 s、20k で 33.1 s → 26.2 s だった。
