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
  direction, hull_size, log_distance, log_speed, odds, posture_of, rounds_to_reach, share, Memory, Picture, GOALS,
  MAX_SHIPS,
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
pub const OWN_FEATS: usize = 93;
pub const FOE_FEATS: usize = 26;
pub const SIDE_FEATS: usize = 17;
pub const PAIR_FEATS: usize = 40;

/// Where the capital-ship and carrier inputs start in an own row, a pair row
/// and the side row: everything before them is what the network read before
/// step 12, so a network trained then loads with zero weight on the rest.
pub const OWN_CAPITAL_AT: usize = 78;
pub const PAIR_CAPITAL_AT: usize = 39;
pub const SIDE_CAPITAL_AT: usize = 16;

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
  /// For a side whose goal is Hold: where its post is now, and how close to
  /// it counts as holding (metres). `None` for any other goal.
  pub post: Option<(Vec3, f64)>,
}

/// Where a Hold side's scenario puts its post, and its radius in metres: the
/// first of its ships to name one. `None` if none does, and the side holds
/// where it started.
#[must_use]
pub fn scenario_post(own: &[Ship], planets: &[callisto::planet::Planet]) -> Option<(Vec3, f64)> {
  let ship = own.iter().find(|ship| ship.post.is_some())?;
  let at = match ship.post.as_ref()? {
    callisto::ship::Post::Body(name) => planets.iter().find(|planet| planet.get_name() == name)?.get_position(),
    callisto::ship::Post::At([x, y, z]) => Vec3::new(*x, *y, *z),
  };
  let radius = ship.post_radius.unwrap_or(callisto::ship::DEFAULT_POST_RADIUS_KM) * 1000.0;
  Some((at, radius))
}

/// For a side with a post, "hold" means hold the post: every ship told to
/// keep her course instead keeps station on the post, re-aimed each round at
/// where it is now, so a side holding a moon follows it. A squadron's guide
/// holding does the same, and her followers with her. Without this the
/// network had no way to fly to its post at all -- every other helm order
/// is relative to an enemy ship.
pub fn hold_at_post(intents: &mut [(String, callisto::tactics::Intent)], post: Option<(Vec3, f64)>) {
  use callisto::tactics::Helm;
  fn station(helm: &mut Helm, at: [f64; 3]) {
    match helm {
      Helm::Hold => *helm = Helm::Station { position: at },
      Helm::Follow { guide_helm, .. } => station(guide_helm, at),
      _ => {}
    }
  }
  let Some((post, _)) = post else {
    return;
  };
  for (_, intent) in intents.iter_mut() {
    station(&mut intent.helm, [post.x, post.y, post.z]);
  }
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
  side_token(ctx, &own, frame, foes.len(), buffers.side);

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
    (me.aboard().missiles as f32 / full as f32).min(1.0)
  };
  let burn = me.plan.0 .0;
  row[43] = (burn.magnitude() / (10.0 * G)).min(1.0) as f32;
  row[44..47].copy_from_slice(&direction(frame.turned(burn)));
  let inbound: Vec<_> = view.missiles.iter().filter(|missile| missile.target == name).collect();
  row[47] = (orders::missiles_in(inbound.iter().copied()) as f32 / 5.0).min(1.0);
  row[48] = inbound
    .iter()
    .map(|missile| rounds_to_reach(missile.get_position(), missile.get_velocity(), me))
    .fold(f32::INFINITY, f32::min)
    .min(5.0)
    .mul_add(-0.2, 1.0)
    .max(0.0);
  row[49] = (orders::missiles_in(view.missiles.iter().filter(|missile| missile.source == name)) as f32 / 10.0).min(1.0);
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
  row[72] = hull_size(me.design.hull);
  // A Hold side's post, from this ship: which way and how far, and whether
  // she is within its radius.
  if let Some((post, radius)) = ctx.post {
    let offset = post - me.get_position();
    row[73..76].copy_from_slice(&direction(frame.turned(offset)));
    row[76] = log_distance(offset.magnitude());
    row[77] = f32::from(u8::from(offset.magnitude() <= radius));
  }
  capital_features(ctx, me, &mut row[OWN_CAPITAL_AT..]);
}

/// A ship's spinal mounts, by index.
fn spinal_mounts(ship: &Ship) -> Vec<usize> {
  ship
    .weapons()
    .iter()
    .enumerate()
    .filter(|(_, weapon)| weapon.mount.spinal_multiple().is_some())
    .map(|(mount, _)| mount)
    .collect()
}

/// The carrier a craft belongs to, if she is on the plot with her side.
fn carrier_of<'a>(view: &'a Observation, me: &Ship) -> Option<&'a Ship> {
  let name = me.carrier.as_deref()?;
  view.own.iter().find(|ship| ship.get_name() == name)
}

