//! A whole side's view, as tokens, for a network that decides for every ship
//! at once.
//!
//! Each of the side's ships is a token (a row of its own features), each
//! enemy on the plot is a token, and one more holds what belongs to no ship.
//! Positions and velocities are relative to the middle of the side. What
//! belongs to a pair -- one of our ships and one enemy: range, contact and
//! lock, the odds each way, how likely the enemy is to find her, last round's
//! fire -- is a table of its own. The network attends across the tokens, so
//! each ship decides knowing the whole side; its target heads point at
//! tokens.
//!
//! The action heads are the per-ship network's, but the target heads index
//! tokens: helm target, fire, lock and the second operator's target point at
//! enemy tokens, the guide at one of the side's own tokens (her own masked
//! out). Orders are then made exactly as for the per-ship network.

use crate::observation::{Contact, Observation};
use callisto::entity::{Entity, Vec3, G};
use callisto::ship::{Goal, Ship};
use callisto::software::SoftwareKind;
use callisto::tactics::{self, Posture, BANDS};
use cgmath::InnerSpace;

use crate::network::action::{self, Head};
use crate::network::encode::{
  direction, log_distance, log_speed, odds, posture_of, rounds_to_reach, share, Memory, Picture, GOALS, MAX_SHIPS,
};
use crate::network::history::History;
use crate::network::orders;

/// Tokens for the side's own ships, and for enemies: twenty a side, which
/// every scenario in the pool fits (Fighter Swarm's twenty fighters are the
/// most). The network's weights do not depend on it, so a network trained at
/// eight a side loads unchanged; the network trims each batch to its largest
/// fleet, so small scenarios do not pay for the empty rows.
pub const OWN: usize = 20;
pub const FOES: usize = 20;
pub const OWN_FEATS: usize = 73;
pub const FOE_FEATS: usize = 26;
pub const SIDE_FEATS: usize = 10;
pub const PAIR_FEATS: usize = 39;

/// The heads, as the per-side network sizes them: the targets point at enemy
/// tokens (with "none" first for fire, lock and the second operator), the
/// guide at any of the side's own tokens.
pub const HEAD_SIZES: [usize; action::HEADS] = {
  let mut sizes = action::HEAD_SIZES;
  sizes[Head::HelmTarget as usize] = FOES;
  sizes[Head::Guide as usize] = OWN;
  sizes[Head::Fire as usize] = FOES + 1;
  sizes[Head::Lock as usize] = FOES + 1;
  sizes[Head::Target2 as usize] = FOES + 1;
  sizes
};
/// One ship's mask, every head end to end.
pub const MASK_LEN: usize = {
  let mut total = 0;
  let mut i = 0;
  while i < action::HEADS {
    total += HEAD_SIZES[i];
    i += 1;
  }
  total
};

/// Where head `head` starts in one ship's mask or scores.
#[must_use]
pub const fn offset(head: Head) -> usize {
  let mut at = 0;
  let mut i = 0;
  while i < head as usize {
    at += HEAD_SIZES[i];
    i += 1;
  }
  at
}

/// One side's view, laid out flat for the network.
pub struct Buffers<'a> {
  /// `OWN × OWN_FEATS`.
  pub own: &'a mut [f32],
  /// `FOES × FOE_FEATS`.
  pub foes: &'a mut [f32],
  /// `SIDE_FEATS`.
  pub side: &'a mut [f32],
  /// `OWN × FOES × PAIR_FEATS`.
  pub pairs: &'a mut [f32],
  /// `OWN × MASK_LEN`.
  pub masks: &'a mut [bool],
}

/// Which ship is which token.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tokens {
  pub own: Vec<String>,
  pub foes: Vec<String>,
}

impl Tokens {
  /// The picture a ship's choices are read against: enemies and friends are
  /// tokens.
  #[must_use]
  pub fn picture(&self) -> Picture {
    Picture {
      enemies: self.foes.clone(),
      friends: self.own.clone(),
    }
  }
}

