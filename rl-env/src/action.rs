//! A ship's choices as numbers, one per head, and how they become orders.
//!
//! Each head is an independent discrete choice. Targets and guides are slots
//! in the [`Picture`] the ship's observation was encoded with, so slot 3 is
//! whichever enemy was fourth nearest when she looked.
//!
//! Only the choices that decide a fight are the network's. The rest stay on
//! the rules every baseline uses: engineers repair in [`REPAIR_PRIORITY`]
//! unless the ship jumps out, point defence is left to the tactics layer, the
//! captain and the computer are not used -- but for starting Jump Control,
//! free, when a ship jumps.

use std::collections::{BTreeMap, HashMap};

use ai_ships::commander::REPAIR_PRIORITY;
use ai_ships::decision::{straggler_margin, STRAGGLER_STEP};
use callisto::entity::Entity;
use callisto::ship::{Range, Ship};
use callisto::software::SoftwareKind;
use callisto::tactics::{Engage, EngineerVerb, Helm, Intent, Posture, SensorVerb, SoftwareOrder};
use cgmath::InnerSpace;

use crate::encode::{posture_of, Picture, ENEMIES, FRIENDS};

/// The helm's verbs, in the order of the verb head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
  Hold,
  Intercept,
  HoldRange,
  OpenRange,
  Follow,
}

impl Verb {
  pub const ALL: [Verb; 5] = [
    Verb::Hold,
    Verb::Intercept,
    Verb::HoldRange,
    Verb::OpenRange,
    Verb::Follow,
  ];
}

/// The bands a helm may aim for. Distant is not one: nothing reaches it.
pub const HELM_BANDS: [Range; 4] = [Range::Short, Range::Medium, Range::Long, Range::VeryLong];

/// Dodge reserve choices: 0, 1 or 2 G.
pub const DODGES: usize = 3;

const POSTURES: [Posture; 3] = [Posture::Silent, Posture::Passive, Posture::Active];

/// The action heads, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Head {
  Verb,
  HelmTarget,
  Band,
  Guide,
  Fire,
  Lock,
  Posture,
  Dodge,
  Assist,
  Jump,
}

impl Head {
  pub const ALL: [Head; 10] = [
    Head::Verb,
    Head::HelmTarget,
    Head::Band,
    Head::Guide,
    Head::Fire,
    Head::Lock,
    Head::Posture,
    Head::Dodge,
    Head::Assist,
    Head::Jump,
  ];

  /// How many choices the head has.
  #[must_use]
  pub const fn size(self) -> usize {
    match self {
      Head::Verb => Verb::ALL.len(),
      Head::HelmTarget => ENEMIES,
      Head::Band => HELM_BANDS.len(),
      Head::Guide => FRIENDS,
      Head::Fire | Head::Lock => ENEMIES + 1,
      Head::Posture => POSTURES.len(),
      Head::Dodge => DODGES,
      Head::Assist | Head::Jump => 2,
    }
  }

  /// Where the head's choices start in a ship's mask.
  #[must_use]
  pub const fn offset(self) -> usize {
    let mut at = 0;
    let mut i = 0;
    while i < self as usize {
      at += Head::ALL[i].size();
      i += 1;
    }
    at
  }
}

/// The number of heads.
pub const HEADS: usize = Head::ALL.len();
/// One ship's mask: every head's choices, end to end.
pub const MASK_LEN: usize = Head::Jump.offset() + Head::Jump.size();
/// Each head's size, for the Python side's action space.
pub const HEAD_SIZES: [usize; HEADS] = {
  let mut sizes = [0; HEADS];
  let mut i = 0;
  while i < HEADS {
    sizes[i] = Head::ALL[i].size();
    i += 1;
  }
  sizes
};

/// One ship's choices, one number per head.
pub type Choice = [i64; HEADS];

fn pick(names: &[String], slot: i64) -> Option<&str> {
  usize::try_from(slot).ok().and_then(|slot| names.get(slot)).map(String::as_str)
}

/// A slot for "none, or one of these": 0 is none, `n + 1` is slot `n`.
fn pick_or_none(names: &[String], choice: i64) -> Option<&str> {
  pick(names, choice - 1)
}

