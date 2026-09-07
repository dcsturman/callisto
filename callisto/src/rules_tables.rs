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
    MountClass::Turret | MountClass::Fixed => 1,
    MountClass::Barbette => 3,
    MountClass::SmallBay => 10,
    MountClass::MediumBay => 20,
    MountClass::LargeBay => 100,
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
  use MountClass::{Barbette, Fixed, LargeBay, MediumBay, SmallBay, Turret};
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
    },
    WeaponType::Fusion => match mount {
      Turret | Fixed => P::gun(14, 4, Range::Medium).rad(),
      Barbette => P::gun(12, 5, Range::Medium).ap(3).rad(),
      SmallBay => P::gun(12, 6, Range::Medium).ap(6).rad(),
      MediumBay => P::gun(12, 7, Range::Medium).ap(6).rad(),
      LargeBay => P::gun(12, 10, Range::Long).ap(8).rad(),
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

  const ALL_MOUNTS: [MountClass; 6] = [
    MountClass::Turret,
    MountClass::Fixed,
    MountClass::Barbette,
    MountClass::SmallBay,
    MountClass::MediumBay,
    MountClass::LargeBay,
  ];

  const ALL_WEAPONS: [WeaponType; 12] = [
    WeaponType::Beam,
    WeaponType::Pulse,
    WeaponType::Missile,
    WeaponType::Sand,
    WeaponType::Particle,
    WeaponType::Torpedo,
    WeaponType::Fusion,
    WeaponType::Plasma,
    WeaponType::Railgun,
    WeaponType::Meson,
    WeaponType::MassDriver,
    WeaponType::Repulsor,
  ];

  /// A weapon scales by damage multiple *or* by salvo size, never both and
  /// never neither-when-it-should.  This is the invariant that keeps bay
  /// missiles from being counted twice.
  #[test]
  fn multiple_and_salvo_are_mutually_exclusive() {
    for kind in ALL_WEAPONS {
      for mount in ALL_MOUNTS {
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
    for kind in ALL_WEAPONS {
      for mount in ALL_MOUNTS {
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
    for size in 1..=3 {
      assert_eq!(MountClass::from(&WeaponMount::Turret(size)), MountClass::Turret);
    }
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
    };

    let mut generated = String::from("{\n");
    for (index, kind) in ALL_WEAPONS.iter().enumerate() {
      let legal = ALL_MOUNTS
        .iter()
        .filter(|mount| weapon_profile(*kind, **mount).is_some())
        .map(|mount| format!("\"{}\"", name(*mount)))
        .collect::<Vec<_>>()
        .join(", ");
      let comma = if index + 1 == ALL_WEAPONS.len() { "" } else { "," };
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
        if profile_for(weapon.kind, &weapon.mount).is_none() {
          problems.push(format!("{}: {}", path.display(), String::from(weapon)));
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
}
