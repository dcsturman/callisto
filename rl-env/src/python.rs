//! The Python module `callisto_rl._engine`: the batch, with `NumPy` arrays in
//! and out. `callisto_rl` wraps it for Gymnasium and Stable-Baselines3.

use numpy::{
  PyArray1, PyArray2, PyArray3, PyArray4, PyArrayMethods, PyReadonlyArray1, PyReadonlyArray2, PyReadonlyArray3,
  PyUntypedArrayMethods,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::action::{HEADS, HEAD_SIZES, MASK_LEN};
use crate::batch::{Batch, Config, Finished, Outputs};
use crate::encode::OBS_LEN;
use crate::game::End;
use crate::side;
use crate::side_batch::{SideBatch, Views, WIDTHS};

type ObsMasks<'py> = (Bound<'py, PyArray2<f32>>, Bound<'py, PyArray2<bool>>);

/// Many callisto games, stepped together.
#[pyclass(unsendable, module = "callisto_rl._engine")]
struct Engine {
  batch: Batch,
}

fn end_name(end: End) -> (&'static str, Option<usize>) {
  match end {
    End::Destroyed { side } => ("destroyed", Some(side)),
    End::Fled { side } => ("fled", Some(side)),
    End::Draw => ("draw", None),
  }
}

fn finished_dict<'py>(py: Python<'py>, finished: &Finished) -> PyResult<Bound<'py, PyDict>> {
  let dict = PyDict::new(py);
  let (end, loser) = end_name(finished.end);
  dict.set_item("game", finished.game)?;
  dict.set_item("scenario", &finished.scenario)?;
  dict.set_item("seed", finished.seed)?;
  dict.set_item("rounds", finished.rounds)?;
  dict.set_item("end", end)?;
  dict.set_item("loser", loser)?;
  dict.set_item("sides", finished.sides.to_vec())?;
  dict.set_item("agent", finished.agent.to_vec())?;
  dict.set_item("stand_ins", finished.stand_ins.to_vec())?;
  let goals: Vec<String> = finished
    .goals
    .iter()
    .map(|goal| {
      serde_json::to_value(goal)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
    })
    .collect();
  dict.set_item("goals", goals)?;
  dict.set_item("returns", finished.returns.to_vec())?;
  dict.set_item("hull_shares", finished.hull_shares.to_vec())?;
  Ok(dict)
}

#[pymethods]
impl Engine {
  #[new]
  #[pyo3(signature = (scenarios, designs, games, max_rounds = 40, opponent = None, seed = 0, threads = 10, random_goals = 0.0, destroy_goals = 0.0, kill_bonus = 0.0, firepower_bonus = 0.0, random_starts = 0.0))]
  #[allow(clippy::too_many_arguments)]
  fn new(
    scenarios: Vec<String>, designs: String, games: usize, max_rounds: u32, opponent: Option<String>, seed: u64,
    threads: usize, random_goals: f32, destroy_goals: f32, kill_bonus: f32, firepower_bonus: f32, random_starts: f64,
  ) -> PyResult<Self> {
    let config = Config {
      scenarios,
      designs,
      games,
      max_rounds,
      opponent,
      seed,
      threads,
      destroy_goals,
      random_goals,
      shaping: crate::game::Shaping {
        kills: kill_bonus,
        firepower: firepower_bonus,
      },
      variation: random_starts,
    };
    Batch::new(&config).map(|batch| Engine { batch }).map_err(PyValueError::new_err)
  }

  /// Every slot (ship the network plays) across all games.
  #[getter]
  fn num_slots(&self) -> usize {
    self.batch.slots()
  }

  #[getter]
  fn num_games(&self) -> usize {
    self.batch.games()
  }

  /// Hand one side of one game to a baseline (`close`, `hold`, `idle`,
  /// `random`), or back to the network with `None`. Kept across resets.
  #[pyo3(signature = (game, side, baseline = None))]
  fn stand_in(&mut self, game: usize, side: usize, baseline: Option<&str>) -> PyResult<()> {
    self.batch.stand_in(game, side, baseline).map_err(PyValueError::new_err)
  }

  /// For each slot: its game, its side (0 or 1) and its ship.
  fn slot_info(&self) -> Vec<(usize, usize, String)> {
    self.batch.slot_info()
  }

