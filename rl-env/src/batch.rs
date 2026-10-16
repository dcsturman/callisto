//! Many games stepped at once on a pool of threads: what training calls.
//!
//! The batch's slots are every game's slots end to end, and its buffers are
//! flat: observations are slots × [`OBS_LEN`], masks slots × [`MASK_LEN`],
//! choices slots × [`HEADS`]. A game that ends is started again at once with
//! the next seed, and the observation it ended on is kept apart, so a slot
//! is never idle.

use std::sync::Arc;

use rayon::prelude::*;

use crate::action::{Choice, HEADS, MASK_LEN};
use crate::encode::OBS_LEN;
use crate::game::{Control, End, Game, Scenario};

/// How the batch is made up.
#[derive(Debug, Clone)]
pub struct Config {
  /// Scenario files; game `i` plays scenario `i % len`.
  pub scenarios: Vec<String>,
  /// Ship design directory.
  pub designs: String,
  pub games: usize,
  pub max_rounds: u32,
  /// `None` for self-play: both sides are slots. Otherwise the baseline the
  /// network plays against, with the network taking each side in turn:
  /// within a scenario, alternate games swap sides.
  pub opponent: Option<String>,
  pub seed: u64,
  /// Threads to step games on. Leave the machine some.
  pub threads: usize,
}

/// A game that finished this step.
#[derive(Debug, Clone, PartialEq)]
pub struct Finished {
  pub game: usize,
  pub scenario: String,
  pub seed: u64,
  pub rounds: u32,
  pub end: End,
  /// The side names, in order, and which of them the network played.
  pub sides: [String; 2],
  pub agent: [bool; 2],
  /// The whole game's reward for each side.
  pub returns: [f32; 2],
  pub hull_shares: [f32; 2],
}

struct Running {
  game: Game,
  returns: [f32; 2],
}

/// The batch.
pub struct Batch {
  games: Vec<Running>,
  /// Where each game's slots start.
  offsets: Vec<usize>,
  slots: usize,
  pool: rayon::ThreadPool,
  next_seed: u64,
}

/// Each slot's step: its side's reward, and whether its game ended.
pub struct Outputs<'a> {
  pub obs: &'a mut [f32],
  pub masks: &'a mut [bool],
  pub rewards: &'a mut [f32],
  pub dones: &'a mut [bool],
  /// The observation a finished game ended on, for its slots; untouched for
  /// the others.
  pub terminal_obs: &'a mut [f32],
}

