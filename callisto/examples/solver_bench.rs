//! Measure the course solver on recorded problems: `cargo run --release
//! --example solver_bench -- <corpus.jsonl>`. Record a corpus with the
//! `corpus` example in `rl-env`.
//!
//! Solves every course and reports the time per course and which rung of the
//! ladder each landed on: rendezvous, pursuit or shadow.

use std::io::BufRead;
use std::time::Instant;

use callisto::solver::{solve_course, CourseProblem, FlightPathResult, PathMode};

fn main() {
  let path = std::env::args().nth(1).expect("a corpus file");
  let problems: Vec<CourseProblem> = std::io::BufReader::new(std::fs::File::open(&path).unwrap())
    .lines()
    .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
    .collect();
  let start = Instant::now();
  let solved: Vec<FlightPathResult> = problems.iter().map(solve_course).collect();
  let elapsed = start.elapsed();
  let count = |mode: PathMode| solved.iter().filter(|result| result.mode == mode).count();
  println!(
    "{} courses, {:.1} µs a course: rendezvous {}, pursuit {}, shadow {}",
    problems.len(),
    elapsed.as_secs_f64() * 1e6 / f64::from(u32::try_from(problems.len()).unwrap_or(u32::MAX)),
    count(PathMode::Intercept),
    count(PathMode::Pursuit),
    count(PathMode::Shadow),
  );
}
