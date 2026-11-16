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

use zenoh::pubsub::Publisher;
use zenoh::Wait;

/// The D values measured by default.
const DEFAULT_DS: &str = "0,1000,5000,10000,20000";

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

/// Opens session A listening on `endpoint` and peer B connecting to it, and
/// waits until A sees B.
async fn open_linked_pair(endpoint: &str) -> (zenoh::Session, zenoh::Session) {
    let a = zenoh::open(config("listen", endpoint)).await.unwrap();
    let b = zenoh::open(config("connect", endpoint)).await.unwrap();
    while a.info().peers_zid().await.count() == 0 {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    (a, b)
}

/// Parses a comma-separated list of D values.
fn parse_ds(list: &str) -> Vec<usize> {
    list.split(',').map(|s| s.parse().unwrap()).collect()
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

/// Adds declarations for keys `from..to` on A and on B, then lets B's reach A.
async fn declare_both(a: &zenoh::Session, b: &zenoh::Session, from: usize, to: usize) {
    declare(a, from, to);
    declare(b, from, to);
    tokio::time::sleep(Duration::from_millis(500)).await;
}

/// Runs `f` and returns µs per value for `k` values.
fn per_value(k: usize, f: impl FnOnce()) -> f64 {
    let start = Instant::now();
    f();
    start.elapsed().as_secs_f64() * 1e6 / k as f64
}

/// The undeclared way: `session.put` to each of `k` keys (the problem, for reference).
fn put_undeclared(session: &zenoh::Session, k: usize) {
    for j in 0..k {
        session
            .put(format!("demo/point/{j}/@state"), "1")
            .wait()
            .unwrap();
    }
}

/// (a) One put through each publisher.
fn put_through(publishers: &[Publisher<'_>]) {
    publishers.iter().for_each(|p| p.put("1").wait().unwrap())
}

/// (b) One put carrying every entry (here plain "key=value" lines).
fn put_aggregated(session: &zenoh::Session, k: usize) {
    let mut payload = String::with_capacity(k * 32);
    for j in 0..k {
        payload.push_str(&format!("demo/point/{j}=1\n"));
    }
    session.put("demo/points/@state", payload).wait().unwrap();
}

/// The measurement at one D, in µs per value.
struct Row {
    /// How many keys have declarations (4 each).
    d: usize,
    undeclared: f64,
    /// Round 2 through the publishers.
    publisher: f64,
    aggregated: f64,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let k: usize = args.get(1).map_or(10_000, |s| s.parse().unwrap());
    let ds = parse_ds(args.get(2).map_or(DEFAULT_DS, String::as_str));
    let port: u16 = args.get(3).map_or(17582, |s| s.parse().unwrap());
    let endpoint = format!("tcp/127.0.0.1:{port}");

    // The sessions are kept until the exit below, since closing or dropping one
    // blocks for about 10 s.
    let (a, b) = open_linked_pair(&endpoint).await;
    let publishers = declare_publishers(&a, k);
    let rows = measure(&a, &b, &publishers, k, &ds).await;
    report(&rows, k);
    // Exit right away instead of dropping the sessions.
    std::process::exit(0);
}

/// (a) Declares one publisher per key, once, before any other declaration.
fn declare_publishers(session: &zenoh::Session, k: usize) -> Vec<Publisher<'static>> {
    let start = Instant::now();
    let publishers = (0..k)
        .map(|j| {
            session
                .declare_publisher(format!("demo/point/{j}/@state"))
                .wait()
                .unwrap()
        })
        .collect();
    println!("declared {k} publishers in {:.2?}", start.elapsed());
    publishers
}

/// For each D in `ds`, in order, adds declarations up to D keys, then times the
/// three ways of sending `k` values from A. Prints a row per D.
async fn measure(
    a: &zenoh::Session,
    b: &zenoh::Session,
    publishers: &[Publisher<'_>],
    k: usize,
    ds: &[usize],
) -> Vec<Row> {
    println!("us per value, K = {k} values, per D keys (4 declarations each):");
    println!(
        "{:>7}  {:>11}  {:>11}  {:>11}  {:>11}",
        "D", "undeclared", "pub round1", "pub round2", "aggregated"
    );
    let mut rows = Vec::new();
    let mut declared = 0;
    for &d in ds {
        declare_both(a, b, declared, d).await;
        declared = d;
        let undeclared = per_value(k, || put_undeclared(a, k));
        // Round 1 runs right after the new declarations, round 2 again.
        let round1 = per_value(k, || put_through(publishers));
        let round2 = per_value(k, || put_through(publishers));
        let aggregated = per_value(k, || put_aggregated(a, k));
        println!("{d:>7}  {undeclared:>11.2}  {round1:>11.2}  {round2:>11.2}  {aggregated:>11.3}");
        rows.push(Row {
            d,
            undeclared,
            publisher: round2,
            aggregated,
        });
    }
    rows
}

/// Prints the verdict: whether both ways stay far below the undeclared put at the last D.
fn report(rows: &[Row], k: usize) {
    let (first, last) = (&rows[0], &rows[rows.len() - 1]);
    // (b)'s single put still matches its one key against every declaration, so its
    // per-entry figure grows with D too, but it is one put per sweep instead of K.
    let ms = |us: f64| us * k as f64 / 1000.0;
    let ok = last.publisher * 100.0 < last.undeclared && last.aggregated * 100.0 < last.undeclared;
    println!(
        "{}: at D={} an undeclared put costs {:.2} us ({:.0} ms per sweep of {k}); \
         (a) a publisher put {:.2} us (at D={}: {:.2} us); \
         (b) the aggregated put {:.2} ms per sweep (at D={}: {:.2} ms)",
        if ok { "OK" } else { "NOT FIXED" },
        last.d,
        last.undeclared,
        ms(last.undeclared),
        last.publisher,
        first.d,
        first.publisher,
        ms(last.aggregated),
        first.d,
        ms(first.aggregated),
    );
}
