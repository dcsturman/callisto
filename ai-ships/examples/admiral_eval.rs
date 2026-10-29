//! How well a model turns written orders into the right [`Directive`]: a
//! corpus of cases, each a scenario side given orders, and checks on the
//! scripts the model answers with.
//!
//! ```text
//! cargo run --release --example admiral_eval -- --cases corpus/admiral_orders.json \
//!   --model gemini --model gemini:gemini-3.5-flash-lite --out runs/eval.jsonl
//! ```
//!
//! Each case names a scenario file and a side, puts the side's ships into
//! squadrons (by name prefix), replaces the scenario's orders with its own
//! (fleet, squadron and ship), and calls the model once, as the admiral
//! does. The answer is read as a `Directive` and checked:
//!
//! - `{"who": "Lance", "step": "all", "path": "engage.designs", "contains": "Light Fighter"}`.
//!   `who` is a squadron, `ship:<name>` (her own script, else her
//!   squadron's) or `all` (every group). The script used is the one the
//!   executor would use (`*` for a squadron without its own); no script at
//!   all is one empty step. `step` is `first` (the default), `last`, `all`,
//!   `any` or an index. Tests: `eq`, `in`, `contains`, `gte`, `lte`,
//!   `absent`; `"not": true` inverts. `goal` reads `destroy` when not given.
//! - `{"who": "Lance", "steps_gte": 2}`, `{"who": "Lance", "steps_lte": 1}`.
//! - `{"own_script": "CS Hesperus"}`, `{"no_own_script": "CS Hesperus"}`.
//! - `{"call_me_when": "losses"}`: some trigger sets that field;
//!   `{"no_call_me_when": "round"}`.
//! - `{"say_any": ["cannot", "can't"]}`: `say` mentions one of these.
//! - `{"any_of": [...]}`, `{"all_of": [...]}`.
//!
//! Every case also checks that the names the answer uses -- squadrons,
//! ships, designs, planets -- are ones the situation gave, and that rules
//! name only enemies.
//!
//! The corpus is written by `corpus/make_admiral_orders.py`; edit that and
//! run it rather than the JSON. Results go to `--out` as JSON lines, one per
//! case, model and `--repeat`, and a summary is printed. `--rescore` checks
//! an earlier results file again against the cases as they are now, calling
//! no model; `--dry-run` prints each case's situation.

use std::collections::BTreeMap;
use std::io::Write;
use std::sync::Arc;
use std::time::Instant;

use ai_ships::admiral::{orders_of, schema, situation, system, TOOL_DESCRIPTION, TOOL_NAME};
use ai_ships::fleet::{squadrons, Directive, Place, Rule};
use ai_ships::observation::Observation;
use ai_ships::provider::{price, ModelSpec, Tool, Vertex};
use callisto::entity::Entity;
use callisto::ship::{load_ship_templates_from_dir, replace_ship_templates};
use clap::Parser;
use futures_util::{stream, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Parser, Debug)]
#[command(about = "Score how well models turn written orders into squadron scripts.")]
struct Args {
  /// The corpus of cases.
  #[arg(long, default_value = "corpus/admiral_orders.json")]
  cases: String,
  /// Where the scenario files are.
  #[arg(long, default_value = "../callisto/scenarios")]
  scenarios: String,
  /// Ship designs to load.
  #[arg(long, default_value = "../callisto/ship_templates")]
  designs: String,
  /// A model to score, as the harness names them. Repeat for each.
  #[arg(long = "model", default_value = "gemini")]
  models: Vec<String>,
  /// Only cases whose id contains one of these.
  #[arg(long)]
  only: Vec<String>,
  /// Write each result as a JSON line here.
  #[arg(long)]
  out: Option<String>,
  /// Calls made at once.
  #[arg(long, default_value_t = 4)]
  parallel: usize,
  /// Ask each model this many times a case.
  #[arg(long, default_value_t = 1)]
  repeat: u32,
  /// Stop before spending more than this, in US dollars.
  #[arg(long, default_value_t = 2.0)]
  budget: f64,
  /// Print each case's situation and stop, calling no model.
  #[arg(long)]
  dry_run: bool,
  /// Call no model: check the answers in this earlier results file again,
  /// against the cases as they are now.
  #[arg(long)]
  rescore: Option<String>,
  #[arg(long, env = "GOOGLE_CLOUD_PROJECT", default_value = "callisto-1731280702227")]
  project: String,
  #[arg(long, env = "CALLISTO_VERTEX_REGION", default_value = "global")]
  region: String,
}

