# Point Defence Batteries

Status: **design only, nothing implemented.**
Date: 2026-09-07.
Rules source: *Mongoose Traveller High Guard*, April 2024 update
(`/Users/dan/My Drive/Traveller/MGT/High Guard Apr 2024.pdf`) and
*MgT2 Core Rulebook* 2022 update. All page numbers below are **printed** page
numbers, verified against the High Guard index (p. 288: "Point-Defence Weapons 40",
"Point Defence 104").

Scope: `callisto/src/ship.rs`, `callisto/src/combat.rs`, `callisto/src/entity.rs`,
`fe/callisto/src/lib/weapon.ts`, `fe/callisto/src/lib/hardpoints.ts`,
`fe/callisto/src/components/controls/WeaponUse.tsx`, and four files in
`callisto/ship_templates/`.

Supersedes TODO 1's point-defence rows in `callisto/modified designs.md`.

---

## 1. The rules

### 1.1 The primary text — High Guard p. 40

The whole of the tactical rule is one sidebar, "POINT-DEFENCE WEAPONS", in the
*Weapons and Screens* chapter. Quoted in full because every design decision below
turns on its exact wording:

> A point-defence laser battery consists of linked short-ranged laser turrets
> controlled by their own automated computer. This removes the need for separate
> gunners dedicated to point defence, needing only a command from the bridge to
> activate when an incoming attack is detected.
>
> A point-defence battery automatically intercepts missile and torpedo salvoes
> just before they make their own attack rolls. A point-defence battery reduces
> the number of missiles attacking a ship each turn by its Intercept score. This
> can be applied to any salvo or spread between several salvoes. A point defence
> battery uses 1 Hardpoint.

**Point Defence Laser Batteries** (p. 40):

| Weapon | TL | Intercept | Power | Tons | Cost |
| --- | --- | --- | --- | --- | --- |
| Type I | 10 | +2D | 10 | 20 | MCr5 |
| Type II | 12 | +4D | 20 | 20 | MCr10 |
| Type III | 14 | +6D | 30 | 20 | MCr20 |

**Point Defence Gauss Batteries** (p. 40) exist too — same 20 tons, same +2D/+4D/+6D,
Power 5/15/25, MCr3/6/10. They differ only in that they are tuned against torpedoes:
equal protection against missiles and torpedoes of Thrust 10 or less, **DM-2** against
Thrust 12–14, **DM-6** against Thrust 15+, and they need ammunition (12 rounds per
firing, one ton per canister at Cr30000, 12 more rounds each). Callisto has no
torpedoes and no missile Thrust rating, so gauss batteries are **out of scope** — but
see §3.3, the schema is chosen so they drop in later without a second redesign.

### 1.2 Everywhere else point defence appears

| Rule | Page | Bearing on this work |
| --- | --- | --- |
| Point Defence (Gunner) reaction | Core Rulebook p. 171 | The baseline: turret beam/pulse laser, Gunner (turret) check, Effect = missiles removed, DM+1 double / DM+2 triple, **once per round per gunner**, and a weapon used for PD cannot attack that round. This is what Callisto already implements. |
| Missile countermeasures order | Core Rulebook pp. 172–173 | EW first, then sand, then point defence "just as a salvo is about to strike". |
| Beam laser barbette | HG p. 29 | "cannot be used for point defence, as with any other barbette" |
| Pulse laser barbette | HG p. 31 | same wording |
| Quad turrets | HG p. 81 | "Quad turrets provide DM+3 on point defence." Not implemented today; `WeaponMount::Turret(4)` panics in `ship.rs:1201-1203`. Out of scope, noted. |
| Point Defence software /1, /2 | HG p. 74 | Lets a ship "use point defence batteries and the Point Defence (gunner) reaction to defend **any ship** within Close range" (/1) or Short range (/2). Callisto has no software model; PD is self-only. Out of scope, flagged in §8. |
| Fighters as point defence | HG p. 104 | Fighters with beam/pulse lasers use the ordinary PD *action*, not batteries. No change needed. |
| Fleet Battles salvo defence | HG pp. 112–113 | The fleet-scale abstraction. "+4 for every Type I Point Defence Battery, +8 for every Type II, +12 for every Type III", pooled per round, "Each point removes one missile from incoming salvoes", and "Against torpedoes, double the amount taken from the pool." Callisto is a tactical game, not a fleet game, so this is **corroborating evidence**, not the rule to implement — see §1.4(a). |
| Screens / Angle Screens | HG pp. 40–41 | Meson screens and nuclear dampers are a *separate* system with their own Gunner (screen) reaction. The Kinunir, Midu Agasham and P.F. Sloan all carry them and Callisto models none of it. Same category of gap; explicitly not this project. |