/// The helm a ship's choice asks for, and the guide she would follow.
fn helm_of(choice: &Choice, picture: &Picture) -> (Helm, Option<String>) {
  let verb = usize::try_from(choice[Head::Verb as usize])
    .ok()
    .and_then(|verb| Verb::ALL.get(verb))
    .copied()
    .unwrap_or(Verb::Hold);
  let target = pick(&picture.enemies, choice[Head::HelmTarget as usize]).map(str::to_string);
  let band = usize::try_from(choice[Head::Band as usize])
    .ok()
    .and_then(|band| HELM_BANDS.get(band))
    .copied()
    .unwrap_or(Range::Long);
  match (verb, target) {
    (Verb::Intercept, Some(target)) => (Helm::Intercept { target, band }, None),
    (Verb::HoldRange, Some(target)) => (Helm::HoldRange { target, band }, None),
    (Verb::OpenRange, Some(target)) => (Helm::OpenRange { target }, None),
    (Verb::Follow, _) => (
      Helm::Hold,
      pick(&picture.friends, choice[Head::Guide as usize]).map(str::to_string),
    ),
    _ => (Helm::Hold, None),
  }
}

/// A ship's orders from her choice, the helm aside: fire, lock, posture,
/// dodge, assist, and the standing repair order.
fn intent_of(choice: &Choice, picture: &Picture, helm: Helm) -> Intent {
  let fire = pick_or_none(&picture.enemies, choice[Head::Fire as usize]);
  let lock = pick_or_none(&picture.enemies, choice[Head::Lock as usize]);
  let index = |head: Head| usize::try_from(choice[head as usize]).unwrap_or(0).min(head.size() - 1);
  let jump = index(Head::Jump) == 1;
  Intent {
    posture: Some(POSTURES[index(Head::Posture)]),
    helm,
    evade: u8::try_from(index(Head::Dodge)).ok(),
    assist_gunners: Some(index(Head::Assist) == 1),
    fire: fire
      .map(|target| Engage {
        target: target.to_string(),
        ..Engage::default()
      })
      .into_iter()
      .collect(),
    sensors: lock
      .map(|target| SensorVerb::Lock {
        target: target.to_string(),
      })
      .into_iter()
      .collect(),
    engineering: vec![if jump {
      EngineerVerb::Jump
    } else {
      EngineerVerb::Repair {
        priority: REPAIR_PRIORITY.to_vec(),
      }
    }],
    // Jump Control must be running to jump; starting it costs nothing.
    software: if jump {
      vec![SoftwareOrder {
        program: SoftwareKind::JumpControl,
        running: true,
      }]
    } else {
      vec![]
    },
    ..Intent::default()
  }
}

/// The ship at the head of a chain of follows, or `None` for a loop.
fn root_of<'a>(ship: &'a str, guides: &'a HashMap<String, String>) -> Option<&'a str> {
  let mut at = ship;
  for _ in 0..=guides.len() {
    match guides.get(at) {
      Some(next) => at = next,
      None => return Some(at),
    }
  }
  None
}

