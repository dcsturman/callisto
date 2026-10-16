//! The Python module `callisto_rl._engine`: the batch, with `NumPy` arrays in
//! and out. `callisto_rl` wraps it for Gymnasium and Stable-Baselines3.

use numpy::{PyArray1, PyArray2, PyArrayMethods, PyReadonlyArray2, PyUntypedArrayMethods};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::action::{HEADS, HEAD_SIZES, MASK_LEN};
use crate::batch::{Batch, Config, Finished, Outputs};
use crate::encode::OBS_LEN;
use crate::game::End;

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
  #[pyo3(signature = (scenarios, designs, games, max_rounds = 40, opponent = None, seed = 0, threads = 10, random_goals = 0.0))]
  #[allow(clippy::too_many_arguments)]
  fn new(
    scenarios: Vec<String>, designs: String, games: usize, max_rounds: u32, opponent: Option<String>, seed: u64,
    threads: usize, random_goals: f32,
  ) -> PyResult<Self> {
    let config = Config {
      scenarios,
      designs,
      games,
      max_rounds,
      opponent,
      seed,
      threads,
      random_goals,
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
    let obs = PyArray2::<f32>::zeros(py, [slots, OBS_LEN], false);
    let masks = PyArray2::<bool>::zeros(py, [slots, MASK_LEN], false);
    let rewards = PyArray1::<f32>::zeros(py, [slots], false);
    let dones = PyArray1::<bool>::zeros(py, [slots], false);
    let terminal = PyArray2::<f32>::zeros(py, [slots, OBS_LEN], false);
    let finished = {
      let mut obs_rw = obs.readwrite();
      let mut masks_rw = masks.readwrite();
      let mut rewards_rw = rewards.readwrite();
      let mut dones_rw = dones.readwrite();
      let mut terminal_rw = terminal.readwrite();
      let mut out = Outputs {
        obs: obs_rw.as_slice_mut()?,
        masks: masks_rw.as_slice_mut()?,
        rewards: rewards_rw.as_slice_mut()?,
        dones: dones_rw.as_slice_mut()?,
        terminal_obs: terminal_rw.as_slice_mut()?,
      };
      self.batch.step(actions.as_slice()?, &mut out)
    };
    let finished = finished
      .iter()
      .map(|finished| finished_dict(py, finished))
      .collect::<PyResult<Vec<_>>>()?;
    Ok((obs, masks, rewards, dones, terminal, finished))
  }
}

#[pymodule]
fn _engine(m: &Bound<'_, PyModule>) -> PyResult<()> {
  m.add_class::<Engine>()?;
  m.add("OBS_LEN", OBS_LEN)?;
  m.add("MASK_LEN", MASK_LEN)?;
  m.add("HEAD_SIZES", HEAD_SIZES.to_vec())?;
  Ok(())
}
