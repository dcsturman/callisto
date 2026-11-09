# Screens and Repulsors

Status: **implemented** for meson screens and nuclear dampers (`ScreenType`, `roll_screen_pool`,
`Ship::angle_screens`), angled by screen gunners each round; black globe generators and the
tractor-beam half of repulsors are not. Kept as the design record; the FAQ has the rulings as played.
Date: 2026-09-07, revised 2026-09-08 (allocation, nuclear warheads, black globes, tractor beams) and
2026-10-29 (rewritten to the High Guard 2022 FAQ errata: screens after the Damage Multiple, one attack
per screen a round).
Rules source: *Mongoose Traveller High Guard*, April 2024 update, pp. 40–41 (screens), p. 35 (spinal
mounts) and p. 33 (repulsor bays), **as corrected by the *High Guard Update 2022 FAQ*, p. 2**, which
replaces the screens text. Page numbers are printed page numbers.

---

## 1. The rules

### 1.1 Screens, as the errata has them — HG 2022 FAQ p. 2

The errata replaces both paragraphs of "Screens (page 40)":

> Screens are directed defensive systems that are designed to defeat specific attacks. For example,
> meson screens prevent meson decay and are targeted against meson weapon attacks, while nuclear
> dampers are used against nuclear warheads.
>
> During the Reactions phase of space combat, gunners must angle screens to determine their effect.
> **First, calculate the damage of the opposing weapon, subtract Armour, and apply the Damage
> Multiple.**
>
> **Make a Gunner (screen) check for each screen.** Every successful check reduces the damage of the
> attack by the amount indicated in the Damage Reduction column of the Screens table **multiplied by
> the Effect** of the check. **A gunner may only angle screens against one attack per round.**

It also **deletes** the p. 41 "Angle Screens (Gunner)" box (one check, any number of screens, Effect
applied once) and the Meson Screen sentence about grouping screens in batteries under a single check,
and replaces the table:

| Screen | TL | Power | Tons | Damage Reduction | Cost |
| --- | --- | --- | --- | --- | --- |
| Meson Screen | 13 | 30 | 10 | 2D × 10 | MCr20 |
| Nuclear Damper | 12 | 20 | 10 | 2D | MCr10 |

The spinal mount text agrees on the order (HG p. 35): after the ×1,000, "Apply damage reduction from
any meson screens as normal."

What changed from the book as first printed:

- **Order.** Screens used to come "after armour", before the Damage Multiple. They now come after
  the multiple. Against a meson bay (×10, ×20, ×100) or a spinal (×1,000), a screen meets the
  multiplied damage, so it is worth a tenth to a thousandth of what it was.
- **Checks.** One check per screen, each with its own Effect, instead of one check for a group.
- **Attacks.** Each gunner angles at one attack a round.

Nuclear dampers' Destructive clause ("every five nuclear dampers reduce damage by 1DD") is untouched by
the errata. Callisto has no Destructive trait and no DD scale, so it has nothing to attach to.

**Nuclear warheads are deferred** (decided 2026-09-08): missile and torpedo warhead variants are not
modelled, so a damper's anti-warhead half has nothing to defend against. Dampers work against
**fusion weapons**, which we have.

### 1.2 Repulsor bays — p. 33

> When used as a repulsor, a successful Gunner (capital) check **removes a number of missiles from any
> salvo within range equal to 1D × Effect**. Medium repulsor bays multiply the result by two and large
> repulsor bays multiply it by five. […] A repulsor can only be used **once per round**.

### 1.3 Black globe generator — p. 41

Absorbs all incoming energy automatically. While active the ship *"cannot manoeuvre, dodge, jump or
use weapons or sensors"*, needs capacitors or it overloads. Set aside (§5).

---

## 2. Rulings

**(a) Effect multiplies the meson screen's ×10.** The table's reduction is "2D × 10" and the errata
multiplies "the amount indicated in the Damage Reduction column" by the Effect: `2D × 10 × Effect`.
The errata settles what was an ambiguity in the first printing.

**(b) Effect floors at 1 where it multiplies** (decided 2026-09-08, a general project rule in
`callisto/FAQ.md`). A bare success reduces `2D × 10` or `2D`. A failed check reduces nothing.