/// Missiles and torpedoes aboard a ship, mounts and stores together, as a
/// share of her design's stock; 0 for a ship that carries none.
fn stores_share(ship: &Ship) -> f32 {
  let full = callisto::ship::Magazine::for_design(&ship.design);
  let aboard = ship.aboard();
  share(aboard.missiles + aboard.torpedoes, full.missiles + full.torpedoes)
}

/// Whether `craft` is where the engine lets her land on `carrier` at the
/// round's end: within Short and inside the velocity limit, the gate
/// `tactics` applies before it gives *Land*.
fn on_the_approach(craft: &Ship, carrier: &Ship) -> bool {
  let closing = (craft.get_velocity() - carrier.get_velocity()).magnitude();
  tactics::band_between(craft, carrier) <= callisto::ship::Range::Short
    && closing <= callisto::flight::landing_velocity_limit(craft.current_maneuver)
}

/// The capital-ship and carrier inputs, fifteen numbers from
/// [`OWN_CAPITAL_AT`]: her spinal, her loads, her deck and her carrier.
fn capital_features(ctx: &SideContext, me: &Ship, row: &mut [f32]) {
  let view = ctx.view;
  let name = me.get_name();
  // Her spinal: whether she has one, and the share of her expected damage
  // on the nearest contact, at the band they are at, that it would do.
  let spinals = spinal_mounts(me);
  row[0] = f32::from(u8::from(!spinals.is_empty()));
  if !spinals.is_empty() {
    let nearest = view
      .enemies()
      .into_iter()
      .min_by(|a, b| {
        let from_me = |contact: &Contact| (contact.position - me.get_position()).magnitude();
        from_me(a).total_cmp(&from_me(b))
      })
      .and_then(|contact| {
        let band = tactics::band_at((contact.position - me.get_position()).magnitude());
        view
          .engagement(name, &contact.name)
          .and_then(|table| table.we_hit.iter().find(|odds| odds.band == band))
      });
    if let Some(odds) = nearest.filter(|odds| odds.expected_damage > 0.0) {
      let spinal: f64 = odds
        .by_mount
        .iter()
        .filter(|(mount, _)| spinals.contains(mount))
        .map(|(_, damage)| damage)
        .sum();
      row[1] = (spinal / odds.expected_damage).min(1.0) as f32;
    }
  }
  // Her launchers' loads: missiles and torpedoes in her mounts against what
  // the mounts hold; 1 for a ship with no launcher, which has nothing to
  // reload.
  let (loaded, capacity) = (0..me.weapons().len()).fold((0, 0), |(loaded, capacity), mount| {
    let (have, room) = (me.loaded(mount), me.capacity(mount));
    (
      loaded + have.missiles + have.torpedoes,
      capacity + room.missiles + room.torpedoes,
    )
  });
  row[2] = if capacity == 0 { 1.0 } else { share(loaded, capacity) };
  // Her deck: craft aboard against her berths, what her tubes release a
  // round against what is aboard, and the share of those aboard fit to go.
  let aboard = me.embarked.len();
  let berths = me.berths().len();
  row[3] = if berths == 0 {
    0.0
  } else {
    share(aboard as u32, berths as u32)
  };
  if aboard > 0 {
    let tons = me.embarked.iter().map(|craft| craft.design.displacement).max().unwrap_or(0);
    row[4] = share(me.launch_per_round(tons), aboard as u32);
    row[5] = share(tactics::ready_craft(me).len() as u32, aboard as u32);
  }
  // Her carrier, for a craft: that she is one, that her carrier is on the
  // plot with the side, the band and the closing speed between them against
  // the landing limit (1/2 at the limit), her carrier's hull, whether her
  // loads are spent, and her carrier's stores.
  row[6] = f32::from(u8::from(me.carrier.is_some()));
  if let Some(carrier) = carrier_of(view, me) {
    row[7] = 1.0;
    row[8] = tactics::band_between(me, carrier) as usize as f32 / 4.0;
    let closing = (me.get_velocity() - carrier.get_velocity()).magnitude();
    row[9] = ((closing / callisto::flight::landing_velocity_limit(me.current_maneuver)).min(2.0) / 2.0) as f32;
    row[10] = share(carrier.current_hull, carrier.design.hull);
    row[12] = stores_share(carrier);
    row[14] = f32::from(u8::from(on_the_approach(me, carrier)));
  } else if aboard > 0 {
    row[12] = stores_share(me);
  }
  row[11] = f32::from(u8::from(tactics::loads_spent(me)));
  row[13] = f32::from(u8::from(me.pinned));
}