/// What the side-wide encoding needs besides the view.
pub struct SideContext<'a> {
  pub view: &'a Observation,
  pub memory: &'a Memory,
  pub history: &'a History,
  pub round: u32,
  pub max_rounds: u32,
  pub own_hull_share: f32,
  pub enemy_hull_share: f32,
  pub goal: Goal,
}

struct Foe<'a> {
  name: &'a str,
  position: Vec3,
  velocity: Vec3,
  contact: Option<&'a Contact>,
}

/// The side's own frame: its middle and average velocity, turned about the
/// vertical so that the middle of the enemies it sees lies straight ahead,
/// along +x. Every position, velocity and burn the network is given is in
/// this frame, so a battle looks the same to it whichever way it faces: a
/// side with the enemy to the east sees what a side with the enemy to the
/// west sees. Without it the network learned to play better facing one way.
#[derive(Clone, Copy)]
struct Frame {
  centre: Vec3,
  velocity: Vec3,
  turn: cgmath::Matrix3<f64>,
}

impl Frame {
  /// The side's middle and average velocity, not yet turned.
  fn of(own: &[&Ship]) -> Self {
    use cgmath::SquareMatrix;
    let n = own.len().max(1) as f64;
    let zero = Vec3::new(0.0, 0.0, 0.0);
    Frame {
      centre: own.iter().map(|ship| ship.get_position()).fold(zero, |a, b| a + b) / n,
      velocity: own.iter().map(|ship| ship.get_velocity()).fold(zero, |a, b| a + b) / n,
      turn: cgmath::Matrix3::identity(),
    }
  }

  /// Turned to face the middle of `enemies`, if there are any off the
  /// vertical.
  fn facing(mut self, enemies: &[Vec3]) -> Self {
    if enemies.is_empty() {
      return self;
    }
    let middle = enemies.iter().fold(Vec3::new(0.0, 0.0, 0.0), |a, b| a + b) / enemies.len() as f64;
    let ahead = middle - self.centre;
    if ahead.x.hypot(ahead.y) > 1.0 {
      self.turn = cgmath::Matrix3::from_angle_z(cgmath::Rad(-ahead.y.atan2(ahead.x)));
    }
    self
  }

  /// A direction or difference, in the frame.
  fn turned(&self, v: Vec3) -> Vec3 {
    self.turn * v
  }
}

/// Where and how something moves, relative to the frame: direction (3), log
/// distance, velocity direction (3), log speed.
fn place(out: &mut [f32], frame: Frame, position: Vec3, velocity: Vec3) {
  let offset = frame.turned(position - frame.centre);
  let drift = frame.turned(velocity - frame.velocity);
  out[0..3].copy_from_slice(&direction(offset));
  out[3] = log_distance(offset.magnitude());
  out[4..7].copy_from_slice(&direction(drift));
  out[7] = log_speed(drift.magnitude());
}

