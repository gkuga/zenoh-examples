//! ExprId exhaustion with the fork patch (gkuga/zenoh commit 25c6174e).
//!
//! The same program as `expr-id-exhaustion`, built against zenoh 1.8.0 plus a fix:
//! expression ids come from a wrapping cursor (amortized O(1)) instead of a linear
//! scan, and once every u16 id is in use the face sends full key expressions instead,
//! warning once. So A declares all N liveliness tokens and B still sees the last one.
//!
//! Usage: expr-id-exhaustion-fork [N] [PORT]

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use zenoh::liveliness::LivelinessToken;
use zenoh::Wait;

/// A declaration that has not returned for this long is reported as a hang.
const HANG: Duration = Duration::from_secs(30);
/// Hard limit for the whole run.
const LIMIT: Duration = Duration::from_secs(600);

fn config(mode: &str, listen: Option<&str>, connect: Option<&str>) -> zenoh::Config {
    let mut c = zenoh::Config::default();
    c.insert_json5("mode", &format!("\"{mode}\"")).unwrap();
    // No multicast scouting: the sessions only meet through the endpoints below.
    c.insert_json5("scouting/multicast/enabled", "false")
        .unwrap();
    let listen = listen.map(|e| format!("[\"{e}\"]")).unwrap_or("[]".into());
    c.insert_json5("listen/endpoints", &listen).unwrap();
    if let Some(e) = connect {
        c.insert_json5("connect/endpoints", &format!("[\"{e}\"]"))
            .unwrap();
    }
    c
}

/// Session A: waits for B, then declares `n` liveliness tokens.
async fn serve(n: usize, endpoint: &str) {
    zenoh::init_log_from_env_or("warn");
    exit_with_parent();

    let session = zenoh::open(config("peer", Some(endpoint), None))
        .await
        .unwrap();
    while session.info().peers_zid().await.count() == 0 {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    println!("B connected, declaring {n} liveliness tokens");

    let declared = Arc::new(AtomicUsize::new(0));
    watch_declarations(declared.clone(), n);
    // Keep the tokens to the end: dropping one undeclares it.
    let _tokens = declare_tokens(&session, n, &declared);
    std::future::pending::<()>().await;
}

/// Exits when the parent goes away (its end of our stdin closes).
fn exit_with_parent() {
    std::thread::spawn(|| {
        let mut line = String::new();
        while std::io::stdin()
            .read_line(&mut line)
            .map_or(false, |r| r > 0)
        {}
        std::process::exit(0);
    });
}

/// Reports, every 10 s, a declaration that has not returned, until all `n` have.
/// It runs on its own thread, since a hung declaration blocks its thread.
fn watch_declarations(declared: Arc<AtomicUsize>, n: usize) {
    std::thread::spawn(move || {
        // The count last seen, and for how many seconds it has not moved.
        let (mut last, mut stuck) = (0, 0);
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let d = declared.load(Ordering::SeqCst);
            if d >= n {
                return;
            }
            if d == last {
                stuck += 1;
                if stuck % 10 == 0 {
                    println!("declaration #{} has not returned for {stuck} s", d + 1);
                }
            } else {
                (last, stuck) = (d, 0);
            }
        }
    });
}