Sandcasters do not interact. "Disperse Sand" (CRB p. 171) is a distinct reaction
against **laser** attacks, already modelled independently in
`combat.rs::create_sand_counts` (`combat.rs:1009-1040`) and consumed in the laser
damage path at `combat.rs:891-937`.

### 1.3 Is it "a laser battery permanently set to point defence"?

**No.** The hypothesis is directionally right about intent and wrong about every
mechanic that matters. Three concrete differences:

1. **It is not a weapon.** It has no attack roll, no damage, no range band, no
   Gunner skill, and no offensive mode at all. The book never gives it a Damage
   entry — the table has no Damage column. There is nothing to "set" it to; it has
   one mode.
2. **It costs no action and no crew.** "This removes the need for separate gunners
   dedicated to point defence, needing only a command from the bridge." It
   "automatically intercepts". By contrast the CRB reaction consumes a gunner's
   reaction, is limited to once per round per gunner, and locks that turret out of
   attacking. A battery does none of that.
3. **Its output is a count, not a check.** A laser turret rolls 2D+DM vs 8 and
   removes *Effect* missiles — a fraction of a missile on average. A battery removes
   2D/4D/6D missiles outright, with no roll to succeed.

The magnitude of (3) is the reason the current stopgap is not a rounding error. See
§7 for the arithmetic: the Type II battery on the *Dragon* is currently worth
about **1.7 missiles per round**; the book says **14**.

The closest true statement is: *a point-defence battery is a passive, always-on
sink that absorbs N incoming missiles per round, where N is a die roll.* That is a
different kind of object from a weapon, which is exactly why §3 recommends
representing it as a weapon-shaped record whose combat path bypasses the weapon
machinery entirely.

### 1.4 Where the book is ambiguous

Flagged rather than smoothed over.

**(a) "+2D" is written like a DM but used like a count.** The table column is headed
"Intercept" and the values carry a `+` prefix, which in Traveller notation means a
dice modifier. But the prose says the battery "reduces the number of missiles
attacking a ship each turn by its Intercept score" — and a DM cannot reduce a
count. Two pieces of evidence say it is a roll producing a number of missiles:
the prose itself, and the Fleet Battles conversion (p. 113) which turns each
battery into a flat number of missiles removed from a pool (+4/+8/+12). **Reading
adopted: roll 2D/4D/6D, remove that many missiles.**

Note the fleet numbers do not equal the tactical averages: 2D/4D/6D average
7/14/21, but fleet scale gives 4/8/12 — about 57% in every case. The book does not
explain the discount. Consistent ratio suggests deliberate fleet-scale damping
rather than a different reading of the tactical rule, but this is an inference.

**(b) One roll per round, or one per salvo?** "reduces the number of missiles
attacking a ship **each turn**" says per-round. "This can be applied to any salvo
or spread between several salvoes" confirms a single per-round quantity the
defender allocates. **Reading adopted: one roll per battery per round, forming a
pool.** Callisto has no salvos at all — missiles are individual entities
(`entity.rs:60`) that resolve one at a time — so a per-round pool is the exact
right shape.

**(c) Can a battery be knocked out?** The book does not say. It is a 20-ton
installation occupying a Hardpoint and appears on every deck plan legend
(e.g. Dragon p. 193 legend item 11, Midu Agasham p. 228 legend item 5), so it is
physical hardware. **Reading adopted: yes** — treat it as a weapon for
critical-hit purposes, so `ShipSystem::Weapon` crits (`combat.rs:485-512`) can
disable it like any other mount.

**(d) Does it work with no crew / a destroyed bridge?** "needing only a command
from the bridge to activate". Callisto models neither crew casualties nor bridge
destruction as a gate on anything, so this is moot today. Noted for the record.

**(e) Order of countermeasures.** CRB p. 173 lists EW, then sand, then point
defence. It does not say whether an automatic battery fires before or after a
gunner's PD reaction, because the CRB predates batteries. **Reading adopted: pool
first, gunner second** — see §4.3 for why.

---

## 2. Current state in the code

### 2.1 The stopgap

`WeaponType` is `Beam | Pulse | Missile | Sand | Particle` (`ship.rs:351-358`).
`WeaponMount` is `Turret(u8) | Barbette | Bay(BaySize) | FixedMount`
(`ship.rs:334-342`). Neither can express a battery, so
`callisto/modified designs.md` records the substitution:

| Book | Encoded as | Design file |
| --- | --- | --- |
| PD Laser Battery (Type II) ×1 | `Pulse` / `Turret(2)` | `system_defence_boat_dragon.json` idx 3 |
| PD Laser Battery (Type III) ×1 | `Pulse` / `Turret(3)` | `colonial_cruiser_kinunir.json` idx 4 |
| PD Laser Battery (Type III) ×2 | `Pulse` / `Turret(3)` | `fleet_escort_p_f_sloan.json` idx 32, 33 |
| PD Laser Battery (Type III) ×2 | *omitted entirely* | `midu_agasham.json` |