fn foe_token(ctx: &SideContext, foe: &Foe, frame: Frame, own: &[&Ship], row: &mut [f32]) {
  let view = ctx.view;
  row[0] = 1.0;
  row[1] = f32::from(u8::from(foe.contact.is_some()));
  place(&mut row[2..10], frame, foe.position, foe.velocity);
  match foe.contact {
    Some(contact) => {
      row[10] = share(contact.current_hull, contact.design_hull);
      row[12] = hull_size(contact.design_hull);
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
        row[12] = hull_size(seen.design_hull);
        row[13] = f32::from(seen.thrust) / 10.0;
      }
      None => row[11] = 1.0,
    },
  }
  row[21] = ctx.history.fire_share(foe.name);
  let watching = view.watchers.iter().filter(|watcher| watcher.by == foe.name).count();
  row[24] = watching.min(MAX_SHIPS) as f32 / MAX_SHIPS as f32;
  let ours: Vec<&str> = own.iter().map(|ship| ship.get_name()).collect();
  let missiles = orders::missiles_in(
    view
      .missiles
      .iter()
      .filter(|missile| missile.source == foe.name && ours.contains(&missile.target.as_str())),
  );
  row[25] = (missiles as f32 / 10.0).min(1.0);
}

fn side_token(ctx: &SideContext, ships: &[&Ship], frame: Frame, foes: usize, row: &mut [f32]) {
  let own = ships.len();
  row[0] = ctx.round as f32 / ctx.max_rounds.max(1) as f32;
  let goal = GOALS.iter().position(|goal| *goal == ctx.goal).unwrap_or(0);
  row[1 + goal] = 1.0;
  row[5] = ctx.own_hull_share;
  row[6] = ctx.enemy_hull_share;
  row[7] = own as f32 / MAX_SHIPS as f32;
  row[8] = foes as f32 / MAX_SHIPS as f32;
  row[9] = (orders::missiles_inbound(ctx.view) as f32 / 10.0).min(1.0);
  // A Hold side's post, from the side's middle, and how many of its ships
  // are within its radius: whether it is holding, and which way to go if not.
  if let Some((post, radius)) = ctx.post {
    let offset = post - frame.centre;
    row[10] = 1.0;
    row[11..14].copy_from_slice(&direction(frame.turned(offset)));
    row[14] = log_distance(offset.magnitude());
    let near = ships
      .iter()
      .filter(|ship| (ship.get_position() - post).magnitude() <= radius)
      .count();
    row[15] = near as f32 / own.max(1) as f32;
  }
  // The side's craft still aboard its carriers, as a share of all its hulls
  // -- those aboard and those on the plot together.
  let aboard: usize = ships.iter().map(|ship| ship.embarked.len()).sum();
  row[SIDE_CAPITAL_AT] = aboard as f32 / (aboard + own).max(1) as f32;
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
  let missiles = orders::missiles_in(
    view
      .missiles
      .iter()
      .filter(|missile| missile.source == foe.name && missile.target == name),
  );
  row[38] = (missiles as f32 / 5.0).min(1.0);
  // Whether her spinal can be laid on it: not on a ship under 2,000 tons
  // unless it held still last round (High Guard p. 34).
  if let Some(contact) = foe.contact {
    let can_lay = tactics::spinal_can_lay_on(contact.displacement, contact.held_still);
    row[PAIR_CAPITAL_AT] = f32::from(u8::from(!spinal_mounts(me).is_empty() && can_lay));
  }
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
  // Launch: a carrier with craft aboard that are ready, not pinned by a
  // berth operation and not under Jump Control. Land: a craft whose
  // carrier is on the plot with the side, not pinned; the engine's own gate
  // (within Short, under the velocity limit) decides when the *Land* is
  // given, and until then "land" flies her home.
  let leaving = me.running_level(SoftwareKind::JumpControl).is_some();
  let launch = !me.embarked.is_empty() && !me.pinned && !leaving && !tactics::ready_craft(me).is_empty();
  let land = !me.pinned && carrier_of(view, me).is_some();
  set(mask, Head::Flight, &[true, launch, land]);
}

/// An absent ship's mask: only the first choice of every head.
pub fn absent_mask(mask: &mut [bool]) {
  mask.fill(false);
  for head in Head::ALL {
    mask[offset(head)] = true;
  }
}

/// A launch now flies as one salvo with its missiles counted in it. What the
/// network sees is counted in missiles, as it was when every missile was its
/// own object, so the trained network reads a salvo of three exactly as it
/// read three missiles.
#[cfg(test)]
mod salvo_tests {
  use super::*;
  use crate::observation::observe;
  use callisto::entity::Entities;
  use callisto::ship::{ShipDesignTemplate, Team, Weapon, WeaponMount, WeaponType};
  use cgmath::Zero;
  use std::sync::Arc;

