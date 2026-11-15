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
    let port: u16 = args.get(3).map_or(17581, |s| s.parse().unwrap());
    let endpoint = format!("tcp/127.0.0.1:{port}");

    let a = zenoh::open(config("listen", &endpoint)).await.unwrap();
    let b = zenoh::open(config("connect", &endpoint)).await.unwrap();
    while a.info().peers_zid().await.count() == 0 {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    println!("K = {k} puts to distinct undeclared keys, per D keys (4 declarations each)");
    println!("{:>7}  {:>10}  {:>10}", "D", "us/put", "sweep");
    let mut rows = Vec::new();
    let mut declared = 0;
    for &d in &ds {
        declare(&a, declared, d);
        declare(&b, declared, d);
        declared = d;
        // Let B's declarations reach A.
        tokio::time::sleep(Duration::from_millis(500)).await;
        let us = sweep(&a, k);
        println!("{d:>7}  {us:>10.2}  {:>9.0}ms", us * k as f64 / 1000.0);
        rows.push((d, us));
    }

    let (first, last) = (rows[0], rows[rows.len() - 1]);
    let ratio = last.1 / first.1;
    if ratio > 3.0 {
        println!(
            "REPRODUCED: a put to an undeclared key took {:.2} us at D={} and {:.2} us at D={} ({ratio:.0}x)",
            first.1, first.0, last.1, last.0
        );
    } else {
        println!(
            "NOT REPRODUCED: {:.2} us at D={}, {:.2} us at D={} ({ratio:.1}x)",
            first.1, first.0, last.1, last.0
        );
    }
    // Closing or dropping a session blocks for about 10 s, so exit right away.
    std::process::exit(0);
}