Verified against the book: Dragon p. 193, Kinunir p. 215, P.F. Sloan p. 232,
Midu Agasham p. 228. All four transcriptions are otherwise correct — the
P.F. Sloan's `Beam`/`Turret(3)` ×30 at indices 2–31 matches "Triple Turrets (beam
lasers) x30" exactly, and the Midu Agasham's eight `Pulse`/`Turret(3)` at indices
7–14 are genuine pulse laser turrets, *not* batteries.

The substitution was chosen because `point_defense_score` (`combat.rs:1046-1055`)
only scores `Beam`/`Pulse` in a `Turret` and scales with turret count; barbettes,
bays and fixed mounts score zero.

### 2.2 How point defence resolves today

The whole chain, in order:

1. **Queue.** The client sends `ShipAction::PointDefenseAction { weapon_id }`
   (`action.rs:199-201`). `merge` (`action.rs:293-300`) makes a fire action and a
   PD action mutually exclusive per `weapon_id`, so a weapon does one or the
   other — matching CRB p. 171.
2. **Tally.** `Entities::fire_actions` calls `build_point_defense_tallies`
   (`entity.rs:658-659` → `combat.rs:1060-1115`), which scores every weapon as
   `point_defense_score(weapon) + gunnery(index)` for active weapons only, drops
   any queued action whose weapon scores 0, applies the captain's leadership boost
   (`action.rs:74-79`), and stores `Vec<(weapon_id, bonus)>` on the ship
   (`ship.rs:265`, `#[serde(skip)]`).
3. **Resolve.** In `Entities::update_all`, for each impacting missile
   (`entity.rs:811-836`): first drain `point_defense_memory` — surplus Effect
   carried over from a previous successful check — otherwise call
   `use_next_point_defense` (`combat.rs:1121-1143`), which pops one entry, rolls
   2D + bonus vs 8, and returns `max(effect, 1)` on success. A non-zero result
   destroys the missile and banks `result - 1` in `point_defense_memory`.
4. **Clear.** `ship.clear_point_defense()` at `entity.rs:883`, end of round.

Two pre-existing oddities, both out of scope but worth knowing before touching
this code:

- `build_point_defense_tallies` sorts the list **descending** by bonus
  (`combat.rs:1112`) and `use_next_point_defense` **pops from the back**
  (`combat.rs:1122`), so the *worst* PD weapon fires first. Either the sort or the
  pop is backwards.
- Callisto allows one PD check per *queued weapon*, whereas CRB p. 171 allows one
  per *gunner* per round. This is already a deliberate-looking deviation (there is
  no gunner-to-weapon assignment model), not something batteries make worse.

### 2.3 What a battery would score if we did nothing

Zero. `point_defense_score` returns `0` for any mount that is not a `Turret`, so a
faithful 20-ton battery encoded as, say, `Pulse`/`Barbette` contributes nothing.
That is precisely why the hack exists.

---

## 3. Schema

### 3.1 The options

| # | Shape | Verdict |
| --- | --- | --- |
| A | One new `WeaponType::PointDefense`, reuse `Turret(n)` for the grade | Rejected. Overloads turret count with a meaning it does not have, keeps batteries inside turret rules (`mountCost`, quad-turret DM, PD-action eligibility), and leaves the "20-ton thing pretending to be a turret" lie in place. |
| B | Three variants `WeaponType::PointDefenseI/II/III` | Rejected. `WeaponType` is a flat, `Ord`-derived, string-serialized enum consumed by `WEAPON_COLORS` and `WEAPON_KINDS` on the frontend; tripling it for one family scales badly the moment gauss batteries arrive (six variants). |
| C | **`WeaponType::PointDefense` + `WeaponMount::Battery(u8)`** | **Recommended.** |
| D | A separate `point_defense: Option<Vec<Battery>>` field on `ShipDesignTemplate` and `Ship` | Rejected. Needs a parallel damage model (`active_weapons` is indexed against `weapons()`), a parallel editor UI, a parallel scenario round-trip, and its own Hardpoint accounting — the editor already totals Hardpoints over the weapon list (`hardpoints.ts:110-121`), and a battery legitimately consumes one. |
| E | Turn `Weapon` into an enum: `Weapon::Gun { kind, mount } \| Weapon::PointDefenceBattery { grade }` | Rejected **for now**. It is the type-correct answer and it makes illegal states unrepresentable, but it changes the wire format of every design file and touches every `weapon.kind` / `weapon.mount` match in both languages. Revisit if screens, nuclear dampers and repulsors land — at three or four non-weapon installations the cross-product hack in C stops paying. |

