//! SPEC §10 / §11.3 item 8: drive the real binary — render receipt
//! shape, play fallback, demo, error exits, and `loop` surviving a
//! score error and continuing to watch.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_bc");

fn repo(path: &str) -> String {
    format!("{}/{path}", env!("CARGO_MANIFEST_DIR"))
}

/// Kills the spawned process even when an assertion panics mid-test,
/// so a failure can't leak an orphaned `bc loop` holding pipes open.
struct KillOnDrop(std::process::Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("bc-cli-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// `render <score> [out]` prints the two-space-separated receipt and
/// writes the WAV (§11.3 item 8: `bc render examples/dilla.bc` renders).
#[test]
fn render_receipt_and_file() {
    let dir = scratch("render");
    let out = dir.join("dilla.wav");
    let res = Command::new(BIN)
        .args(["render", &repo("examples/dilla.bc")])
        .arg(&out)
        .output()
        .expect("run bc render");
    assert!(res.status.success(), "stderr: {:?}", res.stderr);
    let stdout = String::from_utf8(res.stdout).expect("utf8");
    let line = stdout.lines().next().expect("receipt line");
    let parts: Vec<&str> = line.split("  ").collect();
    assert_eq!(parts.len(), 4, "two-space separators: {line}");
    assert_eq!(parts[0], out.to_string_lossy());
    assert_eq!(parts[2], "44 events", "{line}");
    assert!(
        parts[3].starts_with("sha256=") && parts[3].len() == 7 + 64,
        "{line}"
    );
    let wav = std::fs::read(&out).expect("wav written");
    assert_eq!(&wav[0..4], b"RIFF");
    // Seconds = frames/44100, §6.10-rounded to 2 decimals — exact text,
    // not just a trailing 's'.
    let frames = (wav.len() - 44) / 4;
    let want_secs = format!("{}s", bc::decfmt::format_dec(frames as f64 / 44100.0, 2));
    assert_eq!(parts[1], want_secs, "{line}");
}

/// edge.bc's receipt carries the `(peak-normalized)` flag (§9.5/§10).
#[test]
fn render_receipt_peak_normalized_flag() {
    let dir = scratch("edge");
    let out = dir.join("edge.wav");
    let res = Command::new(BIN)
        .args(["render", &repo("examples/edge.bc")])
        .arg(&out)
        .output()
        .expect("run bc render");
    assert!(res.status.success());
    let stdout = String::from_utf8(res.stdout).expect("utf8");
    assert!(
        stdout.trim_end().ends_with("  (peak-normalized)"),
        "flag missing: {stdout}"
    );
}

/// `play` prints the 12-hex short receipt; with no player on PATH the
/// fallback message names the rendered file (§10). Runs in a scratch
/// cwd with PATH emptied so the fallback chain is exercised
/// deterministically (§11.3 item 8: "renders and plays" — the receipt
/// and fallback chain are the observable surface here).
#[test]
fn play_receipt_and_fallback() {
    let dir = scratch("play");
    std::fs::copy(repo("examples/dilla.bc"), dir.join("dilla.bc")).expect("copy score");
    let res = Command::new(BIN)
        .args(["play", "dilla.bc"])
        .current_dir(&dir)
        .env("PATH", "")
        .output()
        .expect("run bc play");
    assert!(res.status.success(), "stderr: {:?}", res.stderr);
    let stdout = String::from_utf8(res.stdout).expect("utf8");
    let mut lines = stdout.lines();
    let receipt = lines.next().expect("receipt");
    let parts: Vec<&str> = receipt.split("  ").collect();
    assert_eq!(parts.len(), 3, "{receipt}");
    assert!(parts[2].starts_with("sha256="), "{receipt}");
    assert_eq!(
        parts[2].chars().count(),
        7 + 12 + 1,
        "12 hex + ellipsis: {receipt}"
    );
    assert!(parts[2].ends_with('…'), "{receipt}");
    assert_eq!(
        lines.next().expect("fallback"),
        format!(
            "no audio player found — rendered file is at renders{}dilla.wav",
            std::path::MAIN_SEPARATOR
        ),
    );
    assert!(dir.join("renders/dilla.wav").exists());
}

/// `demo` renders every examples/*.bc in lexicographic order (§10).
#[test]
fn demo_renders_all_examples() {
    let dir = scratch("demo");
    let ex = dir.join("examples");
    std::fs::create_dir_all(&ex).expect("examples dir");
    for name in ["four", "dilla", "poly", "edge"] {
        std::fs::copy(
            repo(&format!("examples/{name}.bc")),
            ex.join(format!("{name}.bc")),
        )
        .expect("copy");
    }
    let res = Command::new(BIN)
        .arg("demo")
        .current_dir(&dir)
        .output()
        .expect("run bc demo");
    assert!(res.status.success(), "stderr: {:?}", res.stderr);
    let stdout = String::from_utf8(res.stdout).expect("utf8");
    let firsts: Vec<&str> = stdout
        .lines()
        .map(|l| l.split("  ").next().expect("path"))
        .collect();
    let sep = std::path::MAIN_SEPARATOR;
    assert_eq!(
        firsts,
        [
            format!("renders{sep}dilla.wav"),
            format!("renders{sep}edge.wav"),
            format!("renders{sep}four.wav"),
            format!("renders{sep}poly.wav"),
        ],
        "lexicographic order"
    );
}

/// Errors print cleanly and exit non-zero (§5.9/§10).
#[test]
fn clean_errors_nonzero_exit() {
    // Missing file.
    let res = Command::new(BIN)
        .args(["events", "no-such-file.bc"])
        .output()
        .expect("run");
    assert!(!res.status.success());
    // Parse error cites the line.
    let dir = scratch("err");
    let bad = dir.join("bad.bc");
    std::fs::write(&bad, "voice k sample=kick\n  gate x.q.\n").expect("write");
    let res = Command::new(BIN)
        .arg("events")
        .arg(&bad)
        .output()
        .expect("run");
    assert!(!res.status.success());
    let stderr = String::from_utf8(res.stderr).expect("utf8");
    assert!(
        stderr.starts_with("score error, line 2:"),
        "stderr: {stderr}"
    );
    // Usage on unknown command, non-zero.
    let res = Command::new(BIN).arg("bogus").output().expect("run");
    assert!(!res.status.success());
}

/// `loop` prints `!! <msg> (fix and save again)` on a score error and
/// keeps watching; a save that fixes the score re-renders
/// (§10, §11.3 item 8).
#[test]
fn loop_survives_score_error_and_keeps_watching() {
    let dir = scratch("loop");
    let score = dir.join("jam.bc");
    std::fs::write(&score, "voice k sample=kick\n  gate x.q.\n").expect("write bad score");

    let mut child = KillOnDrop(
        Command::new(BIN)
            .args(["loop", "jam.bc"])
            .current_dir(&dir)
            .env("PATH", "") // deterministic: no player, no blocking
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn bc loop"),
    );
    let child = &mut child.0;
    let stdout = child.stdout.take().expect("stdout");
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            match line {
                Ok(l) => {
                    if tx.send(l).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    let next_line = |what: &str| -> String {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(l) => return l,
                Err(_) if Instant::now() < deadline => continue,
                Err(_) => panic!("timed out waiting for {what}"),
            }
        }
    };

    // The initial pass sees the typo: the error line, loop still alive.
    let l = next_line("error line");
    assert!(
        l.starts_with("!! score error, line 2:") && l.ends_with("(fix and save again)"),
        "got: {l}"
    );
    assert!(child.try_wait().expect("try_wait").is_none(), "loop exited");

    // Fix and save: the loop re-renders (receipt) and keeps running.
    std::thread::sleep(Duration::from_millis(400));
    std::fs::write(&score, "voice k sample=kick\n  gate x...\n").expect("write good score");
    let l = next_line("receipt line");
    assert!(l.contains("sha256="), "got: {l}");
    // With PATH empty, play falls back to the not-found message — drain
    // it before asserting silence below.
    let l = next_line("player fallback line");
    assert!(l.starts_with("no audio player found"), "got: {l}");
    assert!(dir.join("renders/jam.wav").exists());
    assert!(child.try_wait().expect("try_wait").is_none(), "loop exited");

    // A vanished file just keeps polling (§10): no output, no exit.
    std::fs::remove_file(&score).expect("remove score");
    std::thread::sleep(Duration::from_millis(700));
    assert!(
        rx.try_recv().is_err(),
        "no output expected while the file is gone"
    );
    assert!(child.try_wait().expect("try_wait").is_none(), "loop exited");

    // Reappearing file (new mtime) triggers a fresh render.
    std::fs::write(&score, "voice k sample=kick\n  gate x.x.\n").expect("recreate score");
    let l = next_line("receipt after reappearing");
    assert!(l.contains("sha256="), "got: {l}");

    // KillOnDrop reaps the loop process (here and on any earlier panic).
}
