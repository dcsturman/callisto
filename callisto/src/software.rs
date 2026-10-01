//! Ship software and the Bandwidth it runs in.
//!
//! A ship's computer has a Processing score (Core Rulebook p. 180) and every
//! software package has a Bandwidth cost. Software is *installed* -- bought
//! with the ship -- and some of it *runs*; what limits a ship is how much can
//! run at once, not how much it owns. HMS Executor carries Evade/1, Fire
//! Control/2 and Jump Control/2, which is 30 Bandwidth on a Computer/20: she
//! can fight or she can jump, and the engineer chooses.
//!
//! Computers cost no Power and no tonnage: "while they do have a physical
//! presence, they are distributed throughout the ship and considered part of
//! other components" (p. 180), and no canon ship's Power Requirements lists
//! one. Bandwidth is the budget here, not Power.
//!
//! Packages are from the Core Rulebook p. 161 and High Guard pp. 73-76.

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

/// A kind of ship software, without its level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, EnumIter)]
pub enum SoftwareKind {
  // Core Rulebook p. 161.
  /// Basic control of the ship. Every ship runs it, and it costs nothing.
  Manoeuvre,
  /// Understands spoken orders. Free, and always running.
  Intellect,
  /// Reference data. Free, and always running.
  Library,
  /// Jumps of up to the listed number, astrogation included.
  JumpControl,
  /// The computer flies evasively: a negative DM to all attacks on the ship.
  Evade,
  /// Automated attacks, a DM to a gunner's attack, or any mix of the two.
  FireControl,
  /// Repair attempts, or a DM to one. Needs repair drones aboard.
  AutoRepair,
  // High Guard pp. 73-76.
  /// A DM to every attack roll the ship makes, with no automated fire.
  AdvancedFireControl,
  /// Hinders boarders and hackers. Nothing a space battle touches.
  AntiHijack,
  /// Sensor hand-off out to Medium (/1) or Long (/2) range.
  BattleNetwork,
  /// A DM to Tactics (naval) checks, which this simulation does not roll.
  BattleSystem,
  /// A free electronic warfare action against every salvo inside Long range.
  BroadSpectrumEw,
  /// A sentient ship's mind. Flavour here.
  ConsciousIntelligence,
  /// A DM to the sensor operator's electronic warfare checks.
  ElectronicWarfare,
  /// A DM to every missile and torpedo salvo the ship fires.
  LaunchSolution,
  /// Point defence on behalf of another ship, within Close (/1) or Short (/2).
  PointDefence,
  /// Angles screens automatically, at DM+0.
  ScreenOptimiser,
  /// Replaces pilots, gunners and sensor operators at the listed skill.
  VirtualCrew,
  /// Replaces gunners at the listed skill.
  VirtualGunner,
}