  /// Observations (slots × `OBS_LEN`, float32) and masks (slots × `MASK_LEN`,
  /// bool) as the games stand.
  fn observe<'py>(&mut self, py: Python<'py>) -> PyResult<ObsMasks<'py>> {
    let slots = self.batch.slots();
    let obs = PyArray2::<f32>::zeros(py, [slots, OBS_LEN], false);
    let masks = PyArray2::<bool>::zeros(py, [slots, MASK_LEN], false);
    {
      let mut obs_rw = obs.readwrite();
      let mut masks_rw = masks.readwrite();
      self.batch.observe(obs_rw.as_slice_mut()?, masks_rw.as_slice_mut()?);
    }
    Ok((obs, masks))
  }

  /// Step every game with `actions` (slots × heads, int64). Returns
  /// observations, masks, rewards, dones, the observations finished games
  /// ended on (zeros elsewhere), and a dict for each game that finished.
  // PyO3 hands arguments over by value.
  #[allow(clippy::type_complexity, clippy::needless_pass_by_value)]
  fn step<'py>(
    &mut self, py: Python<'py>, actions: PyReadonlyArray2<'py, i64>,
  ) -> PyResult<(
    Bound<'py, PyArray2<f32>>,
    Bound<'py, PyArray2<bool>>,
    Bound<'py, PyArray1<f32>>,
    Bound<'py, PyArray1<bool>>,
    Bound<'py, PyArray2<f32>>,
    Vec<Bound<'py, PyDict>>,
  )> {
    let slots = self.batch.slots();
    if actions.shape() != [slots, HEADS] {
      return Err(PyValueError::new_err(format!(
        "actions are {:?}, not [{slots}, {HEADS}]",
        actions.shape()
      )));
    }
    // The games are stepped with Python's lock released, so a learner can
    // train on another thread meanwhile; the results are copied out after.
    let choices: Vec<i64> = actions.as_slice()?.to_vec();
    let mut obs = vec![0.0_f32; slots * OBS_LEN];
    let mut masks = vec![false; slots * MASK_LEN];
    let mut rewards = vec![0.0_f32; slots];
    let mut dones = vec![false; slots];
    let mut terminal = vec![0.0_f32; slots * OBS_LEN];
    let batch = &mut self.batch;
    let finished = py.detach(|| {
      let mut out = Outputs {
        obs: &mut obs,
        masks: &mut masks,
        rewards: &mut rewards,
        dones: &mut dones,
        terminal_obs: &mut terminal,
      };
      batch.step(&choices, &mut out)
    });
    let obs = PyArray1::from_vec(py, obs).reshape([slots, OBS_LEN])?;
    let masks = PyArray1::from_vec(py, masks).reshape([slots, MASK_LEN])?;
    let rewards = PyArray1::from_vec(py, rewards);
    let dones = PyArray1::from_vec(py, dones);
    let terminal = PyArray1::from_vec(py, terminal).reshape([slots, OBS_LEN])?;
    let finished = finished
      .iter()
      .map(|finished| finished_dict(py, finished))
      .collect::<PyResult<Vec<_>>>()?;
    Ok((obs, masks, rewards, dones, terminal, finished))
  }
}

/// Many callisto games, each side decided whole: two entries per game.
#[pyclass(unsendable, module = "callisto_rl._engine")]
struct SideEngine {
  batch: SideBatch,
}

type SideViews<'py> = (
  Bound<'py, PyArray3<f32>>,
  Bound<'py, PyArray3<f32>>,
  Bound<'py, PyArray2<f32>>,
  Bound<'py, PyArray4<f32>>,
  Bound<'py, PyArray3<bool>>,
);

struct SideOut {
  own: Vec<f32>,
  foes: Vec<f32>,
  side: Vec<f32>,
  pairs: Vec<f32>,
  masks: Vec<bool>,
}

impl SideOut {
  fn new(entries: usize) -> Self {
    SideOut {
      own: vec![0.0; entries * WIDTHS[0]],
      foes: vec![0.0; entries * WIDTHS[1]],
      side: vec![0.0; entries * WIDTHS[2]],
      pairs: vec![0.0; entries * WIDTHS[3]],
      masks: vec![false; entries * WIDTHS[4]],
    }
  }

  fn views(&mut self) -> Views<'_> {
    Views {
      own: &mut self.own,
      foes: &mut self.foes,
      side: &mut self.side,
      pairs: &mut self.pairs,
      masks: &mut self.masks,
    }
  }

  fn arrays(self, py: Python<'_>, entries: usize) -> PyResult<SideViews<'_>> {
    Ok((
      PyArray1::from_vec(py, self.own).reshape([entries, side::OWN, side::OWN_FEATS])?,
      PyArray1::from_vec(py, self.foes).reshape([entries, side::FOES, side::FOE_FEATS])?,
      PyArray1::from_vec(py, self.side).reshape([entries, side::SIDE_FEATS])?,
      PyArray1::from_vec(py, self.pairs).reshape([entries, side::OWN, side::FOES, side::PAIR_FEATS])?,
      PyArray1::from_vec(py, self.masks).reshape([entries, side::OWN, side::MASK_LEN])?,
    ))
  }
}