### 3.2 Recommendation — option C

```rust
// ship.rs:351
pub enum WeaponType {
  Beam = 0,
  Pulse,
  Missile,
  Sand,
  Particle,
  /// A point-defence laser battery.  Never fires offensively; see
  /// `WeaponMount::Battery` for its Intercept grade.
  PointDefense,
}

// ship.rs:335
pub enum WeaponMount {
  Turret(u8),
  Barbette,
  Bay(BaySize),
  FixedMount,
  /// A 20-ton point-defence battery consuming one Hardpoint.  The `u8` is the
  /// book's Type: 1, 2 or 3 (High Guard p. 40).
  Battery(u8),
}
```

Wire shape, unchanged in kind from what already exists:

```json
{ "kind": "PointDefense", "mount": { "Battery": 3 } }
```

`WeaponType::PointDefense` must go **last** in the enum: the type derives `Ord`
(`ship.rs:351`) and the variant order is the sort order. Appending leaves every
existing comparison untouched.

Why C over the alternatives:

- **`weapon_id` semantics are preserved exactly.** It stays a plain index into
  `Ship::weapons()` (`ship.rs:566-568`). Batteries simply never appear in a
  `FireAction`, a `PointDefenseAction` or a `BoostTarget`.
- **Crit damage works for free.** `active_weapons` (`ship.rs:182`) is sized to the
  weapon list, and the `ShipSystem::Weapon` crit picks uniformly among active
  entries (`combat.rs:485-512`). A battery becomes a legitimate crit target with
  no new code — answering §1.4(c).
- **Hardpoint accounting works for free.** `mountCost` (`hardpoints.ts:67-75`)
  already returns 1 for anything that is not a Bay or a small-craft Barbette,
  which is exactly "a point defence battery uses 1 Hardpoint".
- **Migration is in-place.** Three of the four affected designs need a field
  rewritten, not an insertion — indices do not move. See §6.
- **Grade lives on the mount, not the kind**, which is what makes gauss batteries
  a one-variant change later: `WeaponType::PointDefenseGauss` reuses
  `Battery(1..=3)` verbatim.
- **`WeaponMount` already has payload variants** (`Turret(u8)`, `Bay(BaySize)`),
  so the frontend's `WeaponMount = string | {Turret: number} | {Bay: BaySize}`
  discriminated-union style (`weapon.ts:5`) extends naturally to
  `{Battery: number}`. Putting a payload on `WeaponType` instead would change
  `"kind"` from a bare string to an object and break `WEAPON_COLORS[props.weapon]`
  (`WeaponUse.tsx:60-66`) and `WEAPON_KINDS` (`hardpoints.ts:356-363`).

**The honest cost of C:** `Beam`/`Battery(2)` and `PointDefense`/`Turret(3)` are
representable and meaningless. Mitigation is a single accessor that is the only
place in the backend allowed to interpret the pair:

```rust
// combat.rs, next to point_defense_score
/// Intercept dice for a point-defence battery: 2D / 4D / 6D for Type I / II / III
/// (High Guard p. 40).  `None` for anything that is not a legal battery.
fn battery_intercept_dice(weapon: &Weapon) -> Option<u8> {
  match (weapon.kind, weapon.mount) {
    (WeaponType::PointDefense, WeaponMount::Battery(grade @ 1..=3)) => Some(2 * grade),
    _ => None,
  }
}
```

Everything else treats an illegal pair as inert and logs it.

### 3.3 Every exhaustive match that stops compiling

This is the point of the change: the compiler enumerates the work. All of these
are `match` arms with no catch-all today.

| Site | Change |
| --- | --- |
| `ship.rs:1084-1100` `Ord for Weapon` | Add `Battery` arms. Recommend batteries sort after turrets and before `FixedMount`, matching their "not a gun but real hardware" status. Extend `test_weapon_ordering` (`ship.rs:2204`). |
| `ship.rs:1184-1192` `From<&WeaponType> for String` | `PointDefense => "point defence"` |
| `ship.rs:1196-1212` `From<&Weapon> for String` | `(_, WeaponMount::Battery(n)) => format!("point defence battery (Type {})", roman(n))`. Required — this is what the crit message at `combat.rs:509` prints. |
| `ship.rs:1217-1219` `is_laser` | `PointDefense => false`. A battery is not sand-able; it never takes an attack roll. Extend `test_weapon_type_is_laser` (`ship.rs:2509`). |
| `ship.rs:1224-1231` `in_range` | `PointDefense => false` for every band. Belt and braces: it is never reached because batteries never take a `FireAction`. |
| `combat.rs:851-880` missile launch arm in `do_fire_actions` | Add `WeaponType::PointDefense => { warn!(...); vec![] }`. A `FireAction` naming a battery is a client bug; log and drop, do not panic. |
| `combat.rs:1009-1040` `create_sand_counts` | Add `WeaponMount::Battery(_)` to the error arm alongside Barbette/Bay (unreachable, since the outer test is `kind == Sand`). |
| `combat.rs:1046-1055` `point_defense_score` | `PointDefense => 0` and `Battery(_) => 0`. Deliberate: this keeps batteries out of the *action-driven* PD path, so a stray `PointDefenseAction` naming a battery is dropped with the existing debug log at `combat.rs:1091-1094`. |

