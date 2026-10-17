//! The per-side network, run in Rust: the weights exported from `PyTorch`
//! (`scripts/export_side.py`) and the arithmetic of `callisto_rl.sidenet`,
//! step for step, so the harness can play it.

use std::collections::HashMap;

use rand::Rng;
use serde::Deserialize;

use crate::action::{Choice, Head, HEADS};
use crate::side::{FOES, FOE_FEATS, HEAD_SIZES, MASK_LEN, OWN, OWN_FEATS, PAIR_FEATS, SIDE_FEATS};

const NEG: f32 = -1e8;
const TOKENS: usize = OWN + FOES + 1;
/// The ordinary heads, in the order the network's plain layer gives them.
const PLAIN: [Head; 10] = [
  Head::Verb,
  Head::Band,
  Head::Posture,
  Head::Dodge,
  Head::Assist,
  Head::Jump,
  Head::Sensor1,
  Head::Sensor2,
  Head::Captain,
  Head::Computer,
];
/// The heads that point at enemies, in the order the target scorer gives them.
const AT_FOES: [Head; 4] = [Head::HelmTarget, Head::Fire, Head::Lock, Head::Target2];
/// The heads with "none" first, in the order the none layer gives them.
const WITH_NONE: [Head; 3] = [Head::Fire, Head::Lock, Head::Target2];

#[derive(Debug, Deserialize)]
struct Config {
  width: usize,
  heads: usize,
  layers: usize,
}

#[derive(Debug, Deserialize)]
struct Layout {
  own: usize,
  foes: usize,
  own_feats: usize,
  foe_feats: usize,
  side_feats: usize,
  pair_feats: usize,
  head_sizes: Vec<usize>,
}

#[derive(Debug, Deserialize)]
struct Header {
  config: Config,
  layout: Layout,
  tensors: Vec<(String, Vec<usize>)>,
}

/// A trained per-side network.
pub struct SideNet {
  width: usize,
  heads: usize,
  layers: usize,
  tensors: HashMap<String, Vec<f32>>,
}

fn offset(head: Head) -> usize {
  HEAD_SIZES[..head as usize].iter().sum()
}

/// The error function, to within about 1e-7 (Numerical Recipes' erfc).
fn erf(x: f32) -> f32 {
  let x = f64::from(x);
  let z = x.abs();
  let t = 1.0 / (1.0 + 0.5 * z);
  let r = t
    * (-z * z - 1.265_512_23
      + t
        * (1.000_023_68
          + t
            * (0.374_091_96
              + t
                * (0.096_784_18
                  + t
                    * (-0.186_288_06
                      + t
                        * (0.278_868_07
                          + t * (-1.135_203_98 + t * (1.488_515_87 + t * (-0.822_152_23 + t * 0.170_872_77)))))))))
      .exp();
  (if x >= 0.0 { 1.0 - r } else { r - 1.0 }) as f32
}

fn gelu(x: f32) -> f32 {
  0.5 * x * (1.0 + erf(x / std::f32::consts::SQRT_2))
}

