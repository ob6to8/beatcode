//! CLI dispatch (SPEC §10).

pub const USAGE: &str = "\
bc — deterministic music compiler and renderer

usage:
  bc events <score.bc>          compiled events as JSONL on stdout
  bc render <score.bc> [out]    render WAV, print sha256 receipt
  bc play   <score.bc>          render + play once
  bc loop   <score.bc>          re-render + play on every save
  bc demo                       render every examples/*.bc
";

pub fn run() {
    eprint!("{USAGE}");
    std::process::exit(1);
}
