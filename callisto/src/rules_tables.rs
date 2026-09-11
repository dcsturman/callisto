use crate::ship::{CounterMeasures, MountClass, Salvo, Stealth, WeaponProfile, WeaponType, AP_INFINITE};
use crate::ship::{Range, WeaponMount};

/// The Damage Multiple for a mount (High Guard p. 29).
///
/// Applies only to direct-fire weapons, and only *after* armour has been
/// subtracted.  Launchers scale by salvo size instead and never come through
/// here — see [`WeaponProfile::use_multiple`].
#[must_use]
pub const fn damage_multiple(mount: MountClass) -> u32 {
  match mount {
    MountClass::Barbette => 3,
    MountClass::SmallBay => 10,
    MountClass::MediumBay => 20,
    MountClass::LargeBay => 100,
    // Turrets and fixed mounts genuinely have no multiple.  A battery shares
    // the identity value because it never rolls damage at all, so this is never
    // consulted for one.
    MountClass::Turret | MountClass::Fixed | MountClass::Battery => 1,
  }
}

/// The stats for a weapon in a given mount, or `None` if the rules do not
/// allow that pairing.
///
/// This is transcribed from the Turret Weapons (p. 28), Barbettes (p. 30) and
/// Small/Medium/Large Bay Weapons (pp. 32-33) tables.  Because a weapon only
/// appears in the tables for mounts it is actually sold in, `None` doubles as
/// the mount-legality rule: there are no laser bays and no meson turrets in
/// High Guard, so those pairings simply have no entry.
///
/// Deliberately a `match` rather than a lookup table: adding a [`WeaponType`]
/// then fails to compile until every mount is considered, instead of silently
/// indexing out of bounds at runtime.
#[must_use]
#[allow(clippy::match_same_arms)]
pub fn weapon_profile(kind: WeaponType, mount: MountClass) -> Option<WeaponProfile> {
  use MountClass::{Barbette, Battery, Fixed, LargeBay, MediumBay, SmallBay, Turret};
  type P = WeaponProfile;

  Some(match kind {
    // Lasers are turret and barbette weapons only; the book sells no laser bay.
    WeaponType::Beam => match mount {
      Turret | Fixed => P::gun(10, 1, Range::Medium).hit(4),
      Barbette => P::gun(10, 2, Range::Medium).hit(4),
      _ => return None,
    },
    WeaponType::Pulse => match mount {
      Turret | Fixed => P::gun(9, 2, Range::Long).hit(2),
      Barbette => P::gun(9, 3, Range::Long).hit(2),
      _ => return None,
    },
    // Sandcasters roll no damage; combat.rs handles them as a defence.
    WeaponType::Sand => match mount {
      Turret | Fixed => P::special(9, Range::Short),
      _ => return None,
    },
    WeaponType::Missile => match mount {
      Turret => P::launcher(7, 4, Salvo::PerGun),
      Fixed => P::launcher(7, 4, Salvo::Fixed(1)),
      Barbette => P::launcher(7, 4, Salvo::Fixed(5)),
      SmallBay => P::launcher(7, 4, Salvo::Fixed(12)),
      MediumBay => P::launcher(7, 4, Salvo::Fixed(24)),
      LargeBay => P::launcher(7, 4, Salvo::Fixed(120)),
      Battery => return None,
    },
    // Torpedoes are treated in every way like missiles but hit far harder, and
    // are too large to fit a turret.  The barbette holds three and fires one at
    // a time, keeping it below a small bay's salvo of three.
    WeaponType::Torpedo => match mount {
      Barbette => P::launcher(7, 6, Salvo::Fixed(1)),
      SmallBay => P::launcher(7, 6, Salvo::Fixed(3)),
      MediumBay => P::launcher(7, 6, Salvo::Fixed(6)),
      LargeBay => P::launcher(7, 6, Salvo::Fixed(30)),
      _ => return None,
    },
    WeaponType::Particle => match mount {
      Turret | Fixed => P::gun(12, 3, Range::VeryLong).rad(),
      Barbette => P::gun(11, 4, Range::VeryLong).rad(),
      SmallBay => P::gun(11, 6, Range::VeryLong).rad(),
      MediumBay => P::gun(12, 8, Range::VeryLong).rad(),
      LargeBay => P::gun(13, 10, Range::Distant).rad(),
      Battery => return None,
    },
    WeaponType::Fusion => match mount {
      Turret | Fixed => P::gun(14, 4, Range::Medium).rad(),
      Barbette => P::gun(12, 5, Range::Medium).ap(3).rad(),
      SmallBay => P::gun(12, 6, Range::Medium).ap(6).rad(),
      MediumBay => P::gun(12, 7, Range::Medium).ap(6).rad(),
      LargeBay => P::gun(12, 10, Range::Long).ap(8).rad(),
      Battery => return None,
    },
    WeaponType::Plasma => match mount {
      Turret | Fixed => P::gun(11, 3, Range::Medium),
      Barbette => P::gun(11, 4, Range::Medium).ap(2),
      _ => return None,
    },
    WeaponType::Railgun => match mount {
      Turret | Fixed => P::gun(10, 2, Range::Short).ap(4),
      Barbette => P::gun(10, 3, Range::Medium).ap(5),
      SmallBay => P::gun(10, 3, Range::Short).ap(10),
      MediumBay => P::gun(10, 5, Range::Short).ap(10),
      LargeBay => P::gun(10, 6, Range::Medium).ap(10),
      Battery => return None,
    },
    // Meson guns are bay-and-up weapons that ignore armour outright.
    WeaponType::Meson => match mount {
      SmallBay => P::gun(11, 5, Range::Long).ap(AP_INFINITE).rad(),
      MediumBay => P::gun(12, 6, Range::Long).ap(AP_INFINITE).rad(),
      LargeBay => P::gun(13, 6, Range::Long).ap(AP_INFINITE).rad(),
      _ => return None,
    },
    WeaponType::MassDriver => match mount {
      SmallBay => P::gun(8, 3, Range::Short),
      MediumBay => P::gun(8, 4, Range::Short),
      LargeBay => P::gun(8, 6, Range::Medium),
      _ => return None,
    },
    // The book gives repulsors "Special" damage: they deflect rather than
    // destroy.  The deflection mechanic is not modelled, so a repulsor can be
    // designed and mounted but currently does nothing in combat.
    WeaponType::Repulsor => match mount {
      SmallBay => P::special(15, Range::Short),
      MediumBay => P::special(14, Range::Short),
      LargeBay => P::special(13, Range::Short),
      _ => return None,
    },
    // Ion cannons are barbette-and-bay weapons.  They take the ordinary Damage
    // Multiple -- the book's fleet-scale Ion table (p. 132) lists 75 / 200 /
    // 500 / 3,500 for barbette / small / medium / large, which is exactly these
    // dice times the multiples, so the two scales agree.
    WeaponType::Ion => match mount {
      Barbette => P::gun(12, 7, Range::Medium).ion(),
      SmallBay => P::gun(12, 6, Range::Medium).ion(),
      MediumBay => P::gun(12, 8, Range::Medium).ion(),
      LargeBay => P::gun(12, 10, Range::Long).ion(),
      _ => return None,
    },
    // A battery is not a gun.  It has no attack roll, no damage and no range
    // band; the profile exists only so that the legality matrix knows a
    // point-defence battery goes in a Battery mount and nowhere else -- and,
    // just as importantly, that nothing else goes in a Battery.  Its actual
    // effect is `battery_intercept_dice` in combat.rs.
    WeaponType::PointDefense => match mount {
      Battery => P::special(10, Range::Short),
      _ => return None,
    },
  })
}