impl SideNet {
  /// Read an exported network.
  ///
  /// # Errors
  /// Returns an error if the file cannot be read, is not one, or was
  /// trained for a different layout than this build's.
  pub fn load(path: &str) -> Result<Self, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let rest = bytes
      .strip_prefix(b"CSID")
      .ok_or_else(|| format!("{path}: not a per-side network"))?;
    let (length, rest) = rest.split_first_chunk::<4>().ok_or("truncated")?;
    let length = u32::from_le_bytes(*length) as usize;
    let header: Header =
      serde_json::from_slice(rest.get(..length).ok_or("truncated header")?).map_err(|e| e.to_string())?;
    let l = &header.layout;
    if (l.own, l.foes, l.own_feats, l.foe_feats, l.side_feats, l.pair_feats)
      != (OWN, FOES, OWN_FEATS, FOE_FEATS, SIDE_FEATS, PAIR_FEATS)
      || l.head_sizes != HEAD_SIZES
    {
      return Err(format!("{path}: trained for another layout than this build's"));
    }
    let mut floats = rest
      .get(length..)
      .ok_or("truncated")?
      .as_chunks::<4>()
      .0
      .iter()
      .map(|chunk| f32::from_le_bytes(*chunk));
    let mut tensors = HashMap::new();
    for (name, shape) in header.tensors {
      let count: usize = shape.iter().product();
      let values: Vec<f32> = floats.by_ref().take(count).collect();
      if values.len() != count {
        return Err(format!("{path}: truncated at {name}"));
      }
      tensors.insert(name, values);
    }
    Ok(SideNet {
      width: header.config.width,
      heads: header.config.heads,
      layers: header.config.layers,
      tensors,
    })
  }

  fn t(&self, name: &str) -> &[f32] {
    self.tensors.get(name).map_or(&[], Vec::as_slice)
  }

  /// `rows × inputs` through a linear layer (`weight` is outputs × inputs).
  fn linear(&self, name: &str, x: &[f32], inputs: usize) -> Vec<f32> {
    let weight = self.t(&format!("{name}.weight"));
    let bias = self.t(&format!("{name}.bias"));
    let outputs = bias.len();
    x.chunks(inputs)
      .flat_map(|row| {
        (0..outputs).map(move |o| {
          bias[o]
            + weight[o * inputs..(o + 1) * inputs]
              .iter()
              .zip(row)
              .map(|(w, v)| w * v)
              .sum::<f32>()
        })
      })
      .collect()
  }

  /// Linear, GELU, linear: the two-layer networks `name.0` and `name.2`.
  fn mlp(&self, name: &str, x: &[f32], inputs: usize) -> Vec<f32> {
    let hidden: Vec<f32> = self.linear(&format!("{name}.0"), x, inputs).into_iter().map(gelu).collect();
    let width = self.t(&format!("{name}.0.bias")).len();
    self.linear(&format!("{name}.2"), &hidden, width)
  }

  fn norm(&self, name: &str, x: &[f32]) -> Vec<f32> {
    let gamma = self.t(&format!("{name}.weight"));
    let beta = self.t(&format!("{name}.bias"));
    let w = gamma.len();
    x.chunks(w)
      .flat_map(|row| {
        let mean = row.iter().sum::<f32>() / w as f32;
        let var = row.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / w as f32;
        let scale = 1.0 / (var + 1e-5).sqrt();
        row
          .iter()
          .enumerate()
          .map(move |(i, v)| (v - mean) * scale * gamma[i] + beta[i])
      })
      .collect()
  }

  /// Every own ship's scores, `OWN × MASK_LEN`, as the network gives them.
  ///
  /// # Panics
  /// Panics if an input is the wrong length.
  // Short names follow the formulas: w width, h attention groups, d their
  // size, q/k/v query, key and value.
  #[allow(clippy::many_single_char_names, clippy::too_many_lines)]
  #[must_use]
  pub fn logits(&self, own: &[f32], foes: &[f32], side: &[f32], pairs: &[f32]) -> Vec<f32> {
    assert_eq!(own.len(), OWN * OWN_FEATS);
    assert_eq!(foes.len(), FOES * FOE_FEATS);
    assert_eq!(side.len(), SIDE_FEATS);
    assert_eq!(pairs.len(), OWN * FOES * PAIR_FEATS);
    let (w, h) = (self.width, self.heads);
    let d = w / h;
    let keep: Vec<bool> = (0..OWN)
      .map(|i| own[i * OWN_FEATS] > 0.5)
      .chain((0..FOES).map(|j| foes[j * FOE_FEATS] > 0.5))
      .chain(std::iter::once(true))
      .collect();
    let mut x: Vec<f32> = self.mlp("own_in", own, OWN_FEATS);
    x.extend(self.mlp("foe_in", foes, FOE_FEATS));
    x.extend(self.mlp("side_in", side, SIDE_FEATS));
    let pair = self.mlp("pair_in", pairs, PAIR_FEATS);
    let pw = self.t("pair_in.2.bias").len();
    let bias_of = self.linear("pair_bias", &pair, pw); // OWN × FOES × 2h
    let bias = |head: usize, a: usize, b: usize| -> f32 {
      if a < OWN && (OWN..OWN + FOES).contains(&b) {
        bias_of[(a * FOES + (b - OWN)) * 2 * h + head]
      } else if (OWN..OWN + FOES).contains(&a) && b < OWN {
        bias_of[(b * FOES + (a - OWN)) * 2 * h + h + head]
      } else {
        0.0
      }
    };
    for layer in 0..self.layers {
      let p = format!("layers.{layer}");
      let normed = self.norm(&format!("{p}.norm1"), &x);
      let qkv = self.linear(&format!("{p}.qkv"), &normed, w); // TOKENS × 3w
      let mut attended = vec![0.0; TOKENS * w];
      for head in 0..h {
        for a in 0..TOKENS {
          let q = &qkv[a * 3 * w + head * d..a * 3 * w + head * d + d];
          let scores: Vec<f32> = (0..TOKENS)
            .map(|b| {
              if keep[b] {
                let k = &qkv[b * 3 * w + w + head * d..b * 3 * w + w + head * d + d];
                q.iter().zip(k).map(|(x, y)| x * y).sum::<f32>() / (d as f32).sqrt() + bias(head, a, b)
              } else {
                NEG
              }
            })
            .collect();
          let top = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
          let weights: Vec<f32> = scores.iter().map(|s| (s - top).exp()).collect();
          let total: f32 = weights.iter().sum();
          for (b, weight) in weights.iter().enumerate() {
            let v = &qkv[b * 3 * w + 2 * w + head * d..b * 3 * w + 2 * w + head * d + d];
            for e in 0..d {
              attended[a * w + head * d + e] += weight / total * v[e];
            }
          }
        }
      }
      let out = self.linear(&format!("{p}.out"), &attended, w);
      x.iter_mut().zip(out).for_each(|(x, o)| *x += o);
      let normed = self.norm(&format!("{p}.norm2"), &x);
      let ff = self.mlp(&format!("{p}.ff"), &normed, w);
      x.iter_mut().zip(ff).for_each(|(x, f)| *x += f);
    }
    let x = self.norm("norm", &x);
    let mine = &x[..OWN * w];
    let theirs = &x[OWN * w..(OWN + FOES) * w];

    let mut logits = vec![NEG; OWN * MASK_LEN];
    let plain = self.linear("plain", mine, w);
    let plain_width = plain.len() / OWN;
    let none = self.linear("none", mine, w);
    let guide_q = self.linear("guide_q", mine, w);
    let guide_k = self.linear("guide_k", mine, w);
    for i in 0..OWN {
      let row = &mut logits[i * MASK_LEN..(i + 1) * MASK_LEN];
      let mut at = 0;
      for head in PLAIN {
        let size = HEAD_SIZES[head as usize];
        row[offset(head)..offset(head) + size]
          .copy_from_slice(&plain[i * plain_width + at..i * plain_width + at + size]);
        at += size;
      }
      for j in 0..FOES {
        let joined: Vec<f32> = mine[i * w..(i + 1) * w]
          .iter()
          .chain(&theirs[j * w..(j + 1) * w])
          .chain(&pair[(i * FOES + j) * pw..(i * FOES + j + 1) * pw])
          .copied()
          .collect();
        let scored = self.mlp("point", &joined, 2 * w + pw);
        for (k, head) in AT_FOES.iter().enumerate() {
          let first = usize::from(WITH_NONE.contains(head));
          row[offset(*head) + first + j] = scored[k];
        }
      }
      for (k, head) in WITH_NONE.iter().enumerate() {
        row[offset(*head)] = none[i * WITH_NONE.len() + k];
      }
      for k in 0..OWN {
        let q = &guide_q[i * w..(i + 1) * w];
        let key = &guide_k[k * w..(k + 1) * w];
        row[offset(Head::Guide) + k] = q.iter().zip(key).map(|(a, b)| a * b).sum::<f32>() / (w as f32).sqrt();
      }
    }
    logits
  }

  /// For one ship's scores and mask, each head's open choices with their
  /// probabilities, most likely first.
  #[must_use]
  pub fn probabilities(row: &[f32], mask: &[bool]) -> Vec<Vec<(usize, f32)>> {
    Head::ALL
      .iter()
      .map(|head| {
        let at = offset(*head);
        let open: Vec<(usize, f32)> = (0..HEAD_SIZES[*head as usize])
          .filter(|c| mask[at + c])
          .map(|c| (c, row[at + c]))
          .collect();
        let top = open.iter().map(|(_, l)| *l).fold(f32::NEG_INFINITY, f32::max);
        let total: f32 = open.iter().map(|(_, l)| (l - top).exp()).sum();
        let mut p: Vec<(usize, f32)> = open.iter().map(|(c, l)| (*c, (l - top).exp() / total)).collect();
        p.sort_by(|a, b| b.1.total_cmp(&a.1));
        p
      })
      .collect()
  }

  /// One ship's choice: the best open choice of each head, or one drawn in
  /// proportion with `rng`.
  #[must_use]
  pub fn choose(row: &[f32], mask: &[bool], rng: Option<&mut dyn rand::RngCore>) -> Choice {
    let mut choice: Choice = [0; HEADS];
    let mut rng = rng;
    for (index, options) in Self::probabilities(row, mask).into_iter().enumerate() {
      let picked = match rng.as_deref_mut() {
        None => options.first().map(|(c, _)| *c),
        Some(rng) => {
          let mut draw = rng.gen::<f32>();
          options
            .iter()
            .find(|(_, p)| {
              draw -= p;
              draw <= 0.0
            })
            .or_else(|| options.last())
            .map(|(c, _)| *c)
        }
      };
      choice[index] = i64::try_from(picked.unwrap_or(0)).unwrap_or(0);
    }
    choice
  }
}