/// Encode a side's view. Empty token rows are zeros.
///
/// # Panics
/// Panics if a buffer is the wrong length.
#[must_use]
pub fn encode_side(ctx: &SideContext, buffers: &mut Buffers) -> Tokens {
  assert_eq!(buffers.own.len(), OWN * OWN_FEATS);
  assert_eq!(buffers.foes.len(), FOES * FOE_FEATS);
  assert_eq!(buffers.side.len(), SIDE_FEATS);
  assert_eq!(buffers.pairs.len(), OWN * FOES * PAIR_FEATS);
  assert_eq!(buffers.masks.len(), OWN * MASK_LEN);
  buffers.own.fill(0.0);
  buffers.foes.fill(0.0);
  buffers.side.fill(0.0);
  buffers.pairs.fill(0.0);
  buffers.masks.fill(false);
  let view = ctx.view;

  let mut own: Vec<&Ship> = view.own.iter().collect();
  own.sort_by(|a, b| a.get_name().cmp(b.get_name()));
  own.truncate(OWN);
  let frame = Frame::of(&own);
  let mut foes: Vec<Foe> = view
    .enemies()
    .into_iter()
    .map(|contact| Foe {
      name: &contact.name,
      position: contact.position,
      velocity: contact.velocity,
      contact: Some(contact),
    })
    .chain(view.blips.iter().map(|blip| Foe {
      name: &blip.name,
      position: blip.position,
      velocity: blip.velocity,
      contact: None,
    }))
    .collect();
  foes.sort_by(|a, b| {
    let from = |foe: &Foe| (foe.position - frame.centre).magnitude();
    from(a).total_cmp(&from(b)).then_with(|| a.name.cmp(b.name))
  });
  foes.truncate(FOES);
  let frame = frame.facing(&foes.iter().map(|foe| foe.position).collect::<Vec<_>>());

  for (i, ship) in own.iter().enumerate() {
    own_token(ctx, ship, frame, &mut buffers.own[i * OWN_FEATS..(i + 1) * OWN_FEATS]);
    for (j, foe) in foes.iter().enumerate() {
      let at = (i * FOES + j) * PAIR_FEATS;
      pair(ctx, frame, ship, foe, &mut buffers.pairs[at..at + PAIR_FEATS]);
    }
    ship_mask(
      view,
      ctx.goal,
      ship,
      i,
      &own,
      &foes,
      &mut buffers.masks[i * MASK_LEN..(i + 1) * MASK_LEN],
    );
  }
  for (j, foe) in foes.iter().enumerate() {
    foe_token(ctx, foe, frame, &own, &mut buffers.foes[j * FOE_FEATS..(j + 1) * FOE_FEATS]);
  }
  side_token(ctx, own.len(), foes.len(), buffers.side);

  Tokens {
    own: own.iter().map(|ship| ship.get_name().to_string()).collect(),
    foes: foes.iter().map(|foe| foe.name.to_string()).collect(),
  }
}