---

## 4. Combat integration

### 4.1 Should a battery cost an action?

**No.** The book is unambiguous — "automatically intercepts", "removes the need for
separate gunners dedicated to point defence". There is no firing decision to make
and no gunner to spend. Concretely:

- No `PointDefenseAction` is ever queued for a battery. `point_defense_score`
  returning 0 (§3.3) enforces this server-side.
- No `BoostTarget::PointDefense` for a battery — leadership boosts a person's
  check, and there is no check.
- No button in `WeaponUse.tsx`. See §5.

### 4.2 New state on `Ship`

```rust
// ship.rs:265, beside point_defense_list
#[serde(skip)]
pub point_defense_list: Vec<(usize, u16)>,
/// Missiles this ship's point-defence batteries will absorb this round, rolled
/// once at the start of resolution.  High Guard p. 40.
#[serde(skip)]
pub point_defense_pool: u32,
```

`#[serde(skip)]` matches `point_defense_list` — this is per-round scratch, not
persisted state, so scenario save/load is unaffected.

Accessors alongside `set_point_defense_list` / `clear_point_defense`
(`ship.rs:672-678`); `clear_point_defense` also zeroes the pool.

### 4.3 Rolling the pool

In `Entities::fire_actions` (`entity.rs:646-660`), where the tallies are already
built. The existing loop is driven by `point_defense_actions`, which only contains
ships that queued an action — batteries need every ship, so this is a second pass:

```rust
// Batteries are automatic: every ship with one gets a pool whether or not its
// crew queued anything.  Iterate in name order so the RNG sequence is stable
// for the seeded integration tests.
let mut names: Vec<&String> = self.ships.keys().collect();
names.sort_unstable();
for name in names {
  let mut ship = self.ships[name].write().unwrap();
  let pool = roll_battery_pool(&ship, rng);
  ship.set_point_defense_pool(pool);
}
```

`rng` is already a parameter of `fire_actions` (`entity.rs:640`).

**Determinism matters here.** `self.ships` is a `HashMap` (`entity.rs:59`), and the
existing code already sorts missiles before resolution "ONLY to ensure unit tests
run consistently" (`entity.rs:748-755`). Rolling in unsorted map order would make
every seeded integration test in `callisto/tests/webserver.rs` non-reproducible.
Sort by name.

`roll_battery_pool` lives in `combat.rs` next to `point_defense_score`:

```rust
#[must_use]
pub fn roll_battery_pool(ship: &Ship, rng: &mut dyn RngCore) -> u32 {
  ship.weapons().iter().enumerate()
    .filter(|(index, _)| ship.active_weapons[*index])
    .filter_map(|(_, weapon)| battery_intercept_dice(weapon))
    .map(|dice| u32::from(roll_dice(dice, rng)))
    .sum()
}
```

`roll_dice(n, rng)` (`combat.rs:27`) already takes a die count, so 2D/4D/6D need no
new primitive. Batteries stack additively across mounts — the P.F. Sloan's two
Type III batteries roll 6D each, not 12D as one pool; mechanically identical for a
sum, but keep them separate so a crit that kills one battery halves the ship's
protection.

### 4.4 Spending the pool

The only change to the resolution path, at `entity.rs:811-836`:

```rust
// Batteries first: they are free and automatic, and spending them first leaves
// the gunners' queued point defence available for the overflow.
let available_point_defense = if target.point_defense_pool > 0 {
  target.point_defense_pool -= 1;
  1
} else {
  point_defense_memory
    .remove(&target_name)
    .unwrap_or_else(|| use_next_point_defense(&mut target.point_defense_list, rng))
};
```

Everything downstream — destroying the missile, `cleanup_missile_list`, the
`ExhaustedMissile` effect — is unchanged. Note the battery branch yields exactly 1
and therefore never feeds `point_defense_memory`; the pool *is* the memory.

