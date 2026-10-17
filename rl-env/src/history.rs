//! What a side remembers from the last round: the orders its ships were
//! given, and what happened -- hull lost, and when each ship was last hit.
//!
//! Built from the orders actually given, not from the network's choices, so
//! it means the same in training and in play, whoever gave them.

use std::collections::HashMap;

use ai_ships::observation::Observation;
use callisto::entity::Entity;
use callisto::ship::Range;
use callisto::software::SoftwareKind;
use callisto::tactics::{EngineerVerb, Helm, Intent, SensorVerb};

use crate::action::{Verb, HELM_BANDS};

/// One ship's orders last round, as the observation reads them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LastOrders {
  pub verb: Option<Verb>,
  /// The enemy her helm steered by.
  pub steered_by: Option<String>,
  pub band: Option<Range>,
  /// The friend she followed.
  pub guide: Option<String>,
  pub fired_at: Option<String>,
  pub locked: Option<String>,
  pub withdrawing: bool,
}

impl LastOrders {
  #[must_use]
  pub fn of(intent: &Intent) -> Self {
    let (verb, steered_by, band, guide) = match &intent.helm {
      Helm::Hold | Helm::Station { .. } => (Some(Verb::Hold), None, None, None),
      Helm::Intercept { target, band } => (Some(Verb::Intercept), Some(target.clone()), Some(*band), None),
      Helm::HoldRange { target, band } => (Some(Verb::HoldRange), Some(target.clone()), Some(*band), None),
      Helm::OpenRange { target } => (Some(Verb::OpenRange), Some(target.clone()), None, None),
      Helm::Follow { guide, .. } => (Some(Verb::Follow), None, None, Some(guide.clone())),
    };
    let withdrawing = intent
      .engineering
      .iter()
      .any(|verb| matches!(verb, EngineerVerb::Jump | EngineerVerb::Power { online: false, .. }))
      || intent
        .software
        .iter()
        .any(|order| order.running && order.program == SoftwareKind::JumpControl);
    LastOrders {
      verb,
      steered_by,
      band,
      guide,
      fired_at: intent.fire.first().map(|engage| engage.target.clone()),
      locked: intent.sensors.iter().find_map(|verb| match verb {
        SensorVerb::Lock { target } => Some(target.clone()),
        _ => None,
      }),
      withdrawing,
    }
  }

  /// The band, as the helm's bands number it.
  #[must_use]
  pub fn band_index(&self) -> Option<usize> {
    self.band.and_then(|band| HELM_BANDS.iter().position(|b| *b == band))
  }
}

/// A side's memory of the last round.
#[derive(Debug, Clone, Default)]
pub struct History {
  orders: HashMap<String, LastOrders>,
  /// Hull as last seen, by ship, ours and contacts'.
  hull_seen: HashMap<String, u32>,
  /// Hull lost between the last two looks.
  hull_lost: HashMap<String, u32>,
  /// The round each of our ships last lost hull.
  last_hit: HashMap<String, u32>,
  round: u32,
  /// The round last looked at, so a second look in the same round changes
  /// nothing.
  looked: Option<u32>,
}

impl History {
  /// Note the orders the side's ships were given this round.
  pub fn record(&mut self, intents: &[(String, Intent)]) {
    self.orders = intents
      .iter()
      .map(|(ship, intent)| (ship.clone(), LastOrders::of(intent)))
      .collect();
  }

  /// Look at the side's view as a round begins: what each ship and contact
  /// has lost since the last look.
  pub fn look(&mut self, view: &Observation) {
    if self.looked == Some(view.round) {
      return;
    }
    self.looked = Some(view.round);
    self.round = view.round;
    let hulls = view
      .own
      .iter()
      .map(|ship| (ship.get_name().to_string(), ship.current_hull))
      .chain(view.contacts.iter().map(|contact| (contact.name.clone(), contact.current_hull)));
    let mut lost = HashMap::new();
    for (name, hull) in hulls {
      let before = self.hull_seen.insert(name.clone(), hull);
      let loss = before.map_or(0, |before| before.saturating_sub(hull));
      if loss > 0 && view.own.iter().any(|ship| ship.get_name() == name) {
        self.last_hit.insert(name.clone(), view.round);
      }
      lost.insert(name, loss);
    }
    self.hull_lost = lost;
  }

  #[must_use]
  pub fn orders(&self, ship: &str) -> Option<&LastOrders> {
    self.orders.get(ship)
  }

  /// Hull this ship lost last round.
  #[must_use]
  pub fn lost(&self, ship: &str) -> u32 {
    self.hull_lost.get(ship).copied().unwrap_or(0)
  }

  /// Rounds since this ship of ours last lost hull; `None` if never.
  #[must_use]
  pub fn since_hit(&self, ship: &str) -> Option<u32> {
    self.last_hit.get(ship).map(|round| self.round.saturating_sub(*round))
  }

  /// The share of the side's ships that fired at this enemy last round.
  #[must_use]
  pub fn fire_share(&self, enemy: &str) -> f32 {
    if self.orders.is_empty() {
      return 0.0;
    }
    let firing = self
      .orders
      .values()
      .filter(|orders| orders.fired_at.as_deref() == Some(enemy))
      .count();
    firing as f32 / self.orders.len() as f32
  }
}
