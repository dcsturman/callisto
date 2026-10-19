//! A trained policy, run in Rust: the layers exported from `PyTorch`, and the
//! arithmetic to pick each head's choice from them.
//!
//! The file (`.cpol`, written by `scripts/export.py`) is the bytes `CPOL`, a
//! little-endian `u32` header length, a JSON header naming the layers'
//! shapes, then every layer's weights (row-major, outputs by inputs) and
//! biases as little-endian `f32`.

use rand::Rng;
use serde::Deserialize;

use crate::network::action::{Choice, Head, HEADS, HEAD_SIZES, MASK_LEN};
use crate::network::encode::OBS_LEN;

#[derive(Debug, Deserialize)]
struct Header {
  activation: String,
  obs_len: usize,
  head_sizes: Vec<usize>,
  /// Each layer's (inputs, outputs), in order; the last is the action head.
  layers: Vec<(usize, usize)>,
}

struct Layer {
  inputs: usize,
  outputs: usize,
  weights: Vec<f32>,
  bias: Vec<f32>,
}

/// A policy network: hidden layers with tanh, then one output per choice of
/// every head.
pub struct Policy {
  layers: Vec<Layer>,
}

impl Policy {
  /// Read a policy file.
  ///
  /// # Errors
  /// Returns an error if the file cannot be read, is not a policy file, or
  /// does not fit this build's observation and action heads.
  pub fn load(path: &str) -> Result<Self, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    Self::from_bytes(&bytes).map_err(|e| format!("{path}: {e}"))
  }

  /// A policy from a file's bytes.
  ///
  /// # Errors
  /// As [`Policy::load`].
  pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
    let rest = bytes.strip_prefix(b"CPOL").ok_or("not a policy file")?;
    let (length, rest) = rest.split_first_chunk::<4>().ok_or("truncated")?;
    let length = usize::try_from(u32::from_le_bytes(*length)).map_err(|e| e.to_string())?;
    let header: Header =
      serde_json::from_slice(rest.get(..length).ok_or("truncated header")?).map_err(|e| e.to_string())?;
    if header.activation != "tanh" {
      return Err(format!("activation {} is not supported", header.activation));
    }
    if header.obs_len != OBS_LEN || header.head_sizes != HEAD_SIZES {
      return Err(format!(
        "trained for {} inputs and heads {:?}; this build has {OBS_LEN} and {HEAD_SIZES:?}",
        header.obs_len, header.head_sizes
      ));
    }
    let mut floats = rest
      .get(length..)
      .ok_or("truncated header")?
      .as_chunks::<4>()
      .0
      .iter()
      .map(|chunk| f32::from_le_bytes(*chunk));
    let mut layers = Vec::new();
    let mut width = OBS_LEN;
    for (inputs, outputs) in header.layers {
      if inputs != width {
        return Err(format!("a layer takes {inputs} inputs after one giving {width}"));
      }
      let weights: Vec<f32> = floats.by_ref().take(inputs * outputs).collect();
      let bias: Vec<f32> = floats.by_ref().take(outputs).collect();
      if weights.len() != inputs * outputs || bias.len() != outputs {
        return Err("truncated weights".to_string());
      }
      layers.push(Layer {
        inputs,
        outputs,
        weights,
        bias,
      });
      width = outputs;
    }
    if width != MASK_LEN {
      return Err(format!("the last layer gives {width} outputs, not {MASK_LEN}"));
    }
    Ok(Policy { layers })
  }

  /// The network's score for every choice of every head.
  ///
  /// # Panics
  /// Panics if `obs` is not [`OBS_LEN`] long.
  #[must_use]
  pub fn logits(&self, obs: &[f32]) -> Vec<f32> {
    assert_eq!(obs.len(), OBS_LEN);
    let last = self.layers.len() - 1;
    self.layers.iter().enumerate().fold(obs.to_vec(), |input, (index, layer)| {
      (0..layer.outputs)
        .map(|out| {
          let row = &layer.weights[out * layer.inputs..(out + 1) * layer.inputs];
          let sum = layer.bias[out] + row.iter().zip(&input).map(|(w, x)| w * x).sum::<f32>();
          if index == last {
            sum
          } else {
            sum.tanh()
          }
        })
        .collect()
    })
  }

  /// For each head, the open choices in order of the network's probability:
  /// `(choice, probability)`, most likely first.
  ///
  /// # Panics
  /// Panics if `obs` or `mask` is the wrong length.
  #[must_use]
  pub fn probabilities(&self, obs: &[f32], mask: &[bool]) -> Vec<Vec<(usize, f32)>> {
    assert_eq!(mask.len(), MASK_LEN);
    let logits = self.logits(obs);
    Head::ALL
      .iter()
      .map(|head| {
        let at = head.offset();
        let open: Vec<(usize, f32)> = (0..head.size()).filter(|c| mask[at + c]).map(|c| (c, logits[at + c])).collect();
        let top = open.iter().map(|(_, l)| *l).fold(f32::NEG_INFINITY, f32::max);
        let total: f32 = open.iter().map(|(_, l)| (l - top).exp()).sum();
        let mut probabilities: Vec<(usize, f32)> = open.iter().map(|(c, l)| (*c, (l - top).exp() / total)).collect();
        probabilities.sort_by(|a, b| b.1.total_cmp(&a.1));
        probabilities
      })
      .collect()
  }

  /// One choice per head among those `mask` leaves open: the best scored,
  /// or with `rng`, drawn in proportion to the scores as in training.
  ///
  /// # Panics
  /// Panics if `mask` is not [`MASK_LEN`] long.
  #[must_use]
  pub fn choose(&self, obs: &[f32], mask: &[bool], rng: Option<&mut dyn rand::RngCore>) -> Choice {
    assert_eq!(mask.len(), MASK_LEN);
    let logits = self.logits(obs);
    let mut choice: Choice = [0; HEADS];
    let mut rng = rng;
    for (index, head) in Head::ALL.iter().enumerate() {
      let at = head.offset();
      let open: Vec<(usize, f32)> = (0..head.size()).filter(|c| mask[at + c]).map(|c| (c, logits[at + c])).collect();
      let picked = match rng.as_deref_mut() {
        None => open.iter().max_by(|a, b| a.1.total_cmp(&b.1)).map(|(c, _)| *c),
        Some(rng) => {
          let top = open.iter().map(|(_, l)| *l).fold(f32::NEG_INFINITY, f32::max);
          let weights: Vec<f32> = open.iter().map(|(_, l)| (l - top).exp()).collect();
          let mut draw = rng.gen::<f32>() * weights.iter().sum::<f32>();
          open
            .iter()
            .zip(&weights)
            .find(|(_, w)| {
              draw -= **w;
              draw <= 0.0
            })
            .or_else(|| open.last().map(|last| (last, &0.0)))
            .map(|((c, _), _)| *c)
        }
      };
      choice[index] = i64::try_from(picked.unwrap_or(0)).unwrap_or(0);
    }
    choice
  }
}