fn own_token(ctx: &SideContext, me: &Ship, frame: Frame, row: &mut [f32]) {
  let view = ctx.view;
  let name = me.get_name();
  row[0] = 1.0;
  place(&mut row[1..9], frame, me.get_position(), me.get_velocity());
  row[9] = share(me.current_hull, me.design.hull);
  row[10] = me.get_current_armor().min(20) as f32 / 20.0;
  row[11] = f32::from(me.max_acceleration()) / 10.0;
  row[12] = f32::from(me.design.maneuver) / 10.0;
  row[13] = f32::from(me.get_dodge_thrust()) / 3.0;
  row[14] = f32::from(u8::from(me.get_assist_gunners()));
  row[15 + posture_of(me)] = 1.0;
  row[18] = me.contacts.len().min(MAX_SHIPS) as f32 / MAX_SHIPS as f32;
  row[19] = me.sensor_locks.len().min(4) as f32 / 4.0;
  for (slot, band) in BANDS.iter().enumerate() {
    row[20 + slot] = f32::from(u8::from(!tactics::weapons_that_bear(me, *band).is_empty()));
  }
  for (system, level) in me.crit_level.iter().enumerate() {
    row[25 + system] = f32::from(*level).min(6.0) / 6.0;
  }
  row[36] = me.design.stealth.map_or(0.0, |stealth| (stealth as usize + 1) as f32 / 4.0);
  row[37] = me.design.countermeasures.map_or(0.0, |cm| (cm as usize + 1) as f32 / 2.0);
  let crew = me.get_crew();
  row[38] = f32::from(crew.get_pilot()).min(4.0) / 4.0;
  let mounts = me.weapons().len();
  row[39] = if mounts == 0 {
    0.0
  } else {
    (0..mounts).map(|mount| f32::from(crew.get_gunnery(mount))).sum::<f32>() / mounts as f32 / 4.0
  };
  row[40] = crew
    .sensor_skills()
    .iter()
    .copied()
    .max()
    .map_or(0.0, |skill| f32::from(skill).min(4.0) / 4.0);
  row[41] = crew
    .engineers()
    .iter()
    .map(|e| f32::from(e.maneuver.max(e.power).max(e.jump)))
    .fold(0.0, f32::max)
    .min(4.0)
    / 4.0;
  let full = callisto::ship::Magazine::for_design(&me.design).missiles;
  row[42] = if full == 0 {
    0.0
  } else {
    (me.magazine.missiles as f32 / full as f32).min(1.0)
  };
  let burn = me.plan.0 .0;
  row[43] = (burn.magnitude() / (10.0 * G)).min(1.0) as f32;
  row[44..47].copy_from_slice(&direction(frame.turned(burn)));
  let inbound: Vec<_> = view.missiles.iter().filter(|missile| missile.target == name).collect();
  row[47] = (inbound.len() as f32 / 5.0).min(1.0);
  row[48] = inbound
    .iter()
    .map(|missile| rounds_to_reach(missile.get_position(), missile.get_velocity(), me))
    .fold(f32::INFINITY, f32::min)
    .min(5.0)
    .mul_add(-0.2, 1.0)
    .max(0.0);
  row[49] = (view.missiles.iter().filter(|missile| missile.source == name).count() as f32 / 10.0).min(1.0);
  row[50] = f32::from(u8::from(me.running_level(SoftwareKind::JumpControl).is_some()));
  row[51] = f32::from(u8::from(me.jump_powered()));
  let well = orders::well(me, view);
  row[52] = f32::from(u8::from(well.is_none()));
  row[53] = well.map_or(0.0, |(short, _)| log_distance(short));
  row[54] = f32::from(u8::from(me.bandwidth_spare() > 0));
  row[55] = f32::from(u8::from(me.comms_jammed));
  row[56] = f32::from(u8::from(crew.sensor_operators() >= 2));
  row[57] = f32::from(crew.get_leadership()).min(6.0) / 6.0;
  if let Some(last) = ctx.history.orders(name) {
    if let Some(verb) = last.verb {
      row[58 + verb as usize] = 1.0;
    }
    if let Some(band) = last.band_index() {
      row[63 + band] = 1.0;
    }
    row[67] = f32::from(u8::from(last.withdrawing));
  }
  row[68] = share(ctx.history.lost(name), me.design.hull);
  row[69] = ctx.history.since_hit(name).map_or(1.0, |rounds| rounds.min(10) as f32 / 10.0);
  let watchers = view.watchers.iter().filter(|watcher| watcher.ours == name);
  let (watching, locking) =
    watchers.fold((0, 0), |(seen, locked), watcher| (seen + 1, locked + usize::from(watcher.lock)));
  row[70] = watching.min(MAX_SHIPS) as f32 / MAX_SHIPS as f32;
  row[71] = locking.min(MAX_SHIPS) as f32 / MAX_SHIPS as f32;
  row[72] = (f64::from(me.design.hull.max(1)).log10() / 4.0) as f32;
}