/// Convenience wrapper for the common case of holding a real [`WeaponMount`].
#[must_use]
pub fn profile_for(kind: WeaponType, mount: &WeaponMount) -> Option<WeaponProfile> {
  weapon_profile(kind, MountClass::from(mount))
}

// Range bands for Short, Medium, Long, Very Long
pub const RANGE_BANDS: [u32; 4] = [1_250_000, 10_000_000, 25_000_000, 50_000_000];

// One more than the number of range bands to handle Distant
pub const RANGE_MOD: [i32; 5] = [1, 0, -2, -4, -6];

// DM to sensor checks based on sensor quality
pub const SENSOR_QUALITY_MOD: [i16; 5] = [-4, -2, 0, 1, 2];

// DM to sensor checks based on stealth
pub fn stealth_mod(stealth: Option<Stealth>) -> i16 {
  match stealth {
    None => 0,
    Some(stealth) => STEALTH_MOD[stealth as usize],
  }
}
// Use this locally only.
const STEALTH_MOD: [i16; 4] = [-2, -2, -4, -6];

/// Total DM applied to an Electronics (sensors) check made by an observer at
/// `observer_tl` against a target at `target_tl` carrying `target_stealth`.
///
/// This combines two rules that High Guard keeps separate, and that were
/// previously conflated into a single clamped term:
///
/// * **Initial Detection (p. 76) and Stealthed Ships (p. 77):** "TL difference
///   between ships: +1 per higher TL", with the worked example "A TL15 ship
///   receives DM+3 to detect a TL12 ship". This is a bonus for the
///   better-teched *observer*, and it applies whether or not the target has
///   stealth. Facing a higher-TL ship carries no matching penalty here.
/// * **Stealth Types (p. 14):** the coating's own DM-2/-4/-6, plus "an
///   additional DM-1 for every Tech Level the ship is higher than the sensors
///   trying to locate it". Both are penalties, and both apply only to a target
///   that actually has stealth.
///
/// The two TL terms are mutually exclusive: at most one of them is non-zero for
/// any given pair, so a stealthed target reduces to `delta + grade` while a
/// plain target reduces to `max(0, delta)`.
#[must_use]
pub fn detection_modifiers(observer_tl: u8, target_tl: u8, target_stealth: Option<Stealth>) -> i16 {
  let delta = i16::from(observer_tl) - i16::from(target_tl);

  // "+1 per higher TL" - observer only, never a penalty.
  let tl_bonus = delta.max(0);

  // Stealth grade, plus DM-1 per TL the target is above the observer.
  let stealth = if target_stealth.is_some() {
    stealth_mod(target_stealth) + delta.min(0)
  } else {
    0
  };

  tl_bonus + stealth
}