impl Batch {
  /// Load the designs and scenarios and start every game.
  ///
  /// # Errors
  /// Returns an error if a design or scenario cannot be loaded, the opponent
  /// is not a baseline, or the thread pool cannot be built.
  pub fn new(config: &Config) -> Result<Self, String> {
    if config.scenarios.is_empty() || config.games == 0 {
      return Err("a batch needs at least one scenario and one game".to_string());
    }
    if let Some(opponent) = &config.opponent {
      ai_ships::commander::named(opponent, 0).ok_or_else(|| format!("no baseline {opponent}"))?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
      .enable_all()
      .build()
      .map_err(|e| e.to_string())?;
    let scenarios: Vec<Arc<Scenario>> = runtime.block_on(async {
      let designs = callisto::ship::load_ship_templates_from_dir(&config.designs)
        .await
        .map_err(|e| format!("{}: {e}", config.designs))?;
      callisto::ship::replace_ship_templates(designs);
      let mut scenarios = Vec::new();
      for path in &config.scenarios {
        scenarios.push(Arc::new(Scenario::load(path).await?));
      }
      Ok::<_, String>(scenarios)
    })?;

    let games: Vec<Running> = (0..config.games)
      .map(|index| {
        let scenario = Arc::clone(&scenarios[index % scenarios.len()]);
        let controls = match &config.opponent {
          None => [Control::Agent, Control::Agent],
          Some(opponent) => {
            let baseline = Control::Baseline(opponent.clone());
            if (index / scenarios.len()).is_multiple_of(2) {
              [Control::Agent, baseline]
            } else {
              [baseline, Control::Agent]
            }
          }
        };
        Running {
          game: Game::new(scenario, &controls, config.seed + index as u64, config.max_rounds),
          returns: [0.0; 2],
        }
      })
      .collect();
    let offsets: Vec<usize> = games
      .iter()
      .scan(0, |at, running| {
        let start = *at;
        *at += running.game.slots().len();
        Some(start)
      })
      .collect();
    let slots = games.iter().map(|running| running.game.slots().len()).sum();
    let pool = rayon::ThreadPoolBuilder::new()
      .num_threads(config.threads.max(1))
      .build()
      .map_err(|e| e.to_string())?;
    Ok(Batch {
      next_seed: config.seed + config.games as u64,
      games,
      offsets,
      slots,
      pool,
    })
  }

  /// Every slot in the batch.
  #[must_use]
  pub fn slots(&self) -> usize {
    self.slots
  }

  #[must_use]
  pub fn games(&self) -> usize {
    self.games.len()
  }

  /// For each slot: its game, its side (0 or 1), and its ship's name.
  #[must_use]
  pub fn slot_info(&self) -> Vec<(usize, usize, String)> {
    self
      .games
      .iter()
      .enumerate()
      .flat_map(|(index, running)| {
        running
          .game
          .slots()
          .iter()
          .map(move |slot| (index, slot.side, slot.ship.clone()))
      })
      .collect()
  }

  /// Every slot's observation and mask, as the games stand.
  ///
  /// # Panics
  /// Panics if the buffers are not the batch's size.
  pub fn observe(&mut self, obs: &mut [f32], masks: &mut [bool]) {
    assert_eq!(obs.len(), self.slots * OBS_LEN);
    assert_eq!(masks.len(), self.slots * MASK_LEN);
    let chunks = split(&self.games, obs, OBS_LEN)
      .into_iter()
      .zip(split(&self.games, masks, MASK_LEN));
    let games = &mut self.games;
    self.pool.install(|| {
      games
        .par_iter_mut()
        .zip(chunks.collect::<Vec<_>>())
        .for_each(|(running, (obs, masks))| running.game.observe_into(obs, masks));
    });
  }

  /// Step every game with the slots' `choices` (slots × [`HEADS`]), restart
  /// those that end, and write what comes back into `out`.
  ///
  /// # Panics
  /// Panics if a buffer is not the batch's size.
  pub fn step(&mut self, choices: &[i64], out: &mut Outputs) -> Vec<Finished> {
    assert_eq!(choices.len(), self.slots * HEADS);
    assert_eq!(out.rewards.len(), self.slots);
    assert_eq!(out.dones.len(), self.slots);
    assert_eq!(out.terminal_obs.len(), self.slots * OBS_LEN);
    let choices: Vec<Choice> = choices.as_chunks::<HEADS>().0.to_vec();

    // Seeds for the games that will need restarting, handed out in game
    // order whatever order the threads finish in.
    let first_seed = self.next_seed;
    let games = self.games.len() as u64;
    self.next_seed += games;

    let per_game = self
      .offsets
      .iter()
      .zip(&self.games)
      .map(|(at, running)| &choices[*at..*at + running.game.slots().len()]);
    let obs = split(&self.games, out.obs, OBS_LEN);
    let masks = split(&self.games, out.masks, MASK_LEN);
    let rewards = split(&self.games, out.rewards, 1);
    let dones = split(&self.games, out.dones, 1);
    let terminal = split(&self.games, out.terminal_obs, OBS_LEN);
    let work: Vec<_> = per_game
      .zip(obs)
      .zip(masks)
      .zip(rewards)
      .zip(dones)
      .zip(terminal)
      .map(|(((((choices, obs), masks), rewards), dones), terminal)| (choices, obs, masks, rewards, dones, terminal))
      .collect();
    let games = &mut self.games;
    self.pool.install(|| {
      games
        .par_iter_mut()
        .zip(work)
        .enumerate()
        .filter_map(|(index, (running, (choices, obs, masks, rewards, dones, terminal)))| {
          let step = running.game.step(choices);
          for (side, reward) in step.rewards.iter().enumerate() {
            running.returns[side] += reward;
          }
          for (slot, (reward, done)) in running.game.slots().iter().zip(rewards.iter_mut().zip(dones.iter_mut())) {
            *reward = step.rewards[slot.side];
            *done = step.end.is_some();
          }
          let finished = step.end.map(|end| {
            // The observation it ended on, then a fresh game.
            running.game.observe_into(terminal, masks);
            let game = &running.game;
            let finished = Finished {
              game: index,
              scenario: game.scenario().name.clone(),
              seed: game.seed,
              rounds: game.rounds,
              end,
              sides: [game.side_name(0).to_string(), game.side_name(1).to_string()],
              agent: [0, 1].map(|side| game.slots().iter().any(|slot| slot.side == side)),
              returns: running.returns,
              hull_shares: game.hull_shares(),
            };
            running.game.reset(first_seed + index as u64);
            running.returns = [0.0; 2];
            finished
          });
          running.game.observe_into(obs, masks);
          finished
        })
        .collect()
    })
  }
}

/// Cut a flat buffer into each game's part, `width` per slot.
fn split<'a, T>(games: &[Running], mut buffer: &'a mut [T], width: usize) -> Vec<&'a mut [T]> {
  games
    .iter()
    .map(|running| {
      let (part, rest) = std::mem::take(&mut buffer).split_at_mut(running.game.slots().len() * width);
      buffer = rest;
      part
    })
    .collect()
}