fn foe_token(ctx: &SideContext, foe: &Foe, frame: Frame, own: &[&Ship], row: &mut [f32]) {
  let view = ctx.view;
  row[0] = 1.0;
  row[1] = f32::from(u8::from(foe.contact.is_some()));
  place(&mut row[2..10], frame, foe.position, foe.velocity);
  match foe.contact {
    Some(contact) => {
      row[10] = share(contact.current_hull, contact.design_hull);
      row[12] = (f64::from(contact.design_hull.max(1)).log10() / 4.0) as f32;
      row[13] = f32::from(contact.current_maneuver) / 10.0;
      row[14] = contact.crit_level.iter().map(|level| f32::from(*level)).sum::<f32>().min(20.0) / 20.0;
      row[15] = contact.stealth.map_or(0.0, |stealth| (stealth as usize + 1) as f32 / 4.0);
      row[16] = contact.countermeasures.map_or(0.0, |cm| (cm as usize + 1) as f32 / 2.0);
      row[17] = (contact.burn.magnitude() / (10.0 * G)).min(1.0) as f32;
      row[18..21].copy_from_slice(&direction(frame.turned(contact.burn)));
      row[22] = share(ctx.history.lost(foe.name), contact.design_hull);
      row[23] = contact.held_by.len().min(MAX_SHIPS) as f32 / MAX_SHIPS as f32;
    }
    None => match ctx.memory.get(foe.name) {
      Some(seen) => {
        row[10] = seen.hull_share;
        row[11] = (ctx.round.saturating_sub(seen.round).min(20) as f32) / 20.0;
        row[12] = (f64::from(seen.design_hull.max(1)).log10() / 4.0) as f32;
        row[13] = f32::from(seen.thrust) / 10.0;
      }
      None => row[11] = 1.0,
    },
  }
  row[21] = ctx.history.fire_share(foe.name);
  let watching = view.watchers.iter().filter(|watcher| watcher.by == foe.name).count();
  row[24] = watching.min(MAX_SHIPS) as f32 / MAX_SHIPS as f32;
  let ours: Vec<&str> = own.iter().map(|ship| ship.get_name()).collect();
  let missiles = view
    .missiles
    .iter()
    .filter(|missile| missile.source == foe.name && ours.contains(&missile.target.as_str()))
    .count();
  row[25] = (missiles as f32 / 10.0).min(1.0);
}

fn side_token(ctx: &SideContext, own: usize, foes: usize, row: &mut [f32]) {
  row[0] = ctx.round as f32 / ctx.max_rounds.max(1) as f32;
  let goal = GOALS.iter().position(|goal| *goal == ctx.goal).unwrap_or(0);
  row[1 + goal] = 1.0;
  row[5] = ctx.own_hull_share;
  row[6] = ctx.enemy_hull_share;
  row[7] = own as f32 / MAX_SHIPS as f32;
  row[8] = foes as f32 / MAX_SHIPS as f32;
  row[9] = (orders::missiles_inbound(ctx.view) as f32 / 10.0).min(1.0);
}

fn pair(ctx: &SideContext, frame: Frame, me: &Ship, foe: &Foe, row: &mut [f32]) {
  let view = ctx.view;
  let name = me.get_name();
  row[0] = 1.0;
  let offset = foe.position - me.get_position();
  let drift = foe.velocity - me.get_velocity();
  row[1..4].copy_from_slice(&direction(frame.turned(offset)));
  row[4] = log_distance(offset.magnitude());
  let closing = if offset.magnitude() > 1e-9 {
    -drift.dot(offset) / offset.magnitude()
  } else {
    0.0
  };
  row[5] = log_speed(closing);
  row[6] = tactics::band_at(offset.magnitude()) as usize as f32 / 4.0;
  row[7] = f32::from(u8::from(me.contacts.iter().any(|held| held == foe.name)));
  row[8] = f32::from(u8::from(me.sensor_locks.iter().any(|locked| locked == foe.name)));
  let watcher = view
    .watchers
    .iter()
    .find(|watcher| watcher.ours == name && watcher.by == foe.name);
  row[9] = f32::from(u8::from(watcher.is_some()));
  row[10] = f32::from(u8::from(watcher.is_some_and(|watcher| watcher.lock)));
  if let Some(contact) = foe.contact {
    if let Some(table) = view.engagement(name, &contact.name) {
      odds(&mut row[11..21], &table.we_hit, contact.current_hull);
      odds(&mut row[21..31], &table.they_hit, me.current_hull);
    }
    if let Some(exposure) = view
      .exposures
      .iter()
      .find(|exposure| exposure.ours == name && exposure.contact == contact.name)
    {
      let found = |posture: Posture, firing: bool| {
        exposure
          .odds
          .iter()
          .find(|odds| odds.posture == posture && odds.firing == firing)
          .map_or(0.0, |odds| odds.found as f32)
      };
      row[31] = found(Posture::Silent, false);
      row[32] = found(Posture::Passive, false);
      row[33] = found(Posture::Active, false);
      row[34] = found(Posture::Active, true);
    }
  }
  if let Some(last) = ctx.history.orders(name) {
    row[35] = f32::from(u8::from(last.steered_by.as_deref() == Some(foe.name)));
    row[36] = f32::from(u8::from(last.fired_at.as_deref() == Some(foe.name)));
    row[37] = f32::from(u8::from(last.locked.as_deref() == Some(foe.name)));
  }
  let missiles = view
    .missiles
    .iter()
    .filter(|missile| missile.source == foe.name && missile.target == name)
    .count();
  row[38] = (missiles as f32 / 5.0).min(1.0);
}