#[derive(Deserialize, Debug, Clone)]
struct Case {
  id: String,
  category: String,
  scenario: String,
  side: String,
  /// Squadron name to the ship-name prefixes of its members.
  #[serde(default)]
  squadrons: BTreeMap<String, Vec<String>>,
  #[serde(default)]
  orders: CaseOrders,
  checks: Vec<Value>,
}

#[derive(Deserialize, Debug, Clone, Default)]
struct CaseOrders {
  fleet: Option<String>,
  #[serde(default)]
  squadron: BTreeMap<String, String>,
  #[serde(default)]
  ship: BTreeMap<String, String>,
}

/// The side's view with the case's squadrons and orders, the scenario's
/// own orders cleared.
async fn view_of(case: &Case, dir: &str) -> Result<Observation, String> {
  let path = format!("{dir}/{}", case.scenario);
  let mut view = ai_ships::harness::engaged_view(&path, &case.side).await?;
  for ship in &mut view.own {
    let name = ship.get_name().to_string();
    if let Some((squadron, _)) = case
      .squadrons
      .iter()
      .find(|(_, prefixes)| prefixes.iter().any(|prefix| name.starts_with(prefix.as_str())))
    {
      ship.squadron = Some(squadron.clone());
    } else if !case.squadrons.is_empty() {
      ship.squadron = None;
    }
    ship.fleet_orders.clone_from(&case.orders.fleet);
    ship.squadron_orders = ship
      .squadron
      .as_ref()
      .and_then(|squadron| case.orders.squadron.get(squadron))
      .cloned();
    ship.orders = case.orders.ship.get(&name).cloned();
  }
  for who in case.orders.ship.keys() {
    if !view.own.iter().any(|ship| ship.get_name() == who) {
      return Err(format!("{}: no ship {who}", case.id));
    }
  }
  for who in case.orders.squadron.keys() {
    if !case.squadrons.contains_key(who) {
      return Err(format!("{}: no squadron {who}", case.id));
    }
  }
  Ok(view)
}

fn prompt_of(view: &Observation) -> String {
  let orders = orders_of(&view.own);
  format!(
    "Asked because: new orders.\n\n{}",
    situation(view, &orders, &Directive::default(), &|_: &str| None)
  )
}

/// The steps the executor would give a group, as JSON: its own script, else
/// `*`'s, else one empty step.
fn steps_for(directive: &Directive, view: &Observation, who: &str) -> Vec<Value> {
  let script = if let Some(ship) = who.strip_prefix("ship:") {
    directive
      .squadrons
      .iter()
      .find(|script| script.ship.as_deref() == Some(ship))
      .or_else(|| {
        let squadron = view
          .own
          .iter()
          .find(|s| s.get_name() == ship)
          .map(|s| s.squadron.clone().unwrap_or_else(|| "(no squadron)".to_string()))?;
        named_script(directive, &squadron)
      })
  } else {
    named_script(directive, who)
  };
  let steps: Vec<Value> = script
    .map(|script| script.steps.iter().filter_map(|step| serde_json::to_value(step).ok()).collect())
    .unwrap_or_default();
  if steps.is_empty() {
    vec![serde_json::to_value(ai_ships::fleet::Step::default()).unwrap_or_default()]
  } else {
    steps
  }
}

fn named_script<'a>(directive: &'a Directive, squadron: &str) -> Option<&'a ai_ships::fleet::Script> {
  let scripts = &directive.squadrons;
  scripts
    .iter()
    .find(|script| script.ship.is_none() && script.squadron == squadron)
    .or_else(|| scripts.iter().find(|script| script.ship.is_none() && script.squadron == "*"))
}