/// Target-side DMs for a sensor check: how loud the ship being looked for is.
///
/// See [`Emissions`] for the fields; [`Emissions::detection_dm`] sums them.
///
/// High Guard prints these as two tables — Initial Detection (p. 76) and
/// Stealthed Ships (p. 77) — but they are one model written twice. Four rows
/// are word-for-word identical between them, including the same worked example.
/// The rows that differ do so only because of *when* each table is used: the
/// first describes an approach, where nobody is shooting yet and nothing has
/// taken a critical, and the second describes a ship that has already gone
/// dark, so its power plant is off by assumption.
///
/// The book itself collapses them when describing the same situation in prose,
/// listing the giveaways as one set: a powered-down ship stays hidden "until
/// they reveal themselves with a tell-tale sign: use of active sensors,
/// transponder, manoeuvre drives or firing a weapon, just to name a few."
///
/// Callisto is always in the moment where any of it can happen, so it uses the
/// union, and the rows stack — a ship that is both lit up and shooting is
/// easier to find than one doing only one of those:
///
/// * running active sensors, +2
/// * operating its manoeuvre drive, +1 per G of thrust
/// * operating its power plant, +1
/// * fired weapons this round, +2
/// * damaged and emitting heat, +1 per Severity
/// * transmitting — transponder or radio comms — +6
///
/// The transponder and comms rows are one flag: the book prints them as a
/// single row, and they are the same emission to anyone listening.
/// The tech-level and stealth rows live in [`detection_modifiers`], because
/// they depend on both ships rather than just the target.
/// What a ship is doing that someone hunting it could notice.
///
/// Grouped rather than passed as a row of loose booleans, which is both easier
/// to read at the call site and keeps the fields named where they are set.
/// The bools are genuinely independent rows of a rules table rather than a
/// state machine wanting an enum, so the lint against several of them does not
/// apply here.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy)]
pub struct Emissions {
  /// Running active radar/lidar.
  pub active_sensors: bool,
  /// Thrust being applied, in whole G.
  pub thrust_g: u8,
  /// Power plant at minimum level or higher, which is any functioning ship.
  pub power_plant: bool,
  /// Fired any weapon this round.
  pub fired_weapons: bool,
  /// Total severity of criticals taken, which shows up as heat.
  pub crit_severity: u16,
  /// Radiating on RF: transponder, radio comms, or both.
  pub transmitting: bool,
}

