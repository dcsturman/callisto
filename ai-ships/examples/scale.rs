//! A scale test: play a scenario as the harness does, and time each part of
//! every round -- each side's decision, applying its orders, resolving the
//! round -- and the size and cost of the world update a client is sent.
//!
//! ```text
//! cargo run --release --example scale -- "../callisto/scenarios/Scale Test - 200 Ships.json" 10 \
//!   network network
//! ```
//!
//! The arguments are the scenario, the rounds to play, and the commander for
//! each side in side order (as the harness names them: `network`, `close`,
//! `fleet:<directive.json>`, ...). The last one given plays any further side.
//!
//! `SCALE_MISSILES=<n>` puts n missiles in the air before the first round,
//! every ship firing in turn at an enemy; `SCALE_SALVO=<k>` fires them k to a
//! salvo (default 1, every missile its own launch), as a small missile bay's
//! 12 would; `SCALE_WRITE=<file.json>` also writes that world out as a
//! scenario, to load in the browser.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use callisto::authentication::MockAuthenticator;
use callisto::payloads::{EffectsMsg, ResponseMsg};
use callisto::player::PlayerManager;
use callisto::server::Server;
use callisto::ship::{load_ship_templates_from_dir, replace_ship_templates, Team, Weapon, WeaponMount, WeaponType};
use callisto::tactics;

use ai_ships::harness::{apply, sides};
use ai_ships::observation::observe;

/// Put `count` missiles in flight before the first round: every ship in
/// turn fires a salvo of `salvo` at an enemy, the targets spread across the
/// enemy fleet.
fn launch_salvos(server: &Server, count: usize, salvo: usize) -> Result<(), String> {
  let mut world = server.get_unlocked_entities().map_err(|e| e.to_string())?;
  let mut ships: Vec<(String, Option<Team>)> = world
    .ships
    .iter()
    .map(|(name, ship)| (name.clone(), ship.read().unwrap().team))
    .collect();
  ships.sort_by(|a, b| a.0.cmp(&b.0));
  let weapon = Weapon::single(WeaponType::Missile, WeaponMount::Turret);
  let timer = Instant::now();
  let salvo = salvo.max(1);
  for i in 0..count.div_ceil(salvo) {
    let (source, team) = &ships[i % ships.len()];
    let enemies: Vec<&String> = ships.iter().filter(|(_, t)| t != team).map(|(name, _)| name).collect();
    let target = enemies[(i / ships.len() + i) % enemies.len()];
    let size = salvo.min(count - i * salvo);
    world.launch_missile(source, target, weapon.clone(), u16::try_from(size).unwrap_or(u16::MAX))?;
  }
  println!(
    "launched {count} missiles in {} salvoes in {:.0} ms",
    world.missiles.len(),
    ms(timer.elapsed())
  );
  // A scenario file with the missiles already in the air, for looking at in
  // the browser.
  if let Ok(path) = std::env::var("SCALE_WRITE") {
    world.metadata.name = format!("{} - {count} Missiles", world.metadata.name);
    std::fs::write(&path, world.to_scenario_file_json().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    println!("wrote {path}");
  }
  Ok(())
}

fn ms(duration: Duration) -> f64 {
  duration.as_secs_f64() * 1000.0
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let args: Vec<String> = std::env::args().skip(1).collect();
  let scenario = args.first().ok_or("give a scenario")?;
  let rounds: u32 = args.get(1).map_or(Ok(5), |n| n.parse())?;
  let names: Vec<&str> = args.iter().skip(2).map(String::as_str).collect();
  replace_ship_templates(load_ship_templates_from_dir("../callisto/ship_templates").await?);

  let started = Instant::now();
  let server = Arc::new(Server::new("scale", scenario).await);
  let player = PlayerManager::new(Some(server.clone()), Box::new(MockAuthenticator::new("")), true).with_seed(1);
  if let Some(count) = std::env::var("SCALE_MISSILES").ok().and_then(|n| n.parse::<usize>().ok()) {
    let salvo = std::env::var("SCALE_SALVO")
      .ok()
      .and_then(|n| n.parse::<usize>().ok())
      .unwrap_or(1);
    launch_salvos(&server, count, salvo)?;
  }
  let start = player.get_entities().map_err(|e| format!("{e:?}"))?;
  println!("loaded {} ships in {:.0} ms", start.ships.len(), ms(started.elapsed()));
  let rosters = sides(&start);
  let mut commanders = BTreeMap::new();
  for (i, side) in rosters.keys().enumerate() {
    let name = names.get(i).or(names.last()).copied().unwrap_or("network");
    let commander = ai_ships::network::commander::named(name, 1).ok_or_else(|| format!("no commander {name}"))?;
    println!("{side}: {name}, {} ships", rosters[side].len());
    commanders.insert(side.clone(), commander);
  }

  println!(
    "round  ships  missiles (salvoes)  update-bytes  serialize-ms  observe-ms  decide-ms (per side)  apply-ms  resolve-ms"
  );
  for _ in 0..rounds {
    let world = player.get_entities().map_err(|e| format!("{e:?}"))?;
    let ship_count = world.ships.len();
    let salvo_count = world.missiles.len();
    let missile_count: usize = world.missiles.values().map(|m| usize::from(m.read().unwrap().count)).sum();
    let round = world.round;

    let timer = Instant::now();
    let update = serde_json::to_vec(&ResponseMsg::EntityResponse(Box::new(world.clone())))?;
    let serialize = timer.elapsed();

    let mut observing = Duration::ZERO;
    let mut deciding = Vec::new();
    let mut applying = Duration::ZERO;
    for (side, commander) in &mut commanders {
      let ships: Vec<String> = rosters[side]
        .iter()
        .filter(|ship| world.ships.contains_key(*ship))
        .cloned()
        .collect();
      if ships.is_empty() {
        continue;
      }
      let timer = Instant::now();
      let view = observe(&world, &ships);
      observing += timer.elapsed();
      let timer = Instant::now();
      let intents = commander.decide(&view).await;
      deciding.push(format!("{side} {:.0}", ms(timer.elapsed())));
      let timer = Instant::now();
      for (ship, intent) in intents.iter().filter(|(ship, _)| ships.contains(ship)) {
        for request in tactics::orders(&world, ship, intent).unwrap_or_default() {
          apply(&player, request);
        }
      }
      applying += timer.elapsed();
    }

    let timer = Instant::now();
    let effects = player.update();
    let resolve = timer.elapsed();
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for effect in &effects {
      let kind = serde_json::to_value(effect)?;
      let kind = kind["kind"].as_str().unwrap_or_default().to_string();
      *kinds.entry(kind).or_default() += 1;
    }
    let results = serde_json::to_vec(&ResponseMsg::Effects(EffectsMsg { round, effects }))?;
    if let Ok(dir) = std::env::var("SCALE_DUMP") {
      std::fs::write(format!("{dir}/results-{round}.json"), &results)?;
      std::fs::write(format!("{dir}/update-{round}.json"), &update)?;
    }
    println!(
      "{round:>5}  {ship_count:>5}  {missile_count:>8} ({salvo_count:>6})  {:>12}  {:>12.1}  {:>10.1}  {:<20}  {:>8.0}  {:>10.0}   results {} bytes: {:?}",
      update.len(),
      ms(serialize),
      ms(observing),
      deciding.join(", "),
      ms(applying),
      ms(resolve),
      results.len(),
      kinds
    );
  }
  Ok(())
}