/// Every group the executor would form: squadrons, loose ships, and ships
/// with scripts of their own (as `ship:<name>`).
fn groups(directive: &Directive, view: &Observation) -> Vec<String> {
  let detached: Vec<String> = directive.squadrons.iter().filter_map(|s| s.ship.clone()).collect();
  squadrons(&view.own, &detached).into_iter().map(|(name, _)| name).collect()
}

fn lookup<'a>(step: &'a Value, path: &str) -> Option<&'a Value> {
  path
    .split('.')
    .try_fold(step, |value, key| value.get(key))
    .filter(|v| !v.is_null())
}

fn is_absent(value: Option<&Value>) -> bool {
  match value {
    None | Some(Value::Null) => true,
    Some(Value::Object(fields)) => fields.is_empty(),
    Some(Value::Array(items)) => items.is_empty(),
    Some(_) => false,
  }
}

fn same(a: &Value, b: &Value) -> bool {
  match (a, b) {
    (Value::String(a), Value::String(b)) => a.eq_ignore_ascii_case(b),
    (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
    _ => a == b,
  }
}

/// One step against one test.
fn test_step(step: &Value, check: &Value) -> bool {
  let path = check["path"].as_str().unwrap_or("");
  let default_goal = json!("destroy");
  let value = lookup(step, path).or((path == "goal").then_some(&default_goal));
  let result = if let Some(wanted) = check.get("eq") {
    value.is_some_and(|v| same(v, wanted))
  } else if let Some(Value::Array(wanted)) = check.get("in") {
    value.is_some_and(|v| wanted.iter().any(|w| same(v, w)))
  } else if let Some(wanted) = check.get("contains") {
    let wanted = wanted.as_str().unwrap_or("").to_lowercase();
    match value {
      Some(Value::Array(items)) => items
        .iter()
        .any(|item| item.as_str().is_some_and(|s| s.to_lowercase() == wanted)),
      Some(Value::String(text)) => text.to_lowercase().contains(&wanted),
      _ => false,
    }
  } else if let Some(min) = check.get("gte").and_then(Value::as_f64) {
    value.and_then(Value::as_f64).is_some_and(|v| v >= min)
  } else if let Some(max) = check.get("lte").and_then(Value::as_f64) {
    value.and_then(Value::as_f64).is_some_and(|v| v <= max)
  } else if let Some(absent) = check.get("absent").and_then(Value::as_bool) {
    is_absent(value) == absent
  } else {
    false
  };
  result != check["not"].as_bool().unwrap_or(false)
}

/// Whether the answer passes one check.
fn passes(check: &Value, directive: &Directive, view: &Observation) -> bool {
  if let Some(Value::Array(any)) = check.get("any_of") {
    return any.iter().any(|c| passes(c, directive, view));
  }
  if let Some(Value::Array(all)) = check.get("all_of") {
    return all.iter().all(|c| passes(c, directive, view));
  }
  if let Some(ship) = check["own_script"].as_str() {
    return directive.squadrons.iter().any(|s| s.ship.as_deref() == Some(ship));
  }
  if let Some(ship) = check["no_own_script"].as_str() {
    return !directive.squadrons.iter().any(|s| s.ship.as_deref() == Some(ship));
  }
  let trigger_has = |field: &str| {
    directive
      .call_me_when
      .iter()
      .filter_map(|t| serde_json::to_value(t).ok())
      .any(|t| !is_absent(t.get(field)) && t[field] != json!(false))
  };
  if let Some(field) = check["call_me_when"].as_str() {
    return trigger_has(field);
  }
  if let Some(field) = check["no_call_me_when"].as_str() {
    return !trigger_has(field);
  }
  if let Some(Value::Array(words)) = check.get("say_any") {
    let say = directive.say.as_deref().unwrap_or("").to_lowercase();
    return words.iter().filter_map(Value::as_str).any(|w| say.contains(&w.to_lowercase()));
  }
  let who = check["who"].as_str().unwrap_or("all");
  let whos = if who == "all" {
    groups(directive, view)
  } else {
    vec![who.to_string()]
  };
  whos.iter().all(|who| {
    let steps = steps_for(directive, view, who);
    #[allow(clippy::cast_precision_loss)]
    let count = steps.len() as f64;
    if let Some(min) = check["steps_gte"].as_f64() {
      return count >= min;
    }
    if let Some(max) = check["steps_lte"].as_f64() {
      return count <= max;
    }
    match &check["step"] {
      Value::String(which) if which == "all" => steps.iter().all(|s| test_step(s, check)),
      Value::String(which) if which == "any" => steps.iter().any(|s| test_step(s, check)),
      Value::String(which) if which == "last" => steps.last().is_some_and(|s| test_step(s, check)),
      Value::Number(n) => n
        .as_u64()
        .and_then(|n| steps.get(usize::try_from(n).ok()?))
        .is_some_and(|s| test_step(s, check)),
      _ => steps.first().is_some_and(|s| test_step(s, check)),
    }
  })
}

/// Names the answer uses that the situation never gave.
fn unknown_names(directive: &Directive, view: &Observation) -> Vec<String> {
  let squadron_names: Vec<String> = squadrons(&view.own, &[]).into_iter().map(|(name, _)| name).collect();
  let ships: Vec<String> = view
    .own
    .iter()
    .map(|s| s.get_name().to_string())
    .chain(view.contacts.iter().map(|c| c.name.clone()))
    .collect();
  // A rule picks out enemies: only theirs will do.
  let enemies = view.enemies();
  let enemy_ships: Vec<&str> = enemies.iter().map(|c| c.name.as_str()).collect();
  let enemy_designs: Vec<&str> = enemies.iter().map(|c| c.design.as_str()).collect();
  let planets: Vec<String> = view.planets.iter().map(|p| p.get_name().to_string()).collect();
  let mut bad = Vec::new();
  let rule = |rule: &Rule, bad: &mut Vec<String>| {
    bad.extend(
      rule
        .designs
        .iter()
        .filter(|d| !enemy_designs.iter().any(|known| known.eq_ignore_ascii_case(d)))
        .map(|d| format!("enemy design {d}")),
    );
    bad.extend(
      rule
        .ships
        .iter()
        .filter(|s| !enemy_ships.contains(&s.as_str()))
        .map(|s| format!("enemy ship {s}")),
    );
  };
  let place = |place: &Place, bad: &mut Vec<String>| {
    if let Some(p) = &place.planet {
      if !planets.contains(p) {
        bad.push(format!("planet {p}"));
      }
    }
    if let Some(s) = &place.ship {
      if !ships.contains(s) {
        bad.push(format!("ship {s}"));
      }
    }
    if let Some(s) = &place.squadron {
      if !squadron_names.contains(s) {
        bad.push(format!("squadron {s}"));
      }
    }
    if let Some(r) = &place.enemies {
      rule(r, bad);
    }
  };
  for script in &directive.squadrons {
    if let Some(ship) = &script.ship {
      if !view.own.iter().any(|s| s.get_name() == ship) {
        bad.push(format!("own ship {ship}"));
      }
    } else if script.squadron != "*" && !squadron_names.contains(&script.squadron) {
      bad.push(format!("squadron {}", script.squadron));
    }
    for step in &script.steps {
      if let Some(r) = &step.engage {
        rule(r, &mut bad);
      }
      for p in [&step.post, &step.movement.toward, &step.movement.at].into_iter().flatten() {
        place(p, &mut bad);
      }
      if let Some(within) = step.until.as_ref().and_then(|u| u.within.as_ref()) {
        place(&within.of, &mut bad);
      }
    }
  }
  for trigger in &directive.call_me_when {
    if let Some(s) = &trigger.squadron {
      if !squadron_names.contains(s) {
        bad.push(format!("trigger squadron {s}"));
      }
    }
  }
  bad
}

/// A check's label, for the report: its `what`, or "Flag movement.round = 4 (first)".
fn label(check: &Value) -> String {
  if let Some(what) = check["what"].as_str() {
    return what.to_string();
  }
  let Some(path) = check["path"].as_str() else {
    return check.to_string();
  };
  let test = [
    ("eq", "="),
    ("in", "in"),
    ("contains", "has"),
    ("gte", ">="),
    ("lte", "<="),
    ("absent", "absent:"),
  ]
  .into_iter()
  .find_map(|(key, word)| check.get(key).map(|value| format!("{word} {value}")))
  .unwrap_or_default();
  let not = if check["not"] == json!(true) { "not " } else { "" };
  format!(
    "{} {path} {not}{test} ({})",
    check["who"].as_str().unwrap_or("all"),
    check["step"].as_str().unwrap_or("first")
  )
}

async fn run_one(vertex: &Vertex, spec: &ModelSpec, case: &Case, view: &Observation, rep: u32) -> Value {
  let tool = Tool {
    name: TOOL_NAME,
    description: TOOL_DESCRIPTION,
    schema: schema(),
  };
  let started = Instant::now();
  let result = vertex.call(spec, system(), &prompt_of(view), &tool).await;
  let seconds = started.elapsed().as_secs_f64();
  let mut record = json!({
    "id": case.id, "category": case.category, "model": spec.name(), "rep": rep, "seconds": seconds,
  });
  let call = match result {
    Ok(call) => call,
    Err(e) => {
      record["error"] = json!(format!("call: {e}"));
      record["ok"] = json!(false);
      return record;
    }
  };
  record["input_tokens"] = json!(call.input_tokens);
  record["cached_tokens"] = json!(call.cached_tokens);
  record["output_tokens"] = json!(call.output_tokens);
  record["cost_usd"] = json!(call.cost(spec));
  record["answer"] = call.args;
  score(&mut record, case, view);
  record
}

/// Check the answer in `record` against the case, and mark it.
fn score(record: &mut Value, case: &Case, view: &Observation) {
  let directive: Directive = match serde_json::from_value(record["answer"].clone()) {
    Ok(directive) => directive,
    Err(e) => {
      record["error"] = json!(format!("parse: {e}"));
      record["ok"] = json!(false);
      return;
    }
  };
  let unknown = unknown_names(&directive, view);
  let mut checks: Vec<Value> = case
    .checks
    .iter()
    .map(|check| json!({"what": label(check), "pass": passes(check, &directive, view)}))
    .collect();
  checks.push(json!({"what": "names as given", "pass": unknown.is_empty(), "unknown": unknown}));
  record["ok"] = json!(checks.iter().all(|c| c["pass"] == json!(true)));
  record["checks"] = json!(checks);
}

#[derive(Default)]
struct Tally {
  cases: usize,
  passed: usize,
  checks: usize,
  checks_passed: usize,
  errors: usize,
  cost: f64,
  seconds: Vec<f64>,
}

#[allow(clippy::cast_precision_loss)]
fn pct(part: usize, whole: usize) -> f64 {
  100.0 * part as f64 / whole.max(1) as f64
}

fn summarise(records: &[Value]) {
  let mut by_model: BTreeMap<String, Tally> = BTreeMap::new();
  let mut by_category: BTreeMap<(String, String), (usize, usize)> = BTreeMap::new();
  for record in records {
    let model = record["model"].as_str().unwrap_or("").to_string();
    let tally = by_model.entry(model.clone()).or_default();
    let ok = record["ok"] == json!(true);
    tally.cases += 1;
    tally.passed += usize::from(ok);
    tally.errors += usize::from(record.get("error").is_some());
    tally.cost += record["cost_usd"].as_f64().unwrap_or(0.0);
    tally.seconds.push(record["seconds"].as_f64().unwrap_or(0.0));
    if let Some(Value::Array(checks)) = record.get("checks") {
      tally.checks += checks.len();
      tally.checks_passed += checks.iter().filter(|c| c["pass"] == json!(true)).count();
    } else {
      tally.checks += 1;
    }
    let category = by_category
      .entry((record["category"].as_str().unwrap_or("").to_string(), model))
      .or_default();
    category.0 += 1;
    category.1 += usize::from(ok);
  }
  println!(
    "\n| Model | Cases right | Checks right | Errors | Cost/call | Mean s | p90 s |\n|---|---|---|---|---|---|---|"
  );
  for (model, tally) in &mut by_model {
    tally.seconds.sort_by(f64::total_cmp);
    #[allow(clippy::cast_precision_loss)]
    let mean = tally.seconds.iter().sum::<f64>() / tally.seconds.len().max(1) as f64;
    #[allow(
      clippy::cast_possible_truncation,
      clippy::cast_sign_loss,
      clippy::cast_precision_loss
    )]
    let p90 = tally
      .seconds
      .get(((tally.seconds.len() as f64 * 0.9) as usize).min(tally.seconds.len().saturating_sub(1)))
      .copied()
      .unwrap_or(0.0);
    #[allow(clippy::cast_precision_loss)]
    let per_call = tally.cost / tally.cases.max(1) as f64;
    println!(
      "| {model} | {}/{} ({:.0}%) | {}/{} ({:.0}%) | {} | ${per_call:.4} | {mean:.1} | {p90:.1} |",
      tally.passed,
      tally.cases,
      pct(tally.passed, tally.cases),
      tally.checks_passed,
      tally.checks,
      pct(tally.checks_passed, tally.checks),
      tally.errors,
    );
  }
  let models: Vec<&String> = by_model.keys().collect();
  let categories: Vec<String> = {
    let mut c: Vec<String> = by_category.keys().map(|(c, _)| c.clone()).collect();
    c.dedup();
    c
  };
  println!(
    "\n| Category | {} |\n|---|{}",
    models.iter().map(|m| m.as_str()).collect::<Vec<_>>().join(" | "),
    "---|".repeat(models.len())
  );
  for category in &categories {
    let cells: Vec<String> = models
      .iter()
      .map(|model| {
        by_category
          .get(&(category.clone(), (*model).clone()))
          .map_or_else(|| "-".to_string(), |(n, ok)| format!("{ok}/{n}"))
      })
      .collect();
    println!("| {category} | {} |", cells.join(" | "));
  }
  println!("\nFailures:");
  for record in records.iter().filter(|r| r["ok"] != json!(true)) {
    let failed: Vec<String> = record.get("checks").and_then(Value::as_array).map_or_else(
      || vec![record["error"].as_str().unwrap_or("").chars().take(200).collect()],
      |checks| {
        checks
          .iter()
          .filter(|c| c["pass"] != json!(true))
          .map(|c| {
            let unknown = c.get("unknown").map_or_else(String::new, |u| format!(" {u}"));
            format!("{}{unknown}", c["what"].as_str().unwrap_or(""))
          })
          .collect()
      },
    );
    println!(
      "- {} {}: {}",
      record["model"].as_str().unwrap_or(""),
      record["id"].as_str().unwrap_or(""),
      failed.join("; ")
    );
  }
}

