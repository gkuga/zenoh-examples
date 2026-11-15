# put-route-cost

宣言を多く持つセッションほど、宣言されていないキーへの `put` が遅くなる問題の再現（zenoh 1.8.0）。

- セッション A と peer B を `tcp/127.0.0.1:17581` で繋ぐ（マルチキャスト scouting なし）。宣言は繋いだ後に行うので、[peer-join-stall](../peer-join-stall) の参加時の push は起きない。
- D 個のキー `demo/thing/{i}/value` それぞれに、A と B が subscriber と queryable を 1 つずつ宣言する（キーあたり 4 宣言、具体的なキー）。D は 0, 1 000, 5 000, 10 000, 20 000 と増やしていく。
- 各 D で、A が K = 10 000 個の別々の宣言されていないキー `demo/point/{j}/@state` に 1 回ずつ put し、1 回あたりの時間を測る。どの宣言にも一致しないので、データはどこにも送られない。測っているのは経路の計算だけ。

宣言されていないキーにはリソースがないので経路がキャッシュされず、put のたびに `Resource::get_matches` で宣言済みの全リソースと交差を取る。仕組みは [docs/05](../docs/05-put-route-cost.md)、解決方法は [put-route-fix](../put-route-fix)。

## 実行

```sh
make run
make run K=5000 D=0,2000,4000
```

## 出力（2026-11-15, macOS, release）

```
K = 10000 puts to distinct undeclared keys, per D keys (4 declarations each)
      D      us/put       sweep
      0        0.35          4ms
   1000       24.44        244ms
   5000      138.45       1384ms
  10000      293.44       2934ms
  20000      813.87       8139ms
REPRODUCED: a put to an undeclared key took 0.35 us at D=0 and 813.87 us at D=20000 (2317x)
```

put 1 回が D = 0 の 0.35 µs から D = 20 000 の 814 µs まで、ほぼ D に比例して（20 000 では比例以上に）伸びる。30 秒ごとに 10 000 キーへ put するなら、一巡に 8 秒かかる。
同じ日の 1 回目の実行では D = 20 000 で 1 015 µs（このときは別のビルドが並行して走っていた）。