/// Declares a liveliness token on each of `demo/token/0..n`, counting them in
/// `declared` and printing progress every 5000.
fn declare_tokens(
    session: &zenoh::Session,
    n: usize,
    declared: &AtomicUsize,
) -> Vec<LivelinessToken> {
    let start = Instant::now();
    let mut tokens = Vec::with_capacity(n);
    for i in 0..n {
        tokens.push(
            session
                .liveliness()
                .declare_token(format!("demo/token/{i}"))
                .wait()
                .unwrap(),
        );
        declared.store(i + 1, Ordering::SeqCst);
        if (i + 1) % 5000 == 0 {
            println!("declared {} at {:.1?}", i + 1, start.elapsed());
        }
    }
    println!("DONE all {n} declared in {:.1?}", start.elapsed());
    tokens
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("serve") {
        return serve(args[2].parse().unwrap(), &args[3]).await;
    }
    let n: usize = args.get(1).map_or(70_000, |s| s.parse().unwrap());
    let port: u16 = args.get(2).map_or(17542, |s| s.parse().unwrap());
    let endpoint = format!("tcp/127.0.0.1:{port}");

    // Job 1: watch session A, whose output lines arrive on `lines`.
    let (mut child, lines) = spawn_session_a(n, &endpoint);

    // Job 2: session B connects before A declares anything, then follows A until
    // it finishes or hangs. The session is kept until the exit below, since
    // closing or dropping it blocks for about 10 s.
    let start = Instant::now();
    let session = zenoh::open(config("peer", None, Some(&endpoint)))
        .await
        .unwrap();
    println!("[B] connected as peer via {endpoint}");
    let verdict = follow_session_a(&lines, &session, n, start).await;

    println!("{verdict} (after {:.1?})", start.elapsed());
    let _ = child.kill();
    let _ = child.wait();
    // Exit right away instead of dropping the session.
    std::process::exit(0);
}

/// Starts session A in a child process (`serve`), so a hang there cannot block
/// this one. A thread echoes A's output and forwards each line on the returned
/// channel. A exits when its stdin closes, i.e. when this process goes away.
fn spawn_session_a(n: usize, endpoint: &str) -> (Child, mpsc::Receiver<String>) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["serve", &n.to_string(), endpoint])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let (tx, rx) = mpsc::channel();
    let stdout = child.stdout.take().unwrap();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            println!("[A] {line}");
            let _ = tx.send(line);
        }
    });
    (child, rx)
}

/// Reads A's output until A hangs, finishes or exits, or `LIMIT` has passed
/// since `start`, and returns the verdict.
async fn follow_session_a(
    lines: &mpsc::Receiver<String>,
    session: &zenoh::Session,
    n: usize,
    start: Instant,
) -> String {
    loop {
        let line = match lines.recv_timeout(Duration::from_secs(1)) {
            Ok(line) => line,
            Err(mpsc::RecvTimeoutError::Timeout) if start.elapsed() < LIMIT => continue,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return format!("TIMEOUT: A neither finished nor hung within {LIMIT:?}");
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return "REPRODUCED: A exited while declaring (a debug build panics on overflow)"
                    .into();
            }
        };
        if let Some(k) = hung_declaration(&line) {
            return format!(
                "REPRODUCED: A hung at declaration #{k} of {n}; it has not returned for {HANG:?} (65 535 ExprIds)"
            );
        }
        if line.starts_with("DONE") {
            // Every declaration returned: check that B sees the last token, past the id space.
            let key = format!("demo/token/{}", n - 1);
            return match find_token(session, &key).await {
                Some(t) => {
                    format!("OK: A declared all {n} tokens and B sees {key} ({t:.1?} after DONE)")
                }
                None => format!(
                    "FAILED: A declared all {n} tokens but B does not see {key} within 60 s"
                ),
            };
        }
    }
}

/// The number of the declaration in A's watchdog line, once it has not returned
/// for `HANG` ("declaration #65537 has not returned for 30 s" gives "65537").
fn hung_declaration(line: &str) -> Option<&str> {
    if !line.contains(&format!("has not returned for {} s", HANG.as_secs())) {
        return None;
    }
    Some(line.split_whitespace().nth(1)?.trim_start_matches('#'))
}

/// Asks for the liveliness token `key` until B sees it, for up to 60 s, since B
/// may still be taking in the burst. Returns how long that took.
async fn find_token(session: &zenoh::Session, key: &str) -> Option<Duration> {
    let asked = Instant::now();
    while asked.elapsed() < Duration::from_secs(60) {
        let get = session
            .liveliness()
            .get(key)
            .timeout(Duration::from_secs(5))
            .await
            .unwrap();
        while let Ok(reply) = get.recv_async().await {
            if reply.result().is_ok() {
                return Some(asked.elapsed());
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    None
}