/// Which choices of each head are open to ship `me` (token `index`).
fn ship_mask(view: &Observation, goal: Goal, me: &Ship, index: usize, own: &[&Ship], foes: &[Foe], mask: &mut [bool]) {
  let set = |mask: &mut [bool], head: Head, open: &[bool]| {
    let at = offset(head);
    let size = HEAD_SIZES[head as usize];
    mask[at..at + size].copy_from_slice(&open[..size]);
    if !mask[at..at + size].iter().any(|open| *open) {
      mask[at] = true;
    }
  };
  let moves = me.max_acceleration() > 0;
  let any_foe = !foes.is_empty();
  let guides: Vec<bool> = (0..OWN)
    .map(|k| k != index && own.get(k).is_some_and(|ship| ship.max_acceleration() > 0))
    .collect();
  let verbs = action::Verb::ALL.map(|verb| match verb {
    action::Verb::Hold => true,
    action::Verb::Intercept | action::Verb::HoldRange | action::Verb::OpenRange => moves && any_foe,
    action::Verb::Follow => moves && guides.iter().any(|open| *open),
  });
  set(mask, Head::Verb, &verbs);
  let targets: Vec<bool> = (0..FOES).map(|j| j < foes.len()).collect();
  set(mask, Head::HelmTarget, &targets);
  set(mask, Head::Band, &[true; action::HELM_BANDS.len()]);
  set(mask, Head::Guide, &guides);
  let contacts: Vec<bool> = std::iter::once(true)
    .chain((0..FOES).map(|j| foes.get(j).is_some_and(|foe| foe.contact.is_some())))
    .collect();
  set(mask, Head::Fire, &contacts);
  set(mask, Head::Lock, &contacts);
  set(mask, Head::Posture, &[true; 3]);
  let dodges: Vec<bool> = (0..action::DODGES).map(|g| g <= usize::from(me.max_acceleration())).collect();
  set(mask, Head::Dodge, &dodges);
  set(mask, Head::Assist, &[true, true]);
  set(
    mask,
    Head::Jump,
    &[true, orders::goal_allows_withdrawal(goal) && orders::can_withdraw(me)],
  );
  let any_contact = foes.iter().any(|foe| foe.contact.is_some());
  let missiles = orders::missiles_inbound(view) > 0;
  let operators = me.get_crew().sensor_operators();
  let sensor_verbs = orders::SENSOR_ORDERS.map(|order| match order {
    orders::SensorOrder::Lock | orders::SensorOrder::JamComms => any_contact,
    orders::SensorOrder::JamMissiles => missiles,
  });
  set(mask, Head::Sensor1, if operators >= 1 { &sensor_verbs } else { &[false; 3] });
  set(mask, Head::Sensor2, if operators >= 2 { &sensor_verbs } else { &[false; 3] });
  let none_only: Vec<bool> = std::iter::once(true).chain(std::iter::repeat_n(false, FOES)).collect();
  set(mask, Head::Target2, if operators >= 2 { &contacts } else { &none_only });
  set(mask, Head::Captain, &[true; orders::RANKINGS.len()]);
  set(mask, Head::Computer, &[true, true]);
}

/// An absent ship's mask: only the first choice of every head.
pub fn absent_mask(mask: &mut [bool]) {
  mask.fill(false);
  for head in Head::ALL {
    mask[offset(head)] = true;
  }
}