impl Emissions {
  /// The DM this ship's behaviour gives to anyone making a sensor check
  /// against it. Rows stack.
  #[must_use]
  pub fn detection_dm(self) -> i16 {
    i16::from(self.active_sensors) * 2
      + i16::from(self.thrust_g)
      + i16::from(self.power_plant)
      + i16::from(self.fired_weapons) * 2
      + i16::try_from(self.crit_severity).unwrap_or(i16::MAX)
      + i16::from(self.transmitting) * 6
  }
}

pub fn countermeasures_mod(countermeasures: Option<CounterMeasures>) -> i16 {
  match countermeasures {
    None => 0,
    Some(CounterMeasures::Standard) => 2,
    Some(CounterMeasures::Military) => 4,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ship::BaySize;
  use strum::IntoEnumIterator;

  // Derived from the enums rather than written out, so a new weapon or mount
  // cannot quietly escape the checks below.  These arrays used to be
  // hand-maintained, and an ion cannon added to `weapon_profile` went missing
  // from the generated frontend matrix without a single test noticing --
  // because the generator and the checked-in file were both walking the same
  // incomplete list.
  fn all_weapons() -> impl Iterator<Item = WeaponType> {
    WeaponType::iter()
  }

  fn all_mounts() -> impl Iterator<Item = MountClass> {
    MountClass::iter()
  }

  /// A weapon scales by damage multiple *or* by salvo size, never both and
  /// never neither-when-it-should.  This is the invariant that keeps bay
  /// missiles from being counted twice.
  #[test]
  fn multiple_and_salvo_are_mutually_exclusive() {
    for kind in all_weapons() {
      for mount in all_mounts() {
        let Some(profile) = weapon_profile(kind, mount) else {
          continue;
        };
        assert!(
          !(profile.use_multiple && profile.salvo.is_some()),
          "{kind:?} in {mount:?} both takes a damage multiple and launches a salvo"
        );
      }
    }
  }

  /// Launchers carry the Smart trait and have no range limit; direct-fire
  /// weapons have a range band.
  #[test]
  fn launchers_are_smart_and_unlimited_in_range() {
    for kind in all_weapons() {
      for mount in all_mounts() {
        let Some(profile) = weapon_profile(kind, mount) else {
          continue;
        };
        if profile.salvo.is_some() {
          assert!(profile.smart, "{kind:?} in {mount:?} launches but is not Smart");
          assert!(
            profile.max_range.is_none(),
            "{kind:?} in {mount:?} launches but has a range limit"
          );
        }
      }
    }
  }

  /// High Guard sells no torpedo turret, and no meson, mass driver or repulsor
  /// outside a bay.  These are the pairings the Add Ship editor must refuse.
  #[test]
  fn illegal_mounts_have_no_profile() {
    assert!(weapon_profile(WeaponType::Torpedo, MountClass::Turret).is_none());
    assert!(weapon_profile(WeaponType::Torpedo, MountClass::Fixed).is_none());
    for mount in [MountClass::Turret, MountClass::Fixed, MountClass::Barbette] {
      for kind in [WeaponType::Meson, WeaponType::MassDriver, WeaponType::Repulsor] {
        assert!(
          weapon_profile(kind, mount).is_none(),
          "{kind:?} should not be mountable as {mount:?}"
        );
      }
    }
    // There are no laser bays in the book.
    for kind in [WeaponType::Beam, WeaponType::Pulse, WeaponType::Sand] {
      assert!(weapon_profile(kind, MountClass::LargeBay).is_none());
    }
  }

  /// Spot-check the transcription against the printed tables.
  #[test]
  fn profiles_match_the_book() {
    // Turret Weapons, p. 28.
    let beam = weapon_profile(WeaponType::Beam, MountClass::Turret).unwrap();
    assert_eq!((beam.damage_dice, beam.hit_mod), (1, 4));
    let railgun = weapon_profile(WeaponType::Railgun, MountClass::Turret).unwrap();
    assert_eq!((railgun.damage_dice, railgun.ap), (2, 4));

    // Barbettes, p. 30: a railgun reaches further from a barbette than a turret.
    let railgun_barbette = weapon_profile(WeaponType::Railgun, MountClass::Barbette).unwrap();
    assert_eq!(railgun_barbette.max_range, Some(Range::Medium));
    assert_eq!(
      weapon_profile(WeaponType::Railgun, MountClass::Turret).unwrap().max_range,
      Some(Range::Short)
    );

    // Bay Weapons, pp. 32-33.
    let meson = weapon_profile(WeaponType::Meson, MountClass::MediumBay).unwrap();
    assert_eq!((meson.damage_dice, meson.ap), (6, AP_INFINITE));
    // Only a large bay pushes a particle beam out to Distant.
    assert_eq!(
      weapon_profile(WeaponType::Particle, MountClass::LargeBay).unwrap().max_range,
      Some(Range::Distant)
    );
  }

  /// Salvo sizes, p. 30 (barbettes) and pp. 32-33 (bays).  A torpedo barbette
  /// fires one at a time, keeping it below a small bay's three.
  #[test]
  fn salvo_sizes_match_the_book() {
    let salvo = |kind, mount| weapon_profile(kind, mount).unwrap().salvo.unwrap();
    assert_eq!(salvo(WeaponType::Missile, MountClass::Barbette), Salvo::Fixed(5));
    assert_eq!(salvo(WeaponType::Missile, MountClass::LargeBay), Salvo::Fixed(120));
    assert_eq!(salvo(WeaponType::Missile, MountClass::Turret), Salvo::PerGun);
    assert_eq!(salvo(WeaponType::Torpedo, MountClass::Barbette), Salvo::Fixed(1));
    assert_eq!(salvo(WeaponType::Torpedo, MountClass::SmallBay), Salvo::Fixed(3));
    assert_eq!(salvo(WeaponType::Torpedo, MountClass::LargeBay), Salvo::Fixed(30));
  }

  #[test]
  fn damage_multiples_match_the_book() {
    assert_eq!(damage_multiple(MountClass::Turret), 1);
    assert_eq!(damage_multiple(MountClass::Fixed), 1);
    assert_eq!(damage_multiple(MountClass::Barbette), 3);
    assert_eq!(damage_multiple(MountClass::SmallBay), 10);
    assert_eq!(damage_multiple(MountClass::MediumBay), 20);
    assert_eq!(damage_multiple(MountClass::LargeBay), 100);
  }

  /// `MountClass` must agree with the real mount it came from.
  #[test]
  fn mount_class_collapses_turret_size() {
    assert_eq!(MountClass::from(&WeaponMount::Turret), MountClass::Turret);
    assert_eq!(MountClass::from(&WeaponMount::Bay(BaySize::Large)), MountClass::LargeBay);
    assert_eq!(MountClass::from(&WeaponMount::FixedMount), MountClass::Fixed);
  }

  /// The mount-legality matrix the frontend uses to decide which mounts a
  /// weapon may be put in.  It is generated from [`weapon_profile`] rather than
  /// hand-maintained in TypeScript, because a silent divergence would let the
  /// editor offer a torpedo turret that the server then refuses to fire.
  ///
  /// Run with `UPDATE_WEAPON_MOUNTS=1` to rewrite the checked-in file after
  /// changing the table.
  #[test]
  fn frontend_mount_matrix_is_current() {
    use std::fmt::Write as _;

    const PATH: &str = "../fe/callisto/src/lib/weaponMounts.json";

    let name = |mount: MountClass| match mount {
      MountClass::Turret => "Turret",
      MountClass::Fixed => "Fixed",
      MountClass::Barbette => "Barbette",
      MountClass::SmallBay => "SmallBay",
      MountClass::MediumBay => "MediumBay",
      MountClass::LargeBay => "LargeBay",
      MountClass::Battery => "Battery",
    };

    let mut generated = String::from("{\n");
    let weapons: Vec<WeaponType> = all_weapons().collect();
    for (index, kind) in weapons.iter().enumerate() {
      let legal = all_mounts()
        .filter(|mount| weapon_profile(*kind, *mount).is_some())
        .map(|mount| format!("\"{}\"", name(mount)))
        .collect::<Vec<_>>()
        .join(", ");
      let comma = if index + 1 == weapons.len() { "" } else { "," };
      writeln!(generated, "  \"{kind:?}\": [{legal}]{comma}").unwrap();
    }
    generated.push_str("}\n");

    if std::env::var("UPDATE_WEAPON_MOUNTS").is_ok() {
      std::fs::write(PATH, &generated).expect("could not write the frontend matrix");
      return;
    }

    let on_disk = std::fs::read_to_string(PATH).unwrap_or_default();
    assert_eq!(
      on_disk, generated,
      "{PATH} is stale.  Regenerate it with:\n    UPDATE_WEAPON_MOUNTS=1 cargo test frontend_mount_matrix_is_current"
    );
  }

  /// Every shipped design must survive a load/save round trip byte-for-byte.
  ///
  /// `Weapon` became a mount holding a list of guns so that a turret can hold
  /// different ones, but it still *writes* the older `{kind, mount, modifiers}`
  /// shape whenever every gun matches.  Every design in the library is uniform,
  /// so all 79 files must be untouched by that change -- if this fails, the
  /// compatibility layer has regressed and the library is about to be rewritten.
  #[test]
  fn shipped_designs_round_trip_unchanged() {
    let mut checked = 0;
    for entry in std::fs::read_dir("ship_templates").expect("ship_templates should be readable") {
      let path = entry.expect("readable directory entry").path();
      if path.extension().is_none_or(|ext| ext != "json") {
        continue;
      }
      let body = std::fs::read_to_string(&path).expect("design should be readable");
      let parsed: serde_json::Value =
        serde_json::from_str(&body).unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()));

      // Round trip only the weapons, which is what this change touches.
      let Some(weapons) = parsed.get("weapons") else {
        continue;
      };
      let loaded: Vec<crate::ship::Weapon> =
        serde_json::from_value(weapons.clone()).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
      let written = serde_json::to_value(&loaded).expect("weapons should serialize");
      assert_eq!(
        &written,
        weapons,
        "{} does not round trip; the weapon compatibility layer has regressed",
        path.display()
      );
      checked += 1;
    }
    assert!(checked > 0, "no designs were checked, so this test proves nothing");
  }

  /// Every weapon in the shipped design library must be one the rules allow.
  ///
  /// Designs are hand-edited JSON, so this is the guard against a typo or a
  /// half-finished migration leaving a ship carrying something that cannot be
  /// fired -- `attack()` refuses an illegal pairing at runtime, which would
  /// silently disarm the ship mid-game rather than failing loudly here.
  #[test]
  fn shipped_designs_use_legal_mounts() {
    use crate::ship::Weapon;

    #[derive(serde::Deserialize)]
    struct JustWeapons {
      #[serde(default)]
      weapons: Vec<Weapon>,
    }

    let mut checked = 0;
    let mut problems = Vec::new();
    for entry in std::fs::read_dir("ship_templates").expect("ship_templates should be readable") {
      let path = entry.expect("readable directory entry").path();
      if path.extension().is_none_or(|ext| ext != "json") {
        continue;
      }
      let body = std::fs::read_to_string(&path).expect("design should be readable");
      let design: JustWeapons =
        serde_json::from_str(&body).unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()));
      for weapon in &design.weapons {
        checked += 1;
        // Every gun in the mount has to be one the rules allow there.
        for gun in &weapon.guns {
          if profile_for(gun.kind, &weapon.mount).is_none() {
            problems.push(format!("{}: {}", path.display(), String::from(weapon)));
          }
        }
      }
    }

    assert!(
      problems.is_empty(),
      "designs carry weapons the rules do not allow:\n  {}",
      problems.join("\n  ")
    );
    assert!(checked > 0, "no weapons were checked, so this test proves nothing");
  }

  /// The TL bonus is awarded to the better-teched observer only, and never
  /// turns into a penalty when facing a higher-TL ship without stealth.
  #[test]
  fn tl_bonus_favours_the_observer_and_never_penalises() {
    // High Guard's worked example: a TL15 ship detecting a TL12 ship.
    assert_eq!(detection_modifiers(15, 12, None), 3);
    // Equal tech, nothing to award.
    assert_eq!(detection_modifiers(12, 12, None), 0);
    // Observer is three TLs behind, but a plain hull carries no TL penalty.
    assert_eq!(detection_modifiers(12, 15, None), 0);
  }

  /// Stealth grades come straight off the Stealth Types table (High Guard
  /// p. 14): Basic and Improved both -2, Enhanced -4, Advanced -6.
  #[test]
  fn stealth_grades_match_the_table() {
    for (grade, expected) in [
      (Stealth::Basic, -2),
      (Stealth::Improved, -2),
      (Stealth::Enhanced, -4),
      (Stealth::Advanced, -6),
    ] {
      assert_eq!(
        detection_modifiers(12, 12, Some(grade)),
        expected,
        "grade {grade:?} should apply DM{expected}"
      );
    }
  }

  /// "An additional DM-1 for every Tech Level the ship is higher than the
  /// sensors trying to locate it" - stealth only, and only in that direction.
  #[test]
  fn stealth_adds_a_tl_penalty_only_when_the_target_is_ahead() {
    // Target three TLs ahead: -6 grade, -3 TL.
    assert_eq!(detection_modifiers(12, 15, Some(Stealth::Advanced)), -9);
    // Level pegging: grade only.
    assert_eq!(detection_modifiers(15, 15, Some(Stealth::Advanced)), -6);
  }

  /// The old implementation clamped the whole term with `.min(0)`, so a big
  /// tech advantage could never overcome a stealth coating. It should.
  #[test]
  fn a_large_tl_advantage_can_beat_stealth() {
    // TL15 observer against a TL8 Basic-stealth hull: +7 TL, -2 grade.
    assert_eq!(detection_modifiers(15, 8, Some(Stealth::Basic)), 5);
    // The two TL terms never both fire, so this stays a plain sum.
    assert_eq!(detection_modifiers(14, 12, Some(Stealth::Enhanced)), -2);
  }

  /// A silent, drifting, undamaged ship with its plant down: the baseline every
  /// row below is measured against.
  fn silent() -> Emissions {
    Emissions {
      active_sensors: false,
      thrust_g: 0,
      power_plant: false,
      fired_weapons: false,
      crit_severity: 0,
      transmitting: false,
    }
  }

  /// The unified emissions table. Each row on its own, then stacking.
  #[test]
  fn emissions_rows_match_the_tables() {
    assert_eq!(silent().detection_dm(), 0, "nothing to notice");

    for (label, e, expected) in [
      (
        "active sensors",
        Emissions {
          active_sensors: true,
          ..silent()
        },
        2,
      ),
      // The book's example: "A target ship applying Thrust 3 provides DM+3".
      (
        "+1 per Thrust",
        Emissions {
          thrust_g: 3,
          ..silent()
        },
        3,
      ),
      (
        "power plant",
        Emissions {
          power_plant: true,
          ..silent()
        },
        1,
      ),
      (
        "firing gives you away",
        Emissions {
          fired_weapons: true,
          ..silent()
        },
        2,
      ),
      (
        "+1 per Severity",
        Emissions {
          crit_severity: 3,
          ..silent()
        },
        3,
      ),
      (
        "transponder or comms",
        Emissions {
          transmitting: true,
          ..silent()
        },
        6,
      ),
    ] {
      assert_eq!(e.detection_dm(), expected, "{label}");
    }
  }

  /// Rows stack: a ship doing two loud things is easier to find than one doing
  /// a single loud thing. This is the case the two-table split obscured, since
  /// firing only ever appeared on the reacquisition table.
  #[test]
  fn emission_rows_stack() {
    let lit_and_firing = Emissions {
      active_sensors: true,
      fired_weapons: true,
      ..silent()
    };
    assert_eq!(lit_and_firing.detection_dm(), 4, "+2 and +2, not +2");

    // Running dark while shooting still gives something away, which is what
    // stops a stealth ship firing from total concealment indefinitely.
    let dark_firing_running = Emissions {
      thrust_g: 3,
      power_plant: true,
      fired_weapons: true,
      ..silent()
    };
    assert_eq!(dark_firing_running.detection_dm(), 6);

    // Everything at once, with and without the comms row.
    let loud = Emissions {
      active_sensors: true,
      thrust_g: 6,
      power_plant: true,
      fired_weapons: true,
      crit_severity: 2,
      transmitting: true,
    };
    assert_eq!(loud.detection_dm(), 19);
    assert_eq!(
      Emissions {
        transmitting: false,
        ..loud
      }
      .detection_dm(),
      13
    );
  }
}
