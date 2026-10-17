//! Many games stepped at once, each side decided whole: what the side
//! network trains on.
//!
//! Every game has two entries, its two sides, so the batch keeps its shape;
//! a side a baseline stands in for still has its entry, and its choices are
//! ignored. Buffers are flat, entry after entry, each laid out as
//! [`crate::side`] describes. A game that ends starts again at once.

use std::sync::Arc;

use rayon::prelude::*;

use crate::action::{Choice, HEADS};
use crate::batch::{load, Config, Finished};
use crate::game::{Control, Game};
use crate::side::{self, Buffers, FOES, FOE_FEATS, MASK_LEN, OWN, OWN_FEATS, PAIR_FEATS, SIDE_FEATS};

/// Every entry's view, flat.
pub struct Views<'a> {
  pub own: &'a mut [f32],
  pub foes: &'a mut [f32],
  pub side: &'a mut [f32],
  pub pairs: &'a mut [f32],
  pub masks: &'a mut [bool],
}

/// Per-entry widths of each buffer.
pub const WIDTHS: [usize; 5] = [
  OWN * OWN_FEATS,
  FOES * FOE_FEATS,
  SIDE_FEATS,
  OWN * FOES * PAIR_FEATS,
  OWN * MASK_LEN,
];

struct Running {
  game: Game,
  returns: [f32; 2],
}

pub struct SideBatch {
  games: Vec<Running>,
  pool: rayon::ThreadPool,
  next_seed: u64,
}

/// One game's part of each buffer: its two entries.
struct Part<'a> {
  own: &'a mut [f32],
  foes: &'a mut [f32],
  side: &'a mut [f32],
  pairs: &'a mut [f32],
  masks: &'a mut [bool],
}

fn parts<'a>(views: &'a mut Views, games: usize) -> Vec<Part<'a>> {
  let cut = |buffer: &'a mut [f32], width: usize| buffer.chunks_mut(2 * width).collect::<Vec<_>>();
  let own = cut(views.own, WIDTHS[0]);
  let foes = cut(views.foes, WIDTHS[1]);
  let side = cut(views.side, WIDTHS[2]);
  let pairs = cut(views.pairs, WIDTHS[3]);
  let masks: Vec<&mut [bool]> = views.masks.chunks_mut(2 * WIDTHS[4]).collect();
  own
    .into_iter()
    .zip(foes)
    .zip(side)
    .zip(pairs)
    .zip(masks)
    .take(games)
    .map(|((((own, foes), side), pairs), masks)| Part {
      own,
      foes,
      side,
      pairs,
      masks,
    })
    .collect()
}

fn observe_game(game: &mut Game, part: &mut Part) {
  for side in 0..2 {
    let mut buffers = Buffers {
      own: &mut part.own[side * WIDTHS[0]..(side + 1) * WIDTHS[0]],
      foes: &mut part.foes[side * WIDTHS[1]..(side + 1) * WIDTHS[1]],
      side: &mut part.side[side * WIDTHS[2]..(side + 1) * WIDTHS[2]],
      pairs: &mut part.pairs[side * WIDTHS[3]..(side + 1) * WIDTHS[3]],
      masks: &mut part.masks[side * WIDTHS[4]..(side + 1) * WIDTHS[4]],
    };
    game.observe_side(side, &mut buffers);
    // Rows for token slots no ship fills: only the first choice open.
    let filled = game.view(side).own.len().min(OWN);
    for row in filled..OWN {
      side::absent_mask(&mut buffers.masks[row * MASK_LEN..(row + 1) * MASK_LEN]);
    }
  }
}

