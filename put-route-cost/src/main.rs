//! A put to an undeclared key costs more the more declarations a session holds (zenoh 1.8.0).
//!
//! Session A and peer B are linked over loopback TCP. For each of D keys, A and B each
//! declare a subscriber and a queryable on `demo/thing/{i}/value` (4 declarations per
//! key). Then A puts once to each of K distinct, undeclared keys `demo/point/{j}/@state`,
//! none of which matches a declaration. Routing an undeclared key computes its matches
//! (`Resource::get_matches`) against every declared resource, so the time per put grows
//! with D.
//!
//! Usage: put-route-cost [K] [D,D,...] [PORT]

use std::time::{Duration, Instant};

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

/// Puts once to each of `k` undeclared keys; returns µs per put.
fn sweep(session: &zenoh::Session, k: usize) -> f64 {
    let start = Instant::now();
    for j in 0..k {
        session
            .put(format!("demo/point/{j}/@state"), "1")
            .wait()
            .unwrap();
    }
    start.elapsed().as_secs_f64() * 1e6 / k as f64
}

/// The measurement at one D.
struct Row {
    /// How many keys have declarations (4 each).
    d: usize,
    /// µs per put to an undeclared key.
    us_per_put: f64,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let k: usize = args.get(1).map_or(10_000, |s| s.parse().unwrap());
    let ds = parse_ds(args.get(2).map_or(DEFAULT_DS, String::as_str));
    let port: u16 = args.get(3).map_or(17581, |s| s.parse().unwrap());
    let endpoint = format!("tcp/127.0.0.1:{port}");

    // The sessions are kept until the exit below, since closing or dropping one
    // blocks for about 10 s.
    let (a, b) = open_linked_pair(&endpoint).await;
    let rows = measure(&a, &b, k, &ds).await;
    report(&rows);
    // Exit right away instead of dropping the sessions.
    std::process::exit(0);
}

/// For each D in `ds`, in order, adds declarations up to D keys, then times `k`
/// puts from A to undeclared keys. Prints a row per D.
async fn measure(a: &zenoh::Session, b: &zenoh::Session, k: usize, ds: &[usize]) -> Vec<Row> {
    println!("K = {k} puts to distinct undeclared keys, per D keys (4 declarations each)");
    println!("{:>7}  {:>10}  {:>10}", "D", "us/put", "sweep");
    let mut rows = Vec::new();
    let mut declared = 0;
    for &d in ds {
        declare_both(a, b, declared, d).await;
        declared = d;
        let us = sweep(a, k);
        println!("{d:>7}  {us:>10.2}  {:>9.0}ms", us * k as f64 / 1000.0);
        rows.push(Row { d, us_per_put: us });
    }
    rows
}

/// Prints the verdict: whether a put got slower from the first D to the last.
fn report(rows: &[Row]) {
    let (first, last) = (&rows[0], &rows[rows.len() - 1]);
    let ratio = last.us_per_put / first.us_per_put;
    if ratio > 3.0 {
        println!(
            "REPRODUCED: a put to an undeclared key took {:.2} us at D={} and {:.2} us at D={} ({ratio:.0}x)",
            first.us_per_put, first.d, last.us_per_put, last.d
        );
    } else {
        println!(
            "NOT REPRODUCED: {:.2} us at D={}, {:.2} us at D={} ({ratio:.1}x)",
            first.us_per_put, first.d, last.us_per_put, last.d
        );
    }
}