#[pymethods]
impl SideEngine {
  #[new]
  #[pyo3(signature = (scenarios, designs, games, max_rounds = 40, seed = 0, threads = 10, random_goals = 0.0, destroy_goals = 0.0, kill_bonus = 0.0, firepower_bonus = 0.0, random_starts = 0.0))]
  #[allow(clippy::too_many_arguments)]
  fn new(
    scenarios: Vec<String>, designs: String, games: usize, max_rounds: u32, seed: u64, threads: usize,
    random_goals: f32, destroy_goals: f32, kill_bonus: f32, firepower_bonus: f32, random_starts: f64,
  ) -> PyResult<Self> {
    let config = Config {
      scenarios,
      designs,
      games,
      max_rounds,
      opponent: None,
      seed,
      threads,
      destroy_goals,
      random_goals,
      shaping: crate::game::Shaping {
        kills: kill_bonus,
        firepower: firepower_bonus,
      },
      variation: random_starts,
    };
    SideBatch::new(&config)
      .map(|batch| SideEngine { batch })
      .map_err(PyValueError::new_err)
  }

  #[getter]
  fn num_games(&self) -> usize {
    self.batch.games()
  }

  #[pyo3(signature = (game, side, baseline = None))]
  fn stand_in(&mut self, game: usize, side: usize, baseline: Option<&str>) -> PyResult<()> {
    self.batch.stand_in(game, side, baseline).map_err(PyValueError::new_err)
  }

  /// Every entry's view: own tokens, enemy tokens, the side token, the pair
  /// table, and each own ship's mask.
  fn observe<'py>(&mut self, py: Python<'py>) -> PyResult<SideViews<'py>> {
    let entries = 2 * self.batch.games();
    let mut out = SideOut::new(entries);
    let batch = &mut self.batch;
    py.detach(|| batch.observe(&mut out.views()));
    out.arrays(py, entries)
  }

  /// Step with `actions` (entries × own ships × heads). Returns the new
  /// views, each entry's reward and done flag, and a dict per finished game.
  #[allow(clippy::needless_pass_by_value, clippy::type_complexity)]
  fn step<'py>(
    &mut self, py: Python<'py>, actions: PyReadonlyArray3<'py, i64>,
  ) -> PyResult<(
    SideViews<'py>,
    Bound<'py, PyArray1<f32>>,
    Bound<'py, PyArray1<bool>>,
    Vec<Bound<'py, PyDict>>,
  )> {
    let entries = 2 * self.batch.games();
    if actions.shape() != [entries, side::OWN, HEADS] {
      return Err(PyValueError::new_err(format!(
        "actions are {:?}, not [{entries}, {}, {HEADS}]",
        actions.shape(),
        side::OWN
      )));
    }
    let choices: Vec<i64> = actions.as_slice()?.to_vec();
    let mut out = SideOut::new(entries);
    let mut rewards = vec![0.0_f32; entries];
    let mut dones = vec![false; entries];
    let batch = &mut self.batch;
    let finished = py.detach(|| batch.step(&choices, &mut out.views(), &mut rewards, &mut dones));
    let finished = finished
      .iter()
      .map(|finished| finished_dict(py, finished))
      .collect::<PyResult<Vec<_>>>()?;
    Ok((
      out.arrays(py, entries)?,
      PyArray1::from_vec(py, rewards),
      PyArray1::from_vec(py, dones),
      finished,
    ))
  }
}

/// The per-side network's scores as the Rust port computes them, for
/// checking the port against `PyTorch`: one side's view in, own ships × mask
/// length out.
#[pyfunction]
#[allow(clippy::needless_pass_by_value)]
fn side_net_logits<'py>(
  py: Python<'py>, path: &str, own: PyReadonlyArray2<'py, f32>, foes: PyReadonlyArray2<'py, f32>,
  side: PyReadonlyArray1<'py, f32>, pairs: PyReadonlyArray3<'py, f32>,
) -> PyResult<Bound<'py, PyArray2<f32>>> {
  let net = crate::side_net::SideNet::load(path).map_err(PyValueError::new_err)?;
  let logits = net.logits(own.as_slice()?, foes.as_slice()?, side.as_slice()?, pairs.as_slice()?);
  PyArray1::from_vec(py, logits).reshape([side::OWN, side::MASK_LEN])
}

#[pymodule]
fn _engine(m: &Bound<'_, PyModule>) -> PyResult<()> {
  m.add_function(wrap_pyfunction!(side_net_logits, m)?)?;
  m.add_class::<Engine>()?;
  m.add_class::<SideEngine>()?;
  m.add("SIDE_HEAD_SIZES", side::HEAD_SIZES.to_vec())?;
  m.add("SIDE_MASK_LEN", side::MASK_LEN)?;
  m.add("OWN", side::OWN)?;
  m.add("FOES", side::FOES)?;
  m.add("OWN_FEATS", side::OWN_FEATS)?;
  m.add("FOE_FEATS", side::FOE_FEATS)?;
  m.add("SIDE_FEATS", side::SIDE_FEATS)?;
  m.add("PAIR_FEATS", side::PAIR_FEATS)?;
  m.add("OBS_LEN", OBS_LEN)?;
  m.add("MASK_LEN", MASK_LEN)?;
  m.add("HEAD_SIZES", HEAD_SIZES.to_vec())?;
  Ok(())
}