fn by_model_and_case(records: &mut [Value]) {
  records.sort_by(|a, b| (a["model"].as_str(), a["id"].as_str()).cmp(&(b["model"].as_str(), b["id"].as_str())));
}

/// Check the answers in an earlier results file again, against the cases
/// as they are now, calling no model.
fn rescore(
  path: &str, cases: &[Case], views: &[Observation], out: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
  let mut records = Vec::new();
  for line in std::fs::read_to_string(path)?.lines().filter(|l| !l.trim().is_empty()) {
    let mut record: Value = serde_json::from_str(line)?;
    let found = cases
      .iter()
      .zip(views)
      .find(|(case, _)| record["id"].as_str() == Some(case.id.as_str()));
    let Some((case, view)) = found else {
      continue;
    };
    if record.get("answer").is_some() {
      if let Some(fields) = record.as_object_mut() {
        fields.remove("error");
      }
      score(&mut record, case, view);
    }
    records.push(record);
  }
  if let Some(out) = out {
    let mut out = std::io::BufWriter::new(std::fs::File::create(out)?);
    for record in &records {
      writeln!(out, "{record}")?;
    }
  }
  by_model_and_case(&mut records);
  summarise(&records);
  Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .with_writer(std::io::stderr)
    .init();
  let args = Args::parse();
  replace_ship_templates(load_ship_templates_from_dir(&args.designs).await?);
  let cases: Vec<Case> = serde_json::from_str(&std::fs::read_to_string(&args.cases)?)?;
  let cases: Vec<Case> = cases
    .into_iter()
    .filter(|case| args.only.is_empty() || args.only.iter().any(|only| case.id.contains(only.as_str())))
    .collect();
  let mut views = Vec::new();
  for case in &cases {
    views.push(view_of(case, &args.scenarios).await?);
  }
  if args.dry_run {
    for (case, view) in cases.iter().zip(&views) {
      println!("=== {} ({})\n{}", case.id, case.category, prompt_of(view));
    }
    return Ok(());
  }
  if let Some(path) = &args.rescore {
    return rescore(path, &cases, &views, args.out.as_deref());
  }
  let specs: Vec<ModelSpec> = args
    .models
    .iter()
    .map(|name| ModelSpec::parse(name).ok_or_else(|| format!("not a model: {name}")))
    .collect::<Result<_, _>>()?;
  // About 3,300 tokens of prompt and schema, a few hundred of situation, and
  // about 800 out (thinking included): more than any model here has averaged.
  let estimate: f64 = specs
    .iter()
    .map(|spec| {
      #[allow(clippy::cast_precision_loss)]
      let n = f64::from(args.repeat) * cases.len() as f64;
      price(spec).map_or(0.0, |(input, _, output)| n * (3_800.0 * input + 800.0 * output) / 1e6)
    })
    .sum();
  eprintln!(
    "{} cases x {} models; estimated at most ${estimate:.2}",
    cases.len(),
    specs.len()
  );
  if estimate > args.budget {
    return Err(format!("estimate ${estimate:.2} is over the budget of ${:.2}", args.budget).into());
  }
  let vertex = Arc::new(Vertex::new(Some(&args.project), &args.region).await?);
  let mut out = args
    .out
    .as_ref()
    .map(|path| std::fs::File::create(path).map(std::io::BufWriter::new))
    .transpose()?;
  let jobs: Vec<(&ModelSpec, &Case, &Observation, u32)> = specs
    .iter()
    .flat_map(|spec| {
      let (cases, views) = (&cases, &views);
      (0..args.repeat).flat_map(move |rep| cases.iter().zip(views).map(move |(case, view)| (spec, case, view, rep)))
    })
    .collect();
  let mut results = stream::iter(jobs)
    .map(|(spec, case, view, rep)| {
      let vertex = vertex.clone();
      async move { run_one(&vertex, spec, case, view, rep).await }
    })
    .buffer_unordered(args.parallel);
  let mut records = Vec::new();
  let mut spent = 0.0;
  while let Some(record) = results.next().await {
    spent += record["cost_usd"].as_f64().unwrap_or(0.0);
    eprintln!(
      "{} {}: {} ({:.1}s){}",
      record["model"].as_str().unwrap_or(""),
      record["id"].as_str().unwrap_or(""),
      if record["ok"] == json!(true) { "ok" } else { "FAIL" },
      record["seconds"].as_f64().unwrap_or(0.0),
      record.get("error").map_or_else(String::new, |e| format!(
        " {}",
        e.as_str().unwrap_or("").chars().take(160).collect::<String>()
      ))
    );
    if let Some(out) = out.as_mut() {
      writeln!(out, "{record}")?;
      out.flush()?;
    }
    records.push(record);
    if spent > args.budget {
      eprintln!("Spent ${spent:.2}, over the budget: stopping.");
      break;
    }
  }
  by_model_and_case(&mut records);
  summarise(&records);
  println!("\nSpent ${spent:.4}.");
  Ok(())
}
