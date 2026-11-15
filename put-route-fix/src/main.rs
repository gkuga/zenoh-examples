//! Two ways around the per-put matching cost of `put-route-cost` (zenoh 1.8.0).
//!
//! The same setup: session A and peer B, each declaring a subscriber and a queryable
//! on D keys. A sends K values, each for its own key `demo/point/{j}/@state`:
//! - undeclared: `session.put` per key (the problem, for reference);
//! - (a) publisher: a `Publisher` declared once per key at the start, put through it
//!   twice per D (round 1 right after the new declarations, round 2 again);
//! - (b) aggregated: one put of all K entries on one key, `demo/points/@state`.
//!
//! Usage: put-route-fix [K] [D,D,...] [PORT]

use std::time::{Duration, Instant};

use zenoh::Wait;

fn config(kind: &str, endpoint: &str) -> zenoh::Config {
    let mut c = zenoh::Config::default();
    c.insert_json5("mode", "\"peer\"").unwrap();
    // No multicast scouting: the sessions only meet through the endpoint below.
    c.insert_json5("scouting/multicast/enabled", "false")
        .unwrap();
    if kind == "connect" {
        c.insert_json5("listen/endpoints", "[]").unwrap();
    }
    c.insert_json5(&format!("{kind}/endpoints"), &format!("[\"{endpoint}\"]"))
        .unwrap();
    c
}

/// Declares a subscriber and a queryable on keys `from..to`, kept alive by leaking them.
fn declare(session: &zenoh::Session, from: usize, to: usize) {
    for i in from..to {
        let key = format!("demo/thing/{i}/value");
        std::mem::forget(
            session
                .declare_subscriber(&key)
                .callback(|_| {})
                .wait()
                .unwrap(),
        );
        std::mem::forget(
            session
                .declare_queryable(&key)
                .callback(|_| {})
                .wait()
                .unwrap(),
        );
    }
}

/// Runs `f` and returns µs per value for `k` values.
fn per_value(k: usize, f: impl FnOnce()) -> f64 {
    let start = Instant::now();
    f();
    start.elapsed().as_secs_f64() * 1e6 / k as f64
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let k: usize = args.get(1).map_or(10_000, |s| s.parse().unwrap());
    let ds: Vec<usize> = args
        .get(2)
        .map_or("0,1000,5000,10000,20000", String::as_str)
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect();
    let port: u16 = args.get(3).map_or(17582, |s| s.parse().unwrap());
    let endpoint = format!("tcp/127.0.0.1:{port}");

    let a = zenoh::open(config("listen", &endpoint)).await.unwrap();
    let b = zenoh::open(config("connect", &endpoint)).await.unwrap();
    while a.info().peers_zid().await.count() == 0 {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // (a) One publisher per key, declared once, before any other declaration.
    let start = Instant::now();
    let publishers: Vec<_> = (0..k)
        .map(|j| {
            a.declare_publisher(format!("demo/point/{j}/@state"))
                .wait()
                .unwrap()
        })
        .collect();
    println!("declared {k} publishers in {:.2?}", start.elapsed());

    println!("us per value, K = {k} values, per D keys (4 declarations each):");
    println!(
        "{:>7}  {:>11}  {:>11}  {:>11}  {:>11}",
        "D", "undeclared", "pub round1", "pub round2", "aggregated"
    );
    let mut rows = Vec::new();
    let mut declared = 0;
    for &d in &ds {
        declare(&a, declared, d);
        declare(&b, declared, d);
        declared = d;
        // Let B's declarations reach A.
        tokio::time::sleep(Duration::from_millis(500)).await;

        let undeclared = per_value(k, || {
            for j in 0..k {
                a.put(format!("demo/point/{j}/@state"), "1").wait().unwrap();
            }
        });
        let round1 = per_value(k, || {
            publishers.iter().for_each(|p| p.put("1").wait().unwrap())
        });
        let round2 = per_value(k, || {
            publishers.iter().for_each(|p| p.put("1").wait().unwrap())
        });
        // (b) One put carrying every entry (here plain "key=value" lines).
        let aggregated = per_value(k, || {
            let mut payload = String::with_capacity(k * 32);
            for j in 0..k {
                payload.push_str(&format!("demo/point/{j}=1\n"));
            }
            a.put("demo/points/@state", payload).wait().unwrap();
        });
        println!("{d:>7}  {undeclared:>11.2}  {round1:>11.2}  {round2:>11.2}  {aggregated:>11.3}");
        rows.push((d, undeclared, round2, aggregated));
    }

    let (first, last) = (rows[0], rows[rows.len() - 1]);
    // (b)'s single put still matches its one key against every declaration, so its
    // per-entry figure grows with D too, but it is one put per sweep instead of K.
    let ms = |us: f64| us * k as f64 / 1000.0;
    let ok = last.2 * 100.0 < last.1 && last.3 * 100.0 < last.1;
    println!(
        "{}: at D={} an undeclared put costs {:.2} us ({:.0} ms per sweep of {k}); \
         (a) a publisher put {:.2} us (at D={}: {:.2} us); \
         (b) the aggregated put {:.2} ms per sweep (at D={}: {:.2} ms)",
        if ok { "OK" } else { "NOT FIXED" },
        last.0,
        last.1,
        ms(last.1),
        last.2,
        first.0,
        first.2,
        ms(last.3),
        first.0,
        ms(first.3),
    );
    // Closing or dropping a session blocks for about 10 s, so exit right away.
    std::process::exit(0);
}