  fn world(salvoes: &[u16]) -> Entities {
    let design = Arc::new(ShipDesignTemplate {
      name: "Hulk".to_string(),
      displacement: 2000,
      hull: 1000,
      ..ShipDesignTemplate::default()
    });
    let mut world = Entities::default();
    world.add_ship("Blue".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    world.add_ship("Red".to_string(), Vec3::new(5.0e7, 0.0, 0.0), Vec3::zero(), &design, None, None);
    world.ships["Blue"].write().unwrap().team = Some(Team::Blue);
    world.ships["Red"].write().unwrap().team = Some(Team::Red);
    world.establish_initial_contacts();
    let rack = Weapon::uniform(WeaponType::Missile, WeaponMount::Turret, 1);
    for count in salvoes {
      world.launch_missile("Red", "Blue", rack.clone(), *count).unwrap();
      world.launch_missile("Blue", "Red", rack.clone(), *count).unwrap();
    }
    world
  }

  fn encoded(world: &Entities) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>) {
    let view = observe(world, &["Blue".to_string()]);
    let memory = Memory::default();
    let history = History::default();
    let ctx = SideContext {
      view: &view,
      memory: &memory,
      history: &history,
      round: 1,
      max_rounds: 20,
      own_hull_share: 1.0,
      enemy_hull_share: 1.0,
      goal: Goal::Destroy,
      post: None,
    };
    let (mut own, mut foes, mut side, mut pairs, mut masks) = (
      vec![0.0; OWN * OWN_FEATS],
      vec![0.0; FOES * FOE_FEATS],
      vec![0.0; SIDE_FEATS],
      vec![0.0; OWN * FOES * PAIR_FEATS],
      vec![false; OWN * MASK_LEN],
    );
    let _ = encode_side(
      &ctx,
      &mut Buffers {
        own: &mut own,
        foes: &mut foes,
        side: &mut side,
        pairs: &mut pairs,
        masks: &mut masks,
      },
    );
    (own, foes, side, pairs)
  }

  #[test]
  fn a_salvo_encodes_as_its_missiles() {
    let salvo = world(&[3]);
    let singles = world(&[1, 1, 1]);
    assert_eq!(salvo.missiles.len(), 2);
    assert_eq!(singles.missiles.len(), 6);
    let (a, b) = (encoded(&salvo), encoded(&singles));
    assert_eq!(a.0, b.0, "own features");
    assert_eq!(a.1, b.1, "foe features");
    assert_eq!(a.2, b.2, "side features");
    assert_eq!(a.3, b.3, "pair features");
    // And the counts are really in there, not two empty encodings.
    assert!(a.0[47] > 0.0 && a.0[49] > 0.0, "missiles inbound and ours in flight");
  }

  #[test]
  fn the_per_ship_count_is_missiles_too() {
    assert_eq!(orders::missiles_inbound(&observe(&world(&[3]), &["Blue".to_string()])), 3);
  }
}

/// Mounts now hold their own loads, refilled from stores. What the network
/// sees is everything aboard, mounts and stores together, as a share of the
/// design's stock -- what it saw when one shared magazine held it all -- so
/// however a ship's missiles are split between its racks and its stores, it
/// encodes the same.
#[cfg(test)]
mod reload_tests {
  use super::*;
  use crate::observation::observe;
  use callisto::entity::Entities;
  use callisto::ship::{Magazine, ShipDesignTemplate, Team, Weapon, WeaponMount, WeaponType};
  use cgmath::Zero;
  use std::sync::Arc;