impl SoftwareKind {
  /// What a console calls it.
  #[must_use]
  pub fn label(self) -> &'static str {
    match self {
      SoftwareKind::Manoeuvre => "Manoeuvre",
      SoftwareKind::Intellect => "Intellect",
      SoftwareKind::Library => "Library",
      SoftwareKind::JumpControl => "Jump Control",
      SoftwareKind::Evade => "Evade",
      SoftwareKind::FireControl => "Fire Control",
      SoftwareKind::AutoRepair => "Auto-Repair",
      SoftwareKind::AdvancedFireControl => "Advanced Fire Control",
      SoftwareKind::AntiHijack => "Anti-Hijack",
      SoftwareKind::BattleNetwork => "Battle Network",
      SoftwareKind::BattleSystem => "Battle System",
      SoftwareKind::BroadSpectrumEw => "Broad Spectrum EW",
      SoftwareKind::ConsciousIntelligence => "Conscious Intelligence",
      SoftwareKind::ElectronicWarfare => "Electronic Warfare",
      SoftwareKind::LaunchSolution => "Launch Solution",
      SoftwareKind::PointDefence => "Point Defence",
      SoftwareKind::ScreenOptimiser => "Screen Optimiser",
      SoftwareKind::VirtualCrew => "Virtual Crew",
      SoftwareKind::VirtualGunner => "Virtual Gunner",
    }
  }

  /// The levels this package comes in, and what each costs in Bandwidth and
  /// Tech Level. An unlevelled package is listed as level 0.
  #[must_use]
  pub fn levels(self) -> &'static [(u8, u32, u8)] {
    match self {
      // Manoeuvre and Library are both TL8 and both free: same row, and the
      // table says so.
      SoftwareKind::Manoeuvre | SoftwareKind::Library => &[(0, 0, 8)],
      SoftwareKind::Intellect => &[(0, 0, 11)],
      SoftwareKind::JumpControl => &[
        (1, 5, 9),
        (2, 10, 11),
        (3, 15, 12),
        (4, 20, 13),
        (5, 25, 14),
        (6, 30, 15),
      ],
      SoftwareKind::Evade => &[(1, 10, 9), (2, 15, 11), (3, 25, 13)],
      SoftwareKind::FireControl => &[(1, 5, 9), (2, 10, 10), (3, 15, 11), (4, 20, 12), (5, 25, 13)],
      SoftwareKind::AutoRepair => &[(1, 10, 10), (2, 20, 12)],
      SoftwareKind::AdvancedFireControl => &[(1, 15, 10), (2, 25, 12), (3, 30, 14)],
      SoftwareKind::AntiHijack => &[(1, 2, 11), (2, 10, 12), (3, 15, 13)],
      SoftwareKind::BattleNetwork => &[(1, 5, 12), (2, 10, 14)],
      SoftwareKind::BattleSystem => &[(1, 5, 9), (2, 10, 12), (3, 15, 15)],
      SoftwareKind::BroadSpectrumEw => &[(0, 12, 13)],
      SoftwareKind::ConsciousIntelligence => &[(1, 40, 16), (2, 25, 17), (3, 10, 18)],
      SoftwareKind::ElectronicWarfare => &[(1, 10, 10), (2, 15, 13), (3, 20, 15)],
      SoftwareKind::LaunchSolution => &[(1, 5, 8), (2, 10, 10), (3, 15, 12)],
      SoftwareKind::PointDefence => &[(1, 12, 9), (2, 15, 12)],
      SoftwareKind::ScreenOptimiser => &[(0, 10, 10)],
      SoftwareKind::VirtualCrew => &[(0, 5, 10), (1, 10, 13), (2, 15, 15)],
      SoftwareKind::VirtualGunner => &[(0, 5, 9), (1, 10, 12), (2, 15, 15)],
    }
  }

  /// Whether this package carries a level at all, or is just itself.
  #[must_use]
  pub fn is_levelled(self) -> bool {
    self.levels().len() > 1 || self.levels().first().is_some_and(|(level, _, _)| *level > 0)
  }
}

/// One installed package: a kind at a level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct Software {
  pub kind: SoftwareKind,
  /// 0 for a package that has no levels, such as Library.
  #[serde(default)]
  pub level: u8,
}

impl Software {
  #[must_use]
  pub fn new(kind: SoftwareKind, level: u8) -> Software {
    Software { kind, level }
  }

  /// What it costs to run. An unknown level reads as its lowest.
  #[must_use]
  pub fn bandwidth(&self) -> u32 {
    self
      .kind
      .levels()
      .iter()
      .find(|(level, _, _)| *level == self.level)
      .or_else(|| self.kind.levels().first())
      .map_or(0, |(_, bandwidth, _)| *bandwidth)
  }

  /// Software that costs nothing to run is always running: there is no
  /// Bandwidth to free by stopping it, and a ship without Manoeuvre is not a
  /// ship anyone can fly.
  #[must_use]
  pub fn always_running(&self) -> bool {
    self.bandwidth() == 0
  }
}

impl std::fmt::Display for Software {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    if self.kind.is_levelled() {
      write!(f, "{}/{}", self.kind.label(), self.level)
    } else {
      write!(f, "{}", self.kind.label())
    }
  }
}