/// Every ship of a side's orders, from their choices: `own` is the side's
/// ships still in play, each with her choice and the picture it was made
/// from.
///
/// A ship that follows joins her guide's squadron, and a guide who herself
/// follows hands her followers on to her own guide, so every squadron is
/// led by a ship flying her own helm. The guide then flies at the slowest
/// member's thrust, eased off while anyone is out of station, exactly as a
/// model commander's squadrons fly. Follows that go round in a loop hold.
#[must_use]
pub fn side_intents(own: &[(&Ship, &Choice, &Picture)]) -> Vec<(String, Intent)> {
  let mut helms: BTreeMap<String, Helm> = BTreeMap::new();
  let mut guides: HashMap<String, String> = HashMap::new();
  for (ship, choice, picture) in own {
    let (helm, guide) = helm_of(choice, picture);
    let name = ship.get_name().to_string();
    if let Some(guide) = guide.filter(|guide| own.iter().any(|(other, _, _)| other.get_name() == guide)) {
      guides.insert(name.clone(), guide);
    }
    helms.insert(name, helm);
  }

  // Squadrons: each root and the ships that end up following her.
  let mut squadrons: BTreeMap<String, Vec<String>> = BTreeMap::new();
  for ship in guides.keys() {
    if let Some(root) = root_of(ship, &guides) {
      squadrons.entry(root.to_string()).or_default().push(ship.clone());
    }
  }
  let looped: Vec<String> = guides.keys().filter(|ship| root_of(ship, &guides).is_none()).cloned().collect();

  let ship_named = |name: &str| {
    own
      .iter()
      .find(|(ship, _, _)| ship.get_name() == name)
      .map(|(ship, _, _)| *ship)
  };
  let mut caps: HashMap<String, f64> = HashMap::new();
  let mut follows: HashMap<String, Helm> = HashMap::new();
  for (guide_name, followers) in &squadrons {
    let Some(guide) = ship_named(guide_name) else { continue };
    let members: Vec<&Ship> = std::iter::once(guide)
      .chain(followers.iter().filter_map(|name| ship_named(name)))
      .collect();
    let slowest = f64::from(members.iter().map(|ship| ship.max_acceleration()).min().unwrap_or(1));
    let margin = members
      .iter()
      .map(|ship| {
        straggler_margin(
          (ship.get_position() - guide.get_position()).magnitude(),
          (ship.get_velocity() - guide.get_velocity()).magnitude(),
        )
      })
      .fold(0.0, f64::max);
    let cap = (slowest - margin).max(STRAGGLER_STEP);
    caps.insert(guide_name.clone(), cap);
    let guide_helm = helms.get(guide_name).cloned().unwrap_or_default();
    for follower in followers {
      follows.insert(
        follower.clone(),
        Helm::Follow {
          guide: guide_name.clone(),
          guide_helm: Box::new(guide_helm.clone()),
          guide_cap: Some(cap),
        },
      );
    }
  }

  own
    .iter()
    .map(|(ship, choice, picture)| {
      let name = ship.get_name();
      let helm = if looped.iter().any(|ship| ship == name) {
        Helm::Hold
      } else {
        follows
          .remove(name)
          .unwrap_or_else(|| helms.get(name).cloned().unwrap_or_default())
      };
      let mut intent = intent_of(choice, picture, helm);
      intent.max_thrust = caps.get(name).copied();
      (name.to_string(), intent)
    })
    .collect()
}

/// The choice that asks for `intent`, as near as the heads can say it: what a
/// baseline's orders look like through this action space. Anything the
/// intent leaves as it is (posture, dodge, assist) is chosen as the ship has
/// it now, so asking for it changes nothing.
#[must_use]
pub fn choice_of(intent: &Intent, ship: &Ship, picture: &Picture) -> Choice {
  let slot = |name: &str| {
    picture
      .enemies
      .iter()
      .position(|enemy| enemy == name)
      .and_then(|slot| i64::try_from(slot).ok())
  };
  let band = |band: Range| {
    HELM_BANDS
      .iter()
      .position(|helm_band| *helm_band == band)
      .and_then(|band| i64::try_from(band).ok())
      .unwrap_or(2)
  };
  let mut choice: Choice = [0; HEADS];
  let mut helm = |verb: Verb, target: &str, band_index: i64| {
    if let Some(target) = slot(target) {
      choice[Head::Verb as usize] = verb as i64;
      choice[Head::HelmTarget as usize] = target;
      choice[Head::Band as usize] = band_index;
    }
  };
  match &intent.helm {
    Helm::Intercept { target, band: b } => helm(Verb::Intercept, target, band(*b)),
    Helm::HoldRange { target, band: b } => helm(Verb::HoldRange, target, band(*b)),
    Helm::OpenRange { target } => helm(Verb::OpenRange, target, 0),
    Helm::Hold | Helm::Station { .. } | Helm::Follow { .. } => {}
  }
  choice[Head::Fire as usize] = intent
    .fire
    .first()
    .and_then(|engage| slot(&engage.target))
    .map_or(0, |slot| slot + 1);
  choice[Head::Lock as usize] = intent
    .sensors
    .iter()
    .find_map(|verb| match verb {
      SensorVerb::Lock { target } => slot(target),
      _ => None,
    })
    .map_or(0, |slot| slot + 1);
  let posture = intent
    .posture
    .and_then(|posture| POSTURES.iter().position(|p| *p == posture))
    .unwrap_or_else(|| posture_of(ship));
  choice[Head::Posture as usize] = i64::try_from(posture).unwrap_or(0);
  choice[Head::Dodge as usize] = i64::from(intent.evade.unwrap_or_else(|| ship.get_dodge_thrust())).min(2);
  choice[Head::Assist as usize] = i64::from(intent.assist_gunners.unwrap_or_else(|| ship.get_assist_gunners()));
  choice[Head::Jump as usize] = i64::from(intent.engineering.iter().any(|verb| matches!(verb, EngineerVerb::Jump)));
  choice
}
