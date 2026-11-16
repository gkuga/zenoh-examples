//! A querier on a wildcard never reports a match (zenoh 1.9.0 and later).
//!
//! Peer A declares queryables on `r/fam/one/status` and `r/fam/two/status`.
//! Peer B declares queriers on a concrete key and on three wildcards, and prints for each
//! its `matching_status()`, whether its `matching_listener` fired, and how many replies
//! its `get` received. On zenoh 1.10.2 the wildcard queriers report no match although
//! their `get` is answered; on 1.8.0 every querier matches.
//!
//! Usage: wildcard-querier-matching [PORT]

use std::time::Duration;

use zenoh::query::Queryable;

const QUERYABLES: [&str; 2] = ["r/fam/one/status", "r/fam/two/status"];
const QUERIERS: [&str; 4] = ["r/fam/one/status", "r/fam/*/status", "r/**", "r/fam/one/*"];

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

/// Opens peer A listening on `endpoint` and peer B connecting to it, and waits
/// until B sees A.
async fn open_linked_pair(endpoint: &str) -> (zenoh::Session, zenoh::Session) {
    let a = zenoh::open(config("listen", endpoint)).await.unwrap();
    let b = zenoh::open(config("connect", endpoint)).await.unwrap();
    while b.info().peers_zid().await.count() == 0 {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    (a, b)
}

/// Declares a queryable on each of `QUERYABLES`, replying from its own key.
async fn declare_queryables(session: &zenoh::Session) -> Vec<Queryable<()>> {
    let mut queryables = Vec::new();
    for key in QUERYABLES {
        let q = session.declare_queryable(key).callback(move |q| {
            tokio::spawn(async move { q.reply(key, "v").await.unwrap() });
        });
        queryables.push(q.await.unwrap());
    }
    queryables
}

/// What one querier reports, and what its `get` receives.
struct Probe {
    /// What `matching_listener` reported within 1 s, if anything.
    event: Option<bool>,
    /// `matching_status()`, read after the listener's second.
    status: bool,
    /// How many replies its `get` received.
    replies: usize,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let port: u16 = std::env::args()
        .nth(1)
        .map_or(17551, |s| s.parse().unwrap());
    let endpoint = format!("tcp/127.0.0.1:{port}");
    println!("zenoh {}", zenoh::GIT_VERSION);

    // The sessions are kept until the exit below, since closing or dropping one
    // blocks for about 10 s.
    let (a, b) = open_linked_pair(&endpoint).await;
    // Keep the queryables to the end: dropping one undeclares it.
    let _queryables = declare_queryables(&a).await;
    // Give the declarations time to reach B.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut wrong = Vec::new();
    for key in QUERIERS {
        let p = probe(&b, key).await;
        println!(
            "querier {key:<18} matching_status={:<5} listener_event={:<11} get replies={}",
            p.status,
            format!("{:?}", p.event),
            p.replies
        );
        if p.replies > 0 && !p.status {
            wrong.push(key);
        }
    }
    report(&wrong);
    // Exit right away instead of dropping the sessions.
    std::process::exit(0);
}

/// Declares a querier on `key` and probes it: its matching listener, its matching
/// status, then one `get`.
async fn probe(session: &zenoh::Session, key: &'static str) -> Probe {
    let querier = session
        .declare_querier(key)
        .timeout(Duration::from_secs(2))
        .await
        .unwrap();
    let listener = querier.matching_listener().await.unwrap();
    let event = tokio::time::timeout(Duration::from_secs(1), listener.recv_async()).await;
    let event = event.ok().and_then(|r| r.ok()).map(|s| s.matching());
    let status = querier.matching_status().await.unwrap().matching();
    let replies = querier.get().await.unwrap();
    let mut answered = 0;
    while let Ok(reply) = replies.recv_async().await {
        answered += reply.result().is_ok() as usize;
    }
    Probe {
        event,
        status,
        replies: answered,
    }
}

/// Prints the verdict: `wrong` lists the queriers that got replies but report no match.
fn report(wrong: &[&str]) {
    if wrong.is_empty() {
        println!(
            "OK: every querier that gets replies reports a match ({})",
            zenoh::GIT_VERSION
        );
    } else {
        println!(
            "REPRODUCED: {} querier(s) get replies but report no match: {wrong:?} ({})",
            wrong.len(),
            zenoh::GIT_VERSION
        );
    }
}
