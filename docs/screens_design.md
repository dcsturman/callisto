# Screens and Repulsors

Status: **design proposal, nothing implemented.**
Date: 2026-09-07, revised 2026-09-08 with decisions on allocation, nuclear
warheads, black globes and tractor beams.
Rules source: *Mongoose Traveller High Guard*, April 2024 update, pp. 40–41 (screens)
and p. 33 (repulsor bays). Page numbers are printed page numbers.

Covers TODO 4 in `callisto/modified designs.md`, and finishes the repulsor left
inert by TODO 1.

---

## 1. The rules

### 1.1 The Angle Screens reaction — p. 40

> **ANGLE SCREENS (GUNNER)** Using a screen, a gunner can attempt to deflect or
> reduce damage from incoming attacks. The gunner must succeed at a Gunner
> (screen) check against an attack and, if successful, reduces the damage of the
> attack – **after armour has been accounted for** – by the number of dice rolled
> by the screen (as noted in its description), **multiplied by the Effect** of the
> gunner's check.
>
> A gunner may use **any number of screens against a single attack**, combining
> their dice (but only multiplying the result by the Effect once). A gunner may
> **only attempt to Angle Screens once per round** and each screen can only be
> used once.

### 1.2 The two screens we need

| Screen | TL | Power | Tons | Cost | Effect |
| --- | --- | --- | --- | --- | --- |
| Meson Screen | 13 | 30 | 10 | MCr20 | Reduces meson damage by **2D × 10**, removes Radiation |
| Nuclear Damper | 12 | 20 | 10 | MCr10 | Reduces fusion / nuclear-warhead damage by **2D**, removes Radiation |

Meson screens carry an extra note: *"When used to ward off a single attack,
screens are grouped in batteries, requiring only a single Gunner (screens) check
for an unlimited number of screens."* That is consistent with the general
reaction — one check, many screens.