impl SideBatch {
  /// Load and start every game, both sides the network's.
  ///
  /// # Errors
  /// Returns an error if a design or scenario cannot be loaded, or the
  /// thread pool cannot be built.
  pub fn new(config: &Config) -> Result<Self, String> {
    if config.scenarios.is_empty() || config.games == 0 {
      return Err("a batch needs at least one scenario and one game".to_string());
    }
    let scenarios = load(config)?;
    let games = (0..config.games)
      .map(|index| Running {
        game: Game::new(
          Arc::clone(&scenarios[index % scenarios.len()]),
          &[Control::Agent, Control::Agent],
          config.seed + index as u64,
          config.max_rounds,
        )
        .with_goals(config.destroy_goals, config.random_goals),
        returns: [0.0; 2],
      })
      .collect();
    let pool = rayon::ThreadPoolBuilder::new()
      .num_threads(config.threads.max(1))
      .build()
      .map_err(|e| e.to_string())?;
    Ok(SideBatch {
      games,
      pool,
      next_seed: config.seed + config.games as u64,
    })
  }

  #[must_use]
  pub fn games(&self) -> usize {
    self.games.len()
  }

  /// Hand one side of one game to a baseline, or back to the network.
  ///
  /// # Errors
  /// Returns an error if there is no such game or side, or no such baseline.
  pub fn stand_in(&mut self, game: usize, side: usize, baseline: Option<&str>) -> Result<(), String> {
    if let Some(name) = baseline {
      ai_ships::commander::named(name, 0).ok_or_else(|| format!("no baseline {name}"))?;
    }
    if side > 1 {
      return Err(format!("no side {side}"));
    }
    let running = self.games.get_mut(game).ok_or_else(|| format!("no game {game}"))?;
    running.game.stand_in(side, baseline.map(str::to_string));
    Ok(())
  }

  /// Every entry's view as the games stand.
  pub fn observe(&mut self, views: &mut Views) {
    let games = &mut self.games;
    let parts = parts(views, games.len());
    self.pool.install(|| {
      games
        .par_iter_mut()
        .zip(parts)
        .for_each(|(running, mut part)| observe_game(&mut running.game, &mut part));
    });
  }

  /// Step every game with the entries' choices (entries × OWN × HEADS),
  /// restart those that end, and write the new views, each entry's reward,
  /// and whether its game ended.
  ///
  /// # Panics
  /// Panics if a buffer is not the batch's size.
  pub fn step(&mut self, choices: &[i64], views: &mut Views, rewards: &mut [f32], dones: &mut [bool]) -> Vec<Finished> {
    let entries = 2 * self.games.len();
    assert_eq!(choices.len(), entries * OWN * HEADS);
    assert_eq!(rewards.len(), entries);
    assert_eq!(dones.len(), entries);
    let first_seed = self.next_seed;
    self.next_seed += self.games.len() as u64;
    let per_game: Vec<Vec<Vec<Choice>>> = choices
      .as_chunks::<HEADS>()
      .0
      .chunks(2 * OWN)
      .map(|game| game.chunks(OWN).map(<[Choice]>::to_vec).collect())
      .collect();
    let games = &mut self.games;
    let parts = parts(views, games.len());
    let work: Vec<_> = per_game
      .into_iter()
      .zip(parts)
      .zip(rewards.chunks_mut(2).zip(dones.chunks_mut(2)))
      .collect();
    self.pool.install(|| {
      games
        .par_iter_mut()
        .zip(work)
        .enumerate()
        .filter_map(|(index, (running, ((choices, mut part), (rewards, dones))))| {
          let step = running.game.step_sides([Some(&choices[0]), Some(&choices[1])]);
          for side in 0..2 {
            running.returns[side] += step.rewards[side];
            rewards[side] = step.rewards[side];
            dones[side] = step.end.is_some();
          }
          let finished = step.end.map(|end| {
            let game = &running.game;
            let finished = Finished {
              game: index,
              scenario: game.scenario().name.clone(),
              seed: game.seed,
              rounds: game.rounds,
              end,
              sides: [game.side_name(0).to_string(), game.side_name(1).to_string()],
              agent: [game.network_side(0), game.network_side(1)],
              stand_ins: game.stand_ins().clone(),
              goals: game.goals(),
              returns: running.returns,
              hull_shares: game.hull_shares(),
            };
            running.game.reset(first_seed + index as u64);
            running.returns = [0.0; 2];
            finished
          });
          observe_game(&mut running.game, &mut part);
          finished
        })
        .collect()
    })
  }
}