Ordering rationale (answering §1.4(e)): the pool is a free renewable resource and
the gunners' list is a scarce one. Draining free capacity first strictly dominates
for the defender, and the book gives the defender the allocation choice ("This can
be applied to any salvo or spread between several salvoes"), so taking the optimal
allocation on their behalf is faithful.

### 4.5 Effect messages

The player needs to see which layer stopped a missile. Split the existing message
at `entity.rs:831`:

- pool: `"Missile {missile} destroyed by {target}'s point defence battery"`
- list: `"Missile {missile} destroyed by {target}'s point defence"` (unchanged)

Optionally emit one `EffectMsg::message` per ship per round summarising the pool
roll — `"Kinunir's point defence batteries will intercept 19 missiles this round"` —
so a referee can see the number before the missiles arrive. Cheap and worth it.

### 4.6 Tests

- `combat.rs` unit tests: `battery_intercept_dice` over all legal and illegal
  pairs; `roll_battery_pool` with a seeded RNG for Type I/II/III, for two
  batteries, and with one battery disabled via `active_weapons`.
- `combat.rs`: `point_defense_score` returns 0 for `PointDefense`/`Battery(_)`.
- `entity.rs`: a ship with a Type I battery under a 12-missile small-bay salvo
  loses exactly `pool` missiles to batteries before the gunner list is touched.
- `ship.rs:2204` `test_weapon_ordering` and `ship.rs:2509`
  `test_weapon_type_is_laser`: new cases.
- `unit_tests.rs:1817` already round-trips a `PointDefenseAction` JSON; add a
  companion asserting a `PointDefenseAction` naming a battery is dropped, not
  honoured.

---

## 5. Frontend

### 5.1 Required — the read path

| File | Change |
| --- | --- |
| `lib/weapon.ts:5` | `WeaponMount = string \| {Turret: number} \| {Bay: BaySize} \| {Battery: number}` |
| `lib/weapon.ts:17-34` `weaponToString` | New arm before the `console.error` fallback: `"Battery" in mount` → `` `Point Defence Battery (Type ${roman(mount.Battery)})` ``. **Without this the HUD renders the literal string `ERROR in weaponToString()`** for any migrated design, because `weaponToString` feeds `compressedWeapons` (`shipDesignTemplates.ts:48-68`), `findNthWeapon` (`:73`), `getWeaponName` (`:85`) and the AddShip design tooltip. This is the hard ordering constraint on the phase plan. |
| `components/controls/WeaponUse.tsx:525-545` | The button list filters with `!weapon_name.includes("Sand")`. Batteries also take no action, so replace the string test with a predicate — `isActionableWeapon(weapon)` in `lib/weapon.ts`, false for `Sand` kind and for `Battery` mounts. Doing it by weapon rather than by name also fixes the latent bug where a design named e.g. "Sandstorm" would lose its buttons. |
| `components/controls/WeaponUse.tsx:794-840` | No change. Batteries never produce a `PointDefenseAction`, so the queued-actions strip stays correct by construction. The `console.error` at `:801-805` for a non-laser PD action remains a useful assertion. |

### 5.2 Required — the design editor

| File | Change |
| --- | --- |
| `lib/hardpoints.ts:288-300` `MOUNT_OPTIONS` | Three entries: `battery-1` / `battery-2` / `battery-3`, labelled "PD Battery (Type I/II/III)", mount `{ Battery: n }`. |
| `lib/hardpoints.ts:306` `FIRMPOINT_OPTION_IDS` | **Do not add them.** A 20-ton battery on a hull under 100 tons is impossible, and Firmpoint hulls have no Hardpoints for it to consume. The book does not say this in so many words — flagged in §8. |
| `lib/hardpoints.ts:67-75` `mountCost` | No change. The default arm already returns 1, which is correct. |
| `lib/hardpoints.ts:329-349` `mountOptionId` | Add a `"Battery" in` comparison arm alongside the `Turret` and `Bay` ones, or it returns `null` and the editor treats a legitimate battery as an unrecognised mount. |
| `lib/hardpoints.ts:356-363` `WEAPON_KINDS` | Add `"PointDefense"`. |
| `lib/hardpoints.ts` (new) | A compatibility filter so the kind × mount dropdowns cannot produce nonsense: `PointDefense` offers only Battery mounts, and Battery mounts offer only `PointDefense`. This is genuinely new behaviour — today the two dropdowns are independent — and it is the mitigation for §3.2's "honest cost". |
| `lib/hardpoints.test.ts:407-408` | Existing tests assert `mountOptionId` returns `null` for unknown mounts; add positive `Battery` cases and a negative for `{ Battery: 4 }`. |

`groupWeapons` / `expandGroups` (`hardpoints.ts:184-238`) need no change — they key
on `JSON.stringify(mount)` and are agnostic.

### 5.3 Icon — **not required**

`assets/icons/` holds mount icons (`turret1-3.svg`, `barbette.svg`, `bay-s/m/l.svg`,
`fixed-mount.svg`) and kind icons (`laser.svg`, `missile.svg`). They are consumed
only by `WeaponButton` (`WeaponUse.tsx:86-300`) and the queued-action strip
(`:727-840`) — both of which are action UI that batteries never enter. Everywhere
else batteries appear (AddShip design tooltip, ship summary, the mount dropdown)
is text via `weaponToString`.

So: **no new SVG is needed to ship this.** One becomes worth drawing only if we
later add a passive-defences readout to the HUD (§9 phase 5), at which point it
should be a flat two-colour outline in the same style as `turret2.svg` — a hemi
dome with three short converging barrels — plus a `PointDefense` entry in
`WEAPON_COLORS` (`WeaponUse.tsx:60-66`). Suggest `"orange"`; `cyan` and `purple`
are taken by the pilot-action table at `:82-85` and would read as a clash even
though they are separate maps.

---

## 6. Migration

### 6.1 Files that change

79 design files in `callisto/ship_templates/`. Four change. Verified by reading the
book entries and diffing against the JSON.

| File | Edit | Indices affected | Indices that move |
| --- | --- | --- | --- |
| `system_defence_boat_dragon.json` | idx 3 `{"kind":"Pulse","mount":{"Turret":2}}` → `{"kind":"PointDefense","mount":{"Battery":2}}` | 3 | none |
| `colonial_cruiser_kinunir.json` | idx 4 `{"kind":"Pulse","mount":{"Turret":3}}` → `{"kind":"PointDefense","mount":{"Battery":3}}` | 4 | none |
| `fleet_escort_p_f_sloan.json` | idx 32, 33 `{"kind":"Pulse","mount":{"Turret":3}}` → `{"kind":"PointDefense","mount":{"Battery":3}}` | 32, 33 | none |
| `midu_agasham.json` | **append** two `{"kind":"PointDefense","mount":{"Battery":3}}` at idx 21, 22 | 21, 22 (new) | none — 0–20 unchanged |

**Nothing breaks.** In all four cases every pre-existing `weapon_id` keeps its
index. Three are in-place field rewrites; the fourth is an append.

### 6.2 Knock-on checks, all clear

- **Scenarios reference designs by `name`** — none of the four names change.
- **No scenario uses any of the four designs.** `callisto/scenarios/*.json`
  reference Light Fighter ×11, Gazelle ×4, Threshing Oar ×2, Harrier ×2, and one
  each of Void Trader, McClellan Trader, HMS Excelsior, Far Trader, Ekawsiykua
  Escort. So no live scenario changes behaviour at all on migration day.
- **Hardpoint allowance stays legal.** Dragon: 400 t → 4 Hardpoints; 2 barbettes +
  1 small bay + 1 battery = 4, unchanged from the stopgap. Midu Agasham: 3000 t →
  30 Hardpoints; 21 → 23 used.
- **`gunnery` arrays.** `Crew::get_gunnery` reads out-of-range indices as 0
  (`hardpoints.ts:180-182` documents the mirror), so the Midu Agasham's appended
  entries need no gunnery entries. A battery's gunnery is never read.
- **Per-ship armament overrides.** `Ship::weapons` is
  `Option<Vec<Weapon>>` (`ship.rs:157-161`); a saved ship carrying an inline copy
  of the old `Pulse`/`Turret(2)` list stays valid and simply keeps the weak
  stopgap behaviour until re-added. No deserialization failure.

### 6.3 The one real compatibility hazard

An **old frontend against a new backend** receives `"kind":"PointDefense"` and
renders `ERROR in weaponToString()` in the HUD and the design picker. They deploy
together, so this is a sequencing note, not a risk — but it is why §9 puts the
frontend read path *before* the data migration.

The reverse (new frontend, old backend) is harmless: no design ever contains a
battery, so the new code paths are dead.

### 6.4 Designs we could not verify

25 designs contain `Pulse` weapons. Only the four above are High Guard designs
whose book entry lists a point-defence battery. The eight `Ships of the Reach`
designs and the seven Aslan designs with pulse turrets could not be checked —
neither book is in `/Users/dan/My Drive/Traveller/MGT/`. If those PDFs exist
elsewhere, re-run the same audit before declaring the library correct.

`callisto/modified designs.md` TODO 1 should have its point-defence rows deleted
and replaced with a pointer to this document once phase 3 lands.

---

## 7. What this actually changes at the table

The stopgap's expected kill rate, computed from `use_next_point_defense`
(`combat.rs:1121-1143`): the Dragon's `Pulse`/`Turret(2)` scores
`1 × 2 = 2`, plus gunnery 1, minus 1 → bonus `+2`. Roll 2D + 2 vs 8, effect
`= 2D − 6`, floored at 1 on success:

| 2D | 6 | 7 | 8 | 9 | 10 | 11 | 12 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| p (/36) | 5 | 6 | 5 | 4 | 3 | 2 | 1 |
| missiles killed | 1 | 1 | 2 | 3 | 4 | 5 | 6 |

Expected = 61/36 ≈ **1.7 missiles per round** — and it costs that turret its
action for the round.

| | today (stopgap) | book (§1.4(a) reading) | ratio |
| --- | --- | --- | --- |
| Type I | n/a | 2D = 7.0 | — |
| Type II (Dragon) | ~1.7, costs an action | 4D = 14.0, free | **8.3×** |
| Type III (Kinunir) | ~2.2, costs an action | 6D = 21.0, free | **9.5×** |

For calibration, `do_fire_actions` (`combat.rs:853-860`) launches 12 missiles from
a Small Missile Bay, 24 from a Medium, 120 from a Large. So a single Type III
battery statistically eats **most of two small-bay salvoes per round**, and the
P.F. Sloan's pair absorbs about 42 missiles a round for free.

That is a large power shift and it is what the book says. Two things follow:

1. It is the correct fix — batteries are the book's answer to massed missile fire,
   and the ships that carry them (Kinunir, Midu Agasham, P.F. Sloan, Dragon) are
   exactly the ships meant to shrug off missiles.
2. If playtest says it dominates, the tuning dial already exists in the rules: the
   Fleet Battles flat values of 4/8/12 (HG p. 113) are ~57% of the tactical
   averages and would slot into `battery_intercept_dice` as a constant instead of
   a die count. Put it behind a named constant, not a magic number.

---

## 8. Open questions for the user

1. **Confirm the "+2D = roll 2D, remove that many missiles" reading** (§1.4(a)).
   Everything downstream depends on it. The alternative — treating it as a DM to
   some check — has no check to modify.
2. **Are the power figures worth recording?** Type I/II/III draw 10/20/30 Power and
   `ShipDesignTemplate` has a `power` field (`ship.rs:311`) that nothing reads in
   combat. Recommend ignoring, consistent with every other weapon.
3. **Batteries on Firmpoint hulls (<100 t).** §5.2 excludes them from the small-craft
   dropdown on tonnage grounds. The book does not explicitly forbid it. Confirm.
4. **Should the pool be visible to the enemy?** Today `point_defense_list` is
   `#[serde(skip)]` and invisible. A referee tool arguably wants the pool shown to
   everyone; a hidden-information game wants it shown only to the owner. Currently
   there is no per-player filtering of ship state to hook into.
5. **Point Defence software** (HG p. 74) lets these batteries defend *another*
   ship in Close/Short range. Three of the four affected designs carry
   Point Defence/2. Worth a follow-up project, or leave PD self-only indefinitely?
6. **Gauss batteries** need torpedoes and a missile Thrust rating to mean anything.
   Confirm they stay out until torpedoes land (TODO 1 also wants Torpedo Barbettes).
7. **The `use_next_point_defense` pop-order oddity** (§2.2) — is the worst-first
   behaviour intentional? If it is a bug, it is a two-line fix and this is a
   natural time to make it, but it changes existing combat results and should be
   its own commit.

---

## 9. Phased plan

Each phase is independently shippable and leaves the tree green
(`cargo clippy --all-targets --all-features -- -D warnings`, `cargo nextest r`,
`npm run build`, `npx eslint .`).

**Phase 1 — backend schema + combat.** §3.2, §3.3, §4.2–4.6. Adds the enum
variants, the pool, the resolution branch and the tests. **Zero behaviour change**:
no design file contains a battery yet, so `roll_battery_pool` returns 0 for every
ship and the new branch is never taken. Safe to merge on its own.

**Phase 2 — frontend read path.** §5.1. `weaponToString`, the `WeaponMount` type,
the `isActionableWeapon` button filter. Also inert — nothing produces a battery
yet — but it **must** land before phase 3 or the HUD prints
`ERROR in weaponToString()`.

**Phase 3 — data migration.** §6.1. Four JSON files. This is where the behaviour
change lands and where §7's power shift becomes real. Small enough to revert
cleanly if playtest goes badly. Update `callisto/modified designs.md` in the same
commit.

**Phase 4 — design editor.** §5.2. Mount options, `mountOptionId`, `WEAPON_KINDS`,
the kind × mount compatibility filter, `hardpoints.test.ts` cases. Independent of
phases 2–3; needed only so a referee can *author* a design with batteries rather
than only load one.

**Phase 5 — optional polish.** Passive-defences readout in the ship summary; the
battery icon and `WEAPON_COLORS` entry (§5.3); the per-round pool `EffectMsg`
(§4.5) if it was not folded into phase 1. Only worth doing once phases 1–4 have
been played with.

Deliberately **not** in this plan: gauss batteries, torpedoes, Point Defence
software, quad turrets, screens and nuclear dampers, and the mixed-turret concept
`WeaponMount` still lacks. None of them block this work and none of them are made
harder by it.