Nuclear dampers also have a Destructive-weapon clause (*"every five nuclear
dampers reduce damage by 1DD"*). Callisto has no Destructive trait and no DD
damage scale, so that clause has nothing to attach to.

**Nuclear warheads are deferred** (decided 2026-09-08): missile and torpedo
warhead variants are not modelled, so a damper's anti-warhead half has nothing to
defend against. Dampers still work against **fusion weapons**, which we do have,
so they are useful but well short of their book value. Revisit if warhead types
land.

### 1.3 Repulsor bays — p. 33

> When used as a repulsor, a successful Gunner (capital) check **removes a number
> of missiles from any salvo within range equal to 1D × Effect**. Medium repulsor
> bays multiply the result by two and large repulsor bays multiply it by five.
> […] A repulsor can only be used **once per round**.

Repulsors also work as tractor beams — holding ships up to 100/200/800 tons,
stacking, broken by an opposed Pilot check. That is a manoeuvre mechanic, not a
combat one.

### 1.4 Black globe generator — p. 41

Absorbs **all** incoming energy automatically, regardless of type. While active
the ship *"cannot manoeuvre, dodge, jump or use weapons or sensors"*, needs
capacitor capacity or it overloads, and is left vulnerable (DM+2) when switched
off if its vector was tracked.

---

## 2. Where the book is ambiguous

**(a) Does Effect multiply the meson screen's ×10?** The screen's description
says *"reduces the damage of a meson weapon by 2D × 10"*; the general reaction
says reduce by the screen's dice *"multiplied by the Effect"*. Read together that
is `2D × 10 × Effect`. **Reading adopted: yes, Effect multiplies.** The ×10 is
part of the screen's stated dice, and meson bays deal 5D–6D against multiples of
10–100, so a screen without the ×10 would be nearly useless.

**(b) What happens on Effect 0?** A marginal success multiplies the dice by
zero, reducing damage by nothing. That is literal but makes a successful check
worthless. **Reading adopted: literal (a bare success reduces nothing).** Noted
because our point-defence code takes the opposite choice (`max(effect, 1)`), and
the inconsistency should be a deliberate decision rather than an accident.

**(c) Which attack does a screen defend?** The rules assume a referee choosing in
the moment. Callisto resolves attacks in sequence with nobody to ask.
**Decided (2026-09-08): greedy, in resolution order.** Apply relevant screens to
an attack until its damage reaches zero, then carry the next screen to the next
attack. See §4.2 for why this loses almost nothing.

**(d) Do screens work against the Radiation trait alone?** Both screens "remove
the Radiation trait". Callisto tracks `radiation` on the profile but implements
no radiation effect at all, so there is nothing to remove today.

---

## 3. What this affects in our library

| Design | Screens in the book | In our JSON |
| --- | --- | --- |
| Colonial Cruiser - Kinunir (p215) | Nuclear Dampers ×5, Black Globe Generator | absent |
| Fleet Escort - P.F. Sloan (p232) | Meson Screens ×2, Nuclear Dampers ×2 | absent |
| Midu Agasham (p228) | Meson Screens ×2, Nuclear Dampers ×4 | absent |

**`modified designs.md` TODO 4 is incomplete**: it records only the Kinunir and
the P.F. Sloan. The Midu Agasham's four nuclear dampers and two meson screens
were never written down. Worth fixing in the TODO regardless of when this ships.

Note none of these three ships currently faces a meson gun or a fusion weapon in
any shipped scenario, so screens change nothing at the table until someone builds
that fight — unlike point-defence batteries, which were wrong in every missile
engagement.

---

## 4. Proposal

### 4.1 Schema — screens are not weapons

Screens do not belong in `Ship::weapons()`. They have no mount, take no
hardpoint, never fire, and cannot be aimed. Putting them there would repeat the
mistake the point-defence stopgap made.

Add a parallel list on the design and the ship:

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenType {
  Meson,
  NuclearDamper,
}

// ShipDesignTemplate, beside `weapons`
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub screens: Vec<ScreenType>,
```

`Vec<ScreenType>` rather than a count map, so a future crit can disable one
screen the way `active_weapons` disables one weapon.

Rejected alternatives:

- **A `WeaponType::MesonScreen` in a `WeaponMount::Screen`.** This is the shape
  point-defence batteries used, and it worked there because a battery genuinely
  consumes a Hardpoint and is genuinely destructible hardware sitting in the
  weapon list. A screen consumes no hardpoint, so it would corrupt the editor's
  allowance arithmetic (`hardpoints.ts::mountCost` returns 1 for anything
  unrecognised).
- **A bare count (`meson_screens: u8`).** Cheaper, but gives crits nothing to
  target and makes a third screen type another field.

### 4.2 Combat integration

The insertion point already exists. In `combat.rs::attack()`:

```rust
let effective_armor = defender.get_current_armor().saturating_sub(u32::from(profile.ap));
damage = if damage > effective_armor { damage - effective_armor } else { ... };
//  <-- screens apply exactly here, "after armour has been accounted for"
```

**One screen reaction per ship per round**, mirroring the book's once-per-round
limit, rolled the way point defence now is: one Gunner (screen) check per ship at
the start of resolution, giving one Effect that every screen on that ship
multiplies by.

**Allocation is greedy, in resolution order** (decided 2026-09-08). Each screen
is spent whole on the current attack; when that attack's damage reaches zero the
next screen moves to the next attack. Screens are type-gated throughout — meson
screens only reduce meson weapons, dampers only fusion and nuclear warheads.

This is not RAW. The book has one gunner combine *all* screens into a single
attack; this spreads the same total reduction across several. It is the closest
approximation available given that Callisto has no salvoes and resolves attacks
sequentially with nobody to consult, and it is arguably what several gunners
screening independently would look like anyway.

**What the deviation costs, with numbers.** A screen spent on an attack smaller
than its own reduction wastes the remainder, so allocation order matters only
where overkill is possible. Against the weapons these screens exist to stop, it
mostly is not:

| Attack | Damage | Screen reduction at Effect 2 |
| --- | --- | --- |
| Meson small bay | 5D × 10 ≈ 175 | 2D × 10 × 2 ≈ 140 |
| Meson medium bay | 6D × 20 ≈ 420 | ≈ 140 |
| Meson large bay | 6D × 100 ≈ 2,100 | ≈ 140 |
| Fusion barbette | 5D × 3 ≈ 52 | damper, 2D × 2 ≈ 14 |
| **Fusion turret** | **4D × 1 ≈ 14** | **damper, ≈ 14** |

Meson screens are weaker than every meson weapon they face, so they never
overkill and order is irrelevant. The one exception is a nuclear damper against a
fusion *turret*, where the reduction and the damage are about equal — a ship
firing several fusion turrets at the Kinunir could see a damper or two slightly
wasted.

Fixing that would mean draining largest-attack-first, which requires batching all
attacks against a defender before applying any damage — a real change to
`attack()`, which today resolves one attack at a time. Not worth it for a case
this narrow. Recorded here so the limitation is known rather than discovered.

### 4.3 Repulsors — reuse the point-defence pool

Repulsors are not screens and should not use this machinery. *"Removes a number
of missiles from any salvo within range equal to 1D × Effect"*, once per round,
is **the same shape as a point-defence battery** — and we already have that pool.

```rust
// combat.rs, beside roll_battery_pool
fn repulsor_missiles_removed(mount: MountClass, effect: u32, rng: &mut dyn RngCore) -> u32 {
  let multiplier = match mount {
    MountClass::SmallBay => 1,
    MountClass::MediumBay => 2,
    MountClass::LargeBay => 5,
    _ => return 0,
  };
  u32::from(roll_dice(1, rng)) * effect * multiplier
}
```

Rolled once per round per repulsor and added to `point_defense_pool`. Unlike a
battery it takes a *check*, so it can fail outright — use the weapon's gunnery as
the Gunner (capital) skill, matching how point-defence tallies already work.

This is a small change that makes `WeaponType::Repulsor` stop being inert, and it
needs no new schema at all. **Recommend doing this first**, separately from
screens.

**Tractor beams: set aside (decided 2026-09-08).** Holding a ship, stacking bays, opposed Pilot
checks to break free, and a held ship moving at the operator's Thrust 1 — that is
a manoeuvre subsystem touching flight plans, not a combat one. Worth its own
design if wanted.

### 4.4 Black globe generator — set aside (decided 2026-09-08)

It is a mode, not a modifier: while active the ship cannot manoeuvre, dodge,
jump, shoot or use sensors, and it needs a capacitor model to decide when it
overloads. That is four new interactions with systems we do model, plus one
(capacitors) we do not. Only the Kinunir carries one.

Recommend recording it as a known omission on the Kinunir and leaving it. If it
is ever built, it should be a ship *state* toggled by a crew action, not a screen.

---

## 5. Frontend

Much smaller than the point-defence work, because screens never enter the action
UI — there is no button, no target, no mount icon.

| File | Change |
| --- | --- |
| `lib/shipDesignTemplates.ts` | Add `screens?: string[]` to the design type. |
| Ship summary / design tooltip | List screens as text, e.g. "Meson Screen ×2". |
| `components/controls/AddShip.tsx` | Only if the referee should be able to fit screens. Not required to render existing designs. |

The design tooltip is the one place a referee would notice their absence, so that
is the minimum useful frontend change.

---

## 6. Suggested phasing

1. **Repulsors into the point-defence pool.** Self-contained, no schema, makes an
   existing weapon work. Do this first and separately.
2. **`ScreenType` + design/ship plumbing + JSON migration** for the three ships,
   with screens rendering as text. No combat effect yet — so nothing can regress.
3. **The Angle Screens reaction** in `attack()`, greedy in resolution order.

Steps 1 and 2 are each small and independently useful. Step 3 is where the real
rules risk sits.

Set aside for now: black globe generators, tractor beams, nuclear warheads.

---

## 7. Open question

**Should a bare success (Effect 0) reduce nothing?** The rule is an explicit
multiplication — *"multiplied by the Effect"* — so Effect 0 literally reduces
damage by zero, and a successful check accomplishes nothing.

Our point-defence code takes the opposite view for its own checks
(`effect.max(1)`), on the logic that a successful interception at least stops the
missile it was aimed at.

**Recommendation: keep the literal reading for screens.** Point defence destroys
a discrete object, so "at least one" has physical meaning; a screen only scales
damage down, and scaling by zero is a coherent outcome the rule clearly permits.
The inconsistency between the two is then deliberate rather than accidental —
which is the part that matters.
