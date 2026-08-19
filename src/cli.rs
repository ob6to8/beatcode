//! CLI dispatch (SPEC §10). Clean errors, non-zero exits — except
//! `loop`, which prints `!! <msg> (fix and save again)` and keeps
//! watching.

use crate::{events, jsonl, score};

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

fn read_score(path: &str) -> String {
    match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => fail(&format!("cannot read {path}: {e}")),
    }
}

fn compile_score(src: &str) -> Vec<events::Event> {
    let sc = match score::parse(src) {
        Ok(sc) => sc,
        Err(e) => fail(&e.to_string()),
    };
    match events::compile(&sc) {
        Ok(evs) => evs,
        Err(e) => fail(&e.to_string()),
    }
}

pub fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str);
    match (arg(0), arg(1)) {
        (Some("events"), Some(path)) => {
            let evs = compile_score(&read_score(path));
            print!("{}", jsonl::events_jsonl(&evs));
        }
        _ => {
            eprint!("{USAGE}");
            std::process::exit(1);
        }
    }
}