**(c) Every screen has its own gunner and so its own check** (`Crew::screen_gunnery`, index-aligned
with the design's `screens`). The errata now says this outright: "a Gunner (screen) check for each
screen". A screen nobody is on is not angled at all, unless Screen Optimiser is running, which angles
every screen at DM+0 (HG p. 75).

**(d) Each screen is angled at exactly one attack a round** (2026-10-29). "A gunner may only angle
screens against one attack per round", and each screen is its gunner's. Its whole reduction applies
to that attack and nothing else: what the attack did not need is lost, not carried to another hit.

**(e) Which attack: the largest not yet screened** (2026-10-29). Nobody is asked; it is automatic, like
the extra Jam Missiles stations. Once every hit a ship takes in the round is known, its screens are
taken in order and each goes on the largest hit it defends against that has no screen on it yet.

**(f) More screens than hits: several on one** (2026-10-29). When every hit it can meet already has a
screen, a screen goes on whichever has the most damage left after the screens already on it. Several
screens on one attack is what several gunners each angling at the same attack would do; the errata's
"one attack per round" limits the gunner, not the attack. So nine meson screens against a lone spinal
hit all go on it, and take their nine rolls off its ×1,000 damage.

**(g) A failed check is still angled.** The gunner committed the screen before the check, so the
screen is spent on that attack and takes nothing off it.

**(h) Do screens work against the Radiation trait alone?** Both screens "remove the Radiation trait".
Radiation is tracked on the profile (crew wear, HG p. 31) but screens do not yet strip it; left as is.

---

## 3. What it comes to

Typical damage against a screen's roll at Effect 2 (2D averages 7):

| Attack | Damage after the multiple | Screen at Effect 2 |
| --- | --- | --- |
| Meson small bay | (5D + Effect) × 10 ≈ 200 | meson screen ≈ 140 |
| Meson medium bay | (6D + Effect) × 20 ≈ 460 | ≈ 140 |
| Meson large bay | (6D + Effect) × 100 ≈ 2,300 | ≈ 140 |
| Meson spinal, 3× | (18D + Effect) × 1,000 ≈ 65,000 | ≈ 140 |
| Fusion barbette | (5D + Effect − armour) × 3 | damper ≈ 14 |
| Fusion small bay | (6D + Effect − armour) × 10 | damper ≈ 14 |
| Fusion turret | 4D + Effect − armour | damper ≈ 14 |

Before the errata a meson screen came off a spinal's dice: nine screens took ~1,260 off 18D + Effect
and stopped every hit outright. Now they take ~1,260 off ~65,000.

---

## 4. Implementation

### 4.1 Schema — screens are not weapons

```rust
pub enum ScreenType { Meson, NuclearDamper }
// ShipDesignTemplate, beside `weapons`
pub screens: Vec<ScreenType>,
// Crew, beside `gunnery`
screen_gunnery: Vec<u8>,
```

Screens have no mount, take no hardpoint, never fire and cannot be aimed, so they are a list of their
own rather than entries in `Ship::weapons()`. `Vec<ScreenType>` rather than a count so a crit can one
day disable one screen.

### 4.2 Combat

- **The roll.** At the start of fire resolution every ship rolls its screens (`roll_screen_pool`):
  each crewed screen's Gunner (screen) check and, on a success, `2D × factor × Effect`. The result is
  `Ship::screen_pool: Vec<Option<u32>>`, `None` for a screen nobody angles.
- **The hit.** `combat::aim` works an attack out as far as damage, armour, the turret bonus and the
  Damage Multiple, and returns a `Landing`. `combat::land` takes the screens off it, then does
  everything else: the line, ion drain, criticals (a spinal hit screened to nothing does not crit),
  hull and radiation.
- **The round.** `Entities::fire_actions` runs every attacker's fire (`aim_fire_actions`) before any
  hit a screen could meet lands. A hit no screen of the defender's can meet lands as it is fired, so
  the dice of fights without screens fall exactly as before. Then `land_fire` gives each defender
  every hit waiting on it, `Ship::angle_screens` assigns its screens by §2(d)–(g), and each volley is
  landed and reported in the order it was fired.
- **Outside the round.** `attack()` and `volley()`, which tests and missiles use, angle the defender's
  remaining screens at the hits they make, then land them.
- **The line.** The hover shows the screens after the multiple:
  `... x1000 mount = 64000, -1260 screens (9 screens) = 62740`.

Screens are type-gated throughout: meson screens only meson weapons, dampers only fusion weapons.

### 4.3 The AI

`callisto::tactics` does not price screens, so it needed no change. The side network's features carry
no screen arithmetic.

### 4.4 Repulsors — in the point-defence pool

Rolled once per round per repulsor (`roll_repulsor`), `1D × Effect × 1/2/5` by bay size on a Gunner
(capital) check, and added to the ship's point-defence pool, the same pool batteries feed.

---

## 5. Set aside

- **Tractor beams** (repulsors' other half): a manoeuvre subsystem, not a combat one.
- **Black globe generators**: a mode, not a modifier — no manoeuvre, dodge, jump, fire or sensors
  while on, and a capacitor model to say when it overloads. Only the Kinunir carries one; recorded as a
  known omission on her.
- **Nuclear warheads**, and dampers' Destructive clause.

---

## 6. Frontend

Screens never enter the action UI. The design tooltip and Add Ship list them ("Meson Screen ×2"), and
the screen gunners' skills are set in Add Ship beside the weapon gunners'. Nothing in the frontend
mirrors screen arithmetic; the results hover shows what the backend did.
