//! CLI dispatch (SPEC §10). Clean errors and non-zero exits
//! everywhere — except `loop` mode, which prints
//! `!! <msg> (fix and save again)` and keeps watching.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::decfmt::format_dec;
use crate::{events, jsonl, render, score};

pub const USAGE: &str = "\
bc — deterministic music compiler and renderer

usage:
  bc events <score.bc>          compiled events as JSONL on stdout
  bc render <score.bc> [out]    render WAV, print sha256 receipt
  bc play   <score.bc>          render + play once
  bc loop   <score.bc>          re-render + play on every save
  bc demo                       render every examples/*.bc
";

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
}

/// All stdout goes through here: a downstream close (`bc events x |
/// head -1`) is a normal end of output, not a panic; other write
/// errors get the clean §5.9 treatment.
fn out(s: &str) {
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    if let Err(e) = stdout.write_all(s.as_bytes()).and_then(|()| stdout.flush()) {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            std::process::exit(0);
        }
        fail(&format!("cannot write to stdout: {e}"));
    }
}

/// Full pipeline: read → parse → compile → render.
fn build(score_path: &str) -> Result<render::Render, String> {
    let src = std::fs::read_to_string(score_path)
        .map_err(|e| format!("cannot read {score_path}: {e}"))?;
    let sc = score::parse(&src).map_err(|e| e.to_string())?;
    let evs = events::compile(&sc).map_err(|e| e.to_string())?;
    render::render(&evs).map_err(|e| e.to_string())
}

/// Default output path: `renders/<basename minus .bc>.wav`.
fn default_out(score_path: &str) -> PathBuf {
    let base = Path::new(score_path).file_name().map_or_else(
        || score_path.to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let stem = base.strip_suffix(".bc").unwrap_or(&base);
    PathBuf::from("renders").join(format!("{stem}.wav"))
}

/// Write the WAV, creating the output directory on demand (SPEC §9.7).
fn write_wav(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// Reported seconds = frames/44100 rounded to 2 decimals by the §6.10
/// rule (SPEC §9.8).
fn seconds(frames: usize) -> String {
    format_dec(frames as f64 / 44100.0, 2)
}

/// Render + write + full receipt (SPEC §10 `render`): two-space
/// separators, `(peak-normalized)` appended when scaling occurred.
fn render_to(score_path: &str, out: &Path) -> Result<render::Render, String> {
    let r = build(score_path)?;
    write_wav(out, &r.wav_bytes)?;
    let flag = if r.peak_normalized {
        "  (peak-normalized)"
    } else {
        ""
    };
    self::out(&format!(
        "{}  {}s  {} events  sha256={}{}\n",
        out.display(),
        seconds(r.frames),
        r.events,
        r.sha256_hex,
        flag
    ));
    Ok(r)
}

/// Playback (SPEC §10): first found of the fallback chain, blocking;
/// none found is not an error — print where the render landed.
fn play_file(path: &Path) {
    const PLAYERS: &[(&str, &[&str])] = &[
        ("afplay", &[]),
        ("paplay", &[]),
        ("aplay", &["-q"]),
        ("mpv", &["--really-quiet"]),
        ("ffplay", &["-nodisp", "-autoexit", "-loglevel", "quiet"]),
    ];
    for (bin, args) in PLAYERS {
        match Command::new(bin).args(*args).arg(path).spawn() {
            Ok(mut child) => {
                let _ = child.wait(); // blocking: wait for the player
                return;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => continue,
        }
    }
    out(&format!(
        "no audio player found — rendered file is at {}\n",
        path.display()
    ));
}

/// `play`: render to the default path, print the short receipt
/// (first 12 hex + ellipsis), then play.
fn cmd_play(score_path: &str) -> Result<(), String> {
    let out = default_out(score_path);
    let r = build(score_path)?;
    write_wav(&out, &r.wav_bytes)?;
    self::out(&format!(
        "{}  {}s  sha256={}…\n",
        out.display(),
        seconds(r.frames),
        &r.sha256_hex[..12]
    ));
    play_file(&out);
    Ok(())
}

/// `loop`: poll the score's mtime every 200 ms; re-render + play on
/// change; errors keep the loop alive (the jam must survive typos);
/// a vanished file just keeps polling.
fn cmd_loop(score_path: &str) -> ! {
    let mut last_mtime = None;
    loop {
        let mtime = std::fs::metadata(score_path)
            .and_then(|m| m.modified())
            .ok();
        if mtime.is_some() && mtime != last_mtime {
            last_mtime = mtime;
            if let Err(msg) = cmd_play(score_path) {
                out(&format!("!! {msg} (fix and save again)\n"));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// `demo`: render every `examples/*.bc` in lexicographic order.
fn cmd_demo() -> Result<(), String> {
    let mut scores: Vec<PathBuf> = std::fs::read_dir("examples")
        .map_err(|e| format!("cannot read examples/: {e}"))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "bc"))
        .collect();
    scores.sort();
    for p in scores {
        let path = p.to_string_lossy().into_owned();
        render_to(&path, &default_out(&path))?;
    }
    Ok(())
}

pub fn run() {
    // args_os + explicit conversion: a non-UTF-8 argument gets a clean
    // error instead of std::env::args()'s panic.
    let args: Vec<String> = match std::env::args_os()
        .skip(1)
        .map(std::ffi::OsString::into_string)
        .collect::<Result<_, _>>()
    {
        Ok(v) => v,
        Err(_) => fail("invalid UTF-8 in command-line arguments"),
    };
    let arg = |i: usize| args.get(i).map(String::as_str);
    let result: Result<(), String> = match (arg(0), arg(1)) {
        (Some("events"), Some(path)) => {
            let src = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => fail(&format!("cannot read {path}: {e}")),
            };
            let evs = score::parse(&src)
                .and_then(|sc| events::compile(&sc))
                .unwrap_or_else(|e| fail(&e.to_string()));
            out(&jsonl::events_jsonl(&evs));
            Ok(())
        }
        (Some("render"), Some(path)) => {
            let out = arg(2).map_or_else(|| default_out(path), PathBuf::from);
            render_to(path, &out).map(|_| ())
        }
        (Some("play"), Some(path)) => cmd_play(path),
        (Some("loop"), Some(path)) => cmd_loop(path),
        (Some("demo"), _) => cmd_demo(), // trailing tokens ignored, like §5.2 directives
        _ => {
            eprint!("{USAGE}");
            std::process::exit(1);
        }
    };
    if let Err(msg) = result {
        fail(&msg);
    }
}