  /// Blue carries a Kinunir's two triple-rack turrets and 120 missiles, and
  /// `racks` of them sit in each turret.
  fn world(racks: [u32; 2], spent: u32) -> Entities {
    let rack = Weapon::uniform(WeaponType::Missile, WeaponMount::Turret, 3);
    let design = Arc::new(ShipDesignTemplate {
      name: "Cruiser".to_string(),
      displacement: 1200,
      hull: 480,
      weapons: vec![rack.clone(), rack],
      magazine: Magazine {
        missiles: 120,
        torpedoes: 0,
        sand: 0,
        rounds: 0,
      },
      ..ShipDesignTemplate::default()
    });
    let mut world = Entities::default();
    world.add_ship("Blue".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    world.add_ship("Red".to_string(), Vec3::new(5.0e7, 0.0, 0.0), Vec3::zero(), &design, None, None);
    world.ships["Blue"].write().unwrap().team = Some(Team::Blue);
    world.ships["Red"].write().unwrap().team = Some(Team::Red);
    world.establish_initial_contacts();
    {
      let mut blue = world.ships["Blue"].write().unwrap();
      blue.loaded[0].missiles = racks[0];
      blue.loaded[1].missiles = racks[1];
      blue.magazine.missiles = 120 - spent - racks[0] - racks[1];
    }
    world
  }

  fn own_row(world: &Entities) -> Vec<f32> {
    let view = observe(world, &["Blue".to_string()]);
    let memory = Memory::default();
    let history = History::default();
    let ctx = SideContext {
      view: &view,
      memory: &memory,
      history: &history,
      round: 1,
      max_rounds: 20,
      own_hull_share: 1.0,
      enemy_hull_share: 1.0,
      goal: Goal::Destroy,
      post: None,
    };
    let (mut own, mut foes, mut side, mut pairs, mut masks) = (
      vec![0.0; OWN * OWN_FEATS],
      vec![0.0; FOES * FOE_FEATS],
      vec![0.0; SIDE_FEATS],
      vec![0.0; OWN * FOES * PAIR_FEATS],
      vec![false; OWN * MASK_LEN],
    );
    let _ = encode_side(
      &ctx,
      &mut Buffers {
        own: &mut own,
        foes: &mut foes,
        side: &mut side,
        pairs: &mut pairs,
        masks: &mut masks,
      },
    );
    own
  }

  /// The magazine input reads everything aboard, racks and stores
  /// together; only the step-12 loaded share, which is there to say a rack
  /// needs reloading, tells the two apart.
  #[test]
  fn the_split_between_racks_and_stores_shows_only_in_the_loaded_share() {
    let old = |row: Vec<f32>| row[..OWN_CAPITAL_AT].to_vec();
    let loaded = |row: &[f32]| row[OWN_CAPITAL_AT + 2];
    let full = own_row(&world([12, 12], 0));
    assert_eq!(old(full.clone()), old(own_row(&world([0, 0], 0))), "all in stores");
    assert_eq!(old(full.clone()), old(own_row(&world([12, 0], 0))), "one rack full");
    assert_eq!(full[42], 1.0, "a full stock");
    assert_eq!(loaded(&full), 1.0, "both racks full");
    assert_eq!(loaded(&own_row(&world([12, 0], 0))), 0.5, "one rack of two");
    assert_eq!(loaded(&own_row(&world([0, 0], 0))), 0.0, "all in stores");

    // Thirty spent reads as 90 of 120, wherever the 90 are.
    let spent = own_row(&world([3, 12], 30));
    assert_eq!(old(spent.clone()), old(own_row(&world([12, 12], 30))));
    assert_eq!(spent[42], 90.0 / 120.0);
  }
}

/// What the trained network reads must not move for any design we ship,
/// whatever the engine learns to do with new mounts. Every shipped design is
/// put against itself and against the P.F. Sloan, both ways, at a spread of
/// ranges, and the side's whole encoding -- own, foe, side and pair features
/// and the masks -- is fingerprinted against a file last written for the
/// step-12 layout (the capital-ship and carrier inputs and the flight head).
///
/// Run with `UPDATE_SIDE_FINGERPRINTS=1` to rewrite the file, which should
/// only ever happen alongside a retrain.
#[cfg(test)]
mod fingerprint_tests {
  use super::*;
  use crate::observation::observe;
  use callisto::entity::Entities;
  use callisto::ship::{form_batteries, ShipDesignTemplate, Team};
  use cgmath::Zero;
  use std::fmt::Write as _;
  use std::path::Path;
  use std::sync::Arc;

  const FIXTURE: &str = "tests/fixtures/side_encoding_fingerprints.txt";

  /// Close, Short, Medium, Long and Very Long.
  const DISTANCES: [f64; 5] = [5.0e3, 5.0e5, 5.0e6, 2.0e7, 4.0e7];

  fn designs() -> Vec<(String, Arc<ShipDesignTemplate>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../callisto/ship_templates");
    let mut designs: Vec<(String, Arc<ShipDesignTemplate>)> = std::fs::read_dir(&dir)
      .expect("ship_templates")
      .filter_map(|entry| {
        let path = entry.expect("entry").path();
        (path.extension().and_then(|ext| ext.to_str()) == Some("json")).then_some(path)
      })
      .map(|path| {
        let mut design: ShipDesignTemplate =
          serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("design");
        // As the loader does.
        design.weapons = form_batteries(&design.weapons, design.displacement);
        let file = path.file_stem().and_then(|stem| stem.to_str()).expect("name").to_string();
        (file, Arc::new(design))
      })
      .collect();
    designs.sort_by(|a, b| a.0.cmp(&b.0));
    designs
  }

  fn fingerprint(blue: &Arc<ShipDesignTemplate>, red: &Arc<ShipDesignTemplate>, distance: f64) -> u64 {
    let mut world = Entities::default();
    world.add_ship("Blue".to_string(), Vec3::zero(), Vec3::zero(), blue, None, None);
    world.add_ship("Red".to_string(), Vec3::new(distance, 0.0, 0.0), Vec3::zero(), red, None, None);
    world.ships["Blue"].write().unwrap().team = Some(Team::Blue);
    world.ships["Red"].write().unwrap().team = Some(Team::Red);
    world.establish_initial_contacts();
    let view = observe(&world, &["Blue".to_string()]);
    let memory = Memory::default();
    let history = History::default();
    let ctx = SideContext {
      view: &view,
      memory: &memory,
      history: &history,
      round: 1,
      max_rounds: 20,
      own_hull_share: 1.0,
      enemy_hull_share: 1.0,
      goal: Goal::Destroy,
      post: None,
    };
    let (mut own, mut foes, mut side, mut pairs, mut masks) = (
      vec![0.0; OWN * OWN_FEATS],
      vec![0.0; FOES * FOE_FEATS],
      vec![0.0; SIDE_FEATS],
      vec![0.0; OWN * FOES * PAIR_FEATS],
      vec![false; OWN * MASK_LEN],
    );
    let _ = encode_side(
      &ctx,
      &mut Buffers {
        own: &mut own,
        foes: &mut foes,
        side: &mut side,
        pairs: &mut pairs,
        masks: &mut masks,
      },
    );
    // FNV-1a over every feature's bits, then every mask entry.
    let bytes = [&own, &foes, &side, &pairs]
      .into_iter()
      .flat_map(|features| features.iter().flat_map(|value| value.to_bits().to_le_bytes()))
      .chain(masks.iter().map(|open| u8::from(*open)));
    bytes.fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
      (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
  }

  #[test]
  fn shipped_designs_encode_as_they_always_have() {
    let designs = designs();
    let (_, sloan) = designs
      .iter()
      .find(|(file, _)| file == "fleet_escort_p_f_sloan")
      .expect("the P.F. Sloan");
    let mut generated = String::new();
    for (file, design) in &designs {
      for distance in DISTANCES {
        let pairs = [
          ("itself", design, design),
          ("vs sloan", design, sloan),
          ("sloan vs", sloan, design),
        ];
        for (label, blue, red) in pairs {
          writeln!(
            generated,
            "{file} {label} {distance:e} {:016x}",
            fingerprint(blue, red, distance)
          )
          .unwrap();
        }
      }
    }

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    if std::env::var("UPDATE_SIDE_FINGERPRINTS").is_ok() {
      std::fs::write(&path, &generated).expect("could not write the fingerprints");
      return;
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    let changed: Vec<&str> = generated
      .lines()
      .filter(|line| !on_disk.lines().any(|known| known == *line))
      .collect();
    assert!(
      changed.is_empty() && on_disk.lines().count() == generated.lines().count(),
      "the network now sees these differently:\n  {}",
      changed.join("\n  ")
    );
  }
}

/// The capital-ship and carrier inputs of step 12, and the flight head's
/// mask: what the network is told of a spinal, a magazine, a deck and a
/// carrier, and when it may launch or land.
#[cfg(test)]
mod capital_tests {
  use super::*;
  use crate::observation::observe;
  use callisto::entity::Entities;
  use callisto::ship::{FeatureKind, Magazine, ShipDesignTemplate, ShipFeature, Team, Weapon, WeaponMount, WeaponType};
  use cgmath::Zero;
  use std::sync::Arc;

  struct Encoded {
    own: Vec<f32>,
    side: Vec<f32>,
    pairs: Vec<f32>,
    masks: Vec<bool>,
    tokens: Tokens,
  }

  impl Encoded {
    fn row(&self, ship: &str) -> &[f32] {
      let i = self.tokens.own.iter().position(|name| name == ship).expect("a token");
      &self.own[i * OWN_FEATS..(i + 1) * OWN_FEATS]
    }
    fn pair(&self, ship: &str, foe: &str) -> &[f32] {
      let i = self.tokens.own.iter().position(|name| name == ship).expect("a token");
      let j = self.tokens.foes.iter().position(|name| name == foe).expect("a foe");
      &self.pairs[(i * FOES + j) * PAIR_FEATS..(i * FOES + j + 1) * PAIR_FEATS]
    }
    fn flight(&self, ship: &str) -> [bool; 3] {
      let i = self.tokens.own.iter().position(|name| name == ship).expect("a token");
      let at = i * MASK_LEN + offset(Head::Flight);
      [self.masks[at], self.masks[at + 1], self.masks[at + 2]]
    }
  }

  fn encode(world: &Entities, side: &[&str]) -> Encoded {
    let side: Vec<String> = side.iter().map(|name| (*name).to_string()).collect();
    let view = observe(world, &side);
    let memory = Memory::default();
    let history = History::default();
    let ctx = SideContext {
      view: &view,
      memory: &memory,
      history: &history,
      round: 1,
      max_rounds: 20,
      own_hull_share: 1.0,
      enemy_hull_share: 1.0,
      goal: Goal::Destroy,
      post: None,
    };
    let (mut own, mut foes, mut side, mut pairs, mut masks) = (
      vec![0.0; OWN * OWN_FEATS],
      vec![0.0; FOES * FOE_FEATS],
      vec![0.0; SIDE_FEATS],
      vec![0.0; OWN * FOES * PAIR_FEATS],
      vec![false; OWN * MASK_LEN],
    );
    let tokens = encode_side(
      &ctx,
      &mut Buffers {
        own: &mut own,
        foes: &mut foes,
        side: &mut side,
        pairs: &mut pairs,
        masks: &mut masks,
      },
    );
    Encoded {
      own,
      side,
      pairs,
      masks,
      tokens,
    }
  }

  fn feature(kind: FeatureKind) -> ShipFeature {
    ShipFeature {
      name: "deck".to_string(),
      power: 0,
      default_on: true,
      kind: Some(kind),
    }
  }

  /// A 50,000-ton carrier: thirty 50-ton berths, one tube, one deck, 100
  /// missiles in stores, and a triple missile turret of her own.
  fn carrier() -> Arc<ShipDesignTemplate> {
    Arc::new(ShipDesignTemplate {
      name: "Carrier".to_string(),
      displacement: 50_000,
      hull: 27_500,
      maneuver: 4,
      power: 60_000,
      features: vec![
        feature(FeatureKind::DockingSpace {
          craft_tons: 50,
          count: 30,
        }),
        feature(FeatureKind::LaunchTube {
          craft_tons: 50,
          count: 1,
        }),
        feature(FeatureKind::RecoveryDeck {
          craft_tons: 50,
          count: 1,
        }),
      ],
      weapons: vec![Weapon::uniform(WeaponType::Missile, WeaponMount::Turret, 3)],
      magazine: Magazine {
        missiles: 136,
        ..Magazine::default()
      },
      ..ShipDesignTemplate::default()
    })
  }

  /// A 50-ton fighter with a single missile rack and 12 missiles in all.
  fn fighter() -> Arc<ShipDesignTemplate> {
    Arc::new(ShipDesignTemplate {
      name: "Fighter".to_string(),
      displacement: 50,
      hull: 20,
      maneuver: 6,
      power: 60,
      jump: 0,
      weapons: vec![Weapon::uniform(WeaponType::Missile, WeaponMount::Turret, 1)],
      magazine: Magazine {
        missiles: 12,
        ..Magazine::default()
      },
      ..ShipDesignTemplate::default()
    })
  }

  fn monitor() -> Arc<ShipDesignTemplate> {
    Arc::new(ShipDesignTemplate {
      name: "Monitor".to_string(),
      displacement: 30_000,
      hull: 15_000,
      maneuver: 1,
      power: 10_000,
      tl: 15,
      weapons: vec![
        Weapon::single(WeaponType::Meson, WeaponMount::Spinal { multiple: 2, tl: None }),
        Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 3),
      ],
      ..ShipDesignTemplate::default()
    })
  }

  fn add(world: &mut Entities, name: &str, team: Team, at: Vec3, design: &Arc<ShipDesignTemplate>) {
    world.add_ship(name.to_string(), at, Vec3::zero(), design, None, None);
    world.ships[name].write().unwrap().team = Some(team);
  }

  /// The carrier on Blue at the origin with two fighters aboard and a raider
  /// in contact at Medium.
  fn deck() -> Entities {
    let mut world = Entities::default();
    add(&mut world, "Mother", Team::Blue, Vec3::zero(), &carrier());
    add(&mut world, "Chick 1", Team::Blue, Vec3::zero(), &fighter());
    add(&mut world, "Chick 2", Team::Blue, Vec3::zero(), &fighter());
    add(&mut world, "Raider", Team::Red, Vec3::new(5.0e6, 0.0, 0.0), &monitor());
    world.embark_ship("Chick 1", "Mother").unwrap();
    world.embark_ship("Chick 2", "Mother").unwrap();
    world.establish_initial_contacts();
    world
  }

  #[test]
  fn a_carrier_reads_her_deck_and_may_launch() {
    let world = deck();
    let encoded = encode(&world, &["Mother", "Chick 1", "Chick 2"]);
    assert_eq!(encoded.tokens.own, vec!["Mother"], "craft aboard take no slot");
    let row = &encoded.row("Mother")[OWN_CAPITAL_AT..];
    assert_eq!(row[0], 0.0, "no spinal");
    assert_eq!(row[2], 1.0, "her turret is full");
    assert_eq!(row[3], 2.0 / 30.0, "craft aboard over berths");
    assert_eq!(row[4], 1.0, "the tube takes ten a round: both go at once");
    assert_eq!(row[5], 1.0, "both are ready");
    assert_eq!(row[6], 0.0, "she is nobody's craft");
    assert_eq!(row[12], 1.0, "her stores are full");
    assert_eq!(encoded.flight("Mother"), [true, true, false]);
    assert_eq!(encoded.side[SIDE_CAPITAL_AT], 2.0 / 3.0, "two hulls aboard of three");
  }

  #[test]
  fn a_pinned_or_jumping_carrier_may_not_launch() {
    let world = deck();
    world.ships["Mother"].write().unwrap().pinned = true;
    assert_eq!(encode(&world, &["Mother"]).flight("Mother"), [true, false, false]);
    let empty = {
      let mut world = Entities::default();
      add(&mut world, "Mother", Team::Blue, Vec3::zero(), &carrier());
      world
    };
    assert_eq!(
      encode(&empty, &["Mother"]).flight("Mother"),
      [true, false, false],
      "nothing aboard"
    );
  }

  #[test]
  fn a_craft_reads_her_carrier_and_may_land() {
    let mut world = deck();
    world.add_ship(
      "Chick 3".to_string(),
      Vec3::new(0.0, 1.0e5, 0.0),
      Vec3::new(0.0, 100.0, 0.0),
      &fighter(),
      None,
      None,
    );
    {
      let mut craft = world.ships["Chick 3"].write().unwrap();
      craft.team = Some(Team::Blue);
      craft.carrier = Some("Mother".to_string());
    }
    world.ships["Mother"].write().unwrap().current_hull = 11_000;
    let encoded = encode(&world, &["Mother", "Chick 3"]);
    let row = &encoded.row("Chick 3")[OWN_CAPITAL_AT..];
    assert_eq!(row[6], 1.0, "a craft");
    assert_eq!(row[7], 1.0, "her carrier is on the plot");
    assert_eq!(row[8], 0.0, "Short");
    let limit = callisto::flight::landing_velocity_limit(6);
    assert!(
      (row[9] - (100.0 / limit / 2.0) as f32).abs() < 1e-6,
      "closing against the limit"
    );
    assert_eq!(row[10], 11_000.0 / 27_500.0, "her carrier's hull");
    assert_eq!(row[11], 0.0, "her rack is loaded");
    assert_eq!(row[12], 1.0, "her carrier's stores");
    assert_eq!(row[14], 1.0, "on the approach");
    assert_eq!(encoded.flight("Chick 3"), [true, false, true]);

    // Spent, and too fast to land.
    {
      let mut craft = world.ships["Chick 3"].write().unwrap();
      craft.loaded[0] = Magazine::default();
      craft.magazine = Magazine::default();
      craft.set_velocity(Vec3::new(0.0, 1.0e5, 0.0));
    }
    let encoded = encode(&world, &["Mother", "Chick 3"]);
    let row = &encoded.row("Chick 3")[OWN_CAPITAL_AT..];
    assert_eq!(row[2], 0.0, "nothing loaded");
    assert_eq!(row[11], 1.0, "spent");
    assert_eq!(row[9], 1.0, "far past the limit");
    assert_eq!(row[14], 0.0, "not on the approach");
    assert_eq!(encoded.flight("Chick 3"), [true, false, true], "land still flies her home");

    // Her carrier gone, she has nowhere to land.
    world.ships.remove("Mother");
    let encoded = encode(&world, &["Chick 3"]);
    assert_eq!(encoded.row("Chick 3")[OWN_CAPITAL_AT + 7], 0.0);
    assert_eq!(encoded.flight("Chick 3"), [true, false, false]);
  }

  #[test]
  fn a_spinal_is_read_against_each_target() {
    let mut world = Entities::default();
    add(&mut world, "Monitor", Team::Blue, Vec3::zero(), &monitor());
    add(&mut world, "Fighter", Team::Red, Vec3::new(5.0e6, 0.0, 0.0), &fighter());
    add(&mut world, "Cruiser", Team::Red, Vec3::new(5.5e6, 0.0, 0.0), &monitor());
    world.establish_initial_contacts();
    let encoded = encode(&world, &["Monitor"]);
    let row = &encoded.row("Monitor")[OWN_CAPITAL_AT..];
    assert_eq!(row[0], 1.0, "a spinal");
    assert_eq!(row[1], 0.0, "it cannot be laid on the nearest contact, a fighter under way");
    assert_eq!(row[2], 1.0, "no launcher: nothing to reload");
    assert_eq!(encoded.pair("Monitor", "Fighter")[PAIR_CAPITAL_AT], 0.0);
    assert_eq!(encoded.pair("Monitor", "Cruiser")[PAIR_CAPITAL_AT], 1.0);
    assert_eq!(encoded.flight("Monitor"), [true, false, false]);

    world.ships["Fighter"].write().unwrap().held_still = true;
    let encoded = encode(&world, &["Monitor"]);
    assert_eq!(encoded.pair("Monitor", "Fighter")[PAIR_CAPITAL_AT], 1.0, "held still");

    // With the cruiser nearest, the spinal is nearly all her punch on it.
    world.ships["Fighter"].write().unwrap().set_position(Vec3::new(6.0e6, 0.0, 0.0));
    let share = encode(&world, &["Monitor"]).row("Monitor")[OWN_CAPITAL_AT + 1];
    assert!(share > 0.9 && share <= 1.0, "{share}");
  }
}
