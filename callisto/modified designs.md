# Modified Designs — memo to ourselves

Ship designs in `ship_templates/` converted from **Mongoose Traveller High Guard (April 2024
update)**, pp. 136–232 (printed page numbers). Only hulls of **5,000 tons or less** were
converted; the system does not currently support larger ships.

Everything below records a place where the book's design could **not** be represented
faithfully in `ShipDesignTemplate` (`callisto/src/ship.rs`). These are not transcription
errors — they are gaps in our model. **We need to go back and fix these ASAP.**

---

## ~~TODO 1 — Implement the missing weapon types~~ MOSTLY DONE (2026-09-07)

**Shipped.** `WeaponType` gained `Torpedo, Fusion, Plasma, Railgun, Meson, MassDriver,
Repulsor`, and damage/range/hit-mod/armour-penetration became properties of the
*weapon and its mount together* rather than of the weapon alone — see
`rules_tables.rs::weapon_profile`, transcribed from the Turret Weapons (p. 28),
Barbettes (p. 30) and Bay Weapons (pp. 32-33) tables.

Because the book only lists a weapon against mounts it is actually sold in, a missing
entry doubles as the legality rule: there is no torpedo turret and no laser bay, so
those pairings return `None` and the editor refuses to offer them.

Three things started working as a side effect: **AP** (nothing implemented it before, so
meson's AP ∞ and railgun's AP 4/5/10 did nothing), **mount-dependent range** (a railgun
reaches Short from a turret but Medium from a barbette), and **torpedo damage** —
missiles now carry the weapon that launched them, so a torpedo resolves at its own 6D
instead of being faked as a 4D missile.

Still outstanding from this section: **Ion weapons (see TODO 7)**. Point Defence Laser
Batteries are done — see TODO 9. Not done, and deliberately so:
Laser Drill (an Adjacent-range mining tool, not a warship weapon) and the Orbital
Strike / Orbital Bombardment variants, which target planets — something combat has no
notion of.

The original substitution table, kept for the record:

| Book weapon | Encoded as | Affected designs |
|---|---|---|
| Torpedo Barbette | `Missile` / `Barbette` | Torpedo Boat (p152), Merchant Cruiser - Leviathan (p217, x2) |
| Fusion Barbette | `Particle` / `Barbette` | Destroyer Escort - Chrysanthemum (p207) |
| Point Defence Laser Battery (Type II) | `Pulse` / `Turret(2)` | System Defence Boat - Dragon (p193) |
| Point Defence Laser Battery (Type III) | `Pulse` / `Turret(3)` | Colonial Cruiser - Kinunir (p215), Fleet Escort - P.F. Sloan (p232, x2) |

Point-defence batteries were mapped to *Pulse turrets* deliberately: `combat.rs`
`point_defense_score()` only scores `Beam`/`Pulse` in a `Turret`, and scales with the turret
count — barbettes and bays score zero. Type II -> `Turret(2)` and Type III -> `Turret(3)` so
the PD bonus tracks the battery grade. This is a hack; they are 20-40 ton installations,
not turrets.

Note: the pre-existing `midu_agasham.json` **omits** its two Point Defence Laser Batteries
(Type III) entirely. It should be updated once real support lands.

Weapon types NOT needed under 5,000 tons but required before we can support larger ships:
meson guns (incl. spinal mounts), particle accelerator spinal mounts, railguns, repulsors,
orbital strike mass drivers. **There is no spinal mount concept in `WeaponMount` at all.**

## ~~TODO 8 — Migrate the designs that still use substituted weapons~~ DONE (2026-09-07)

**Shipped.** Three designs carried stand-ins because the real weapon did not exist yet.
Each was re-checked against the book rather than trusted from the table above:

| Design | Was | Now | Book |
|---|---|---|---|
| Torpedo Boat (p152) | `Missile` / `Barbette` | `Torpedo` / `Barbette` | "Torpedo Barbette" |
| Merchant Cruiser - Leviathan (p217) | `Missile` / `Barbette` x2 | `Torpedo` / `Barbette` x2 | "Torpedo Barbettes x2" |
| Destroyer Escort - Chrysanthemum (p207) | `Particle` / `Barbette` x3 | `Fusion` x1 + `Particle` x2 | "Fusion Barbette" + "Particle Barbettes x2" |

Not changed, having been checked and found correct: the **Fer-de-Lance**'s four barbettes
really are missile barbettes (the book's "accurate" modifier is still dropped, per TODO 2),
and the Leviathan's two **Fixed Mount** missile racks are genuinely missiles — only its
barbettes were torpedoes.

Every other new-weapon mention in the book (meson screens and spinal mounts, repulsor
bays, large meson gun bays, fusion barbettes x12) belongs to a hull over 5,000 tons and
is therefore out of scope — see "Out of scope" below.

`rules_tables.rs::shipped_designs_use_legal_mounts` now walks the whole library and fails
if any design carries a weapon the rules do not allow in that mount, so a future typo
cannot silently disarm a ship at runtime.

The Point Defence Laser Battery substitutions are deliberately left alone; see TODO 9.

---

## TODO 2 — Weapon modifiers are silently dropped

High Guard weapon modifications have nowhere to live in the schema. Dropped from:

| Design | Dropped modifiers |
|---|---|
| Military Gig - Close Escort Variant (p138) | intense focus, high yield |
| System Defence Boat - Dragon (p193) | size reduction x3 (on the Small Missile Bay) |
| Destroyer Escort - Fer-de-Lance (p211) | accurate, high yield |
| Merchant Cruiser - Leviathan (p217) | energy efficient x3 |
| Cargo Carrier - MK Mora (p223) | long range, high yield, accurate |

## ~~TODO 3 — `add ship` needs weapon customization~~ DONE (2026-09-06)

~~The "add ship" flow currently pins a ship to its design's fixed weapon list. Real play needs
per-ship armament: a Free Trader or Far Trader may be fitted with almost anything, and 11 of
these designs ship with **empty mounts** the crew is expected to fill.~~

**Shipped.** `Ship.weapons: Option<Vec<Weapon>>` holds a ship's own armament, with `None`
meaning "inherit from the design" — so every scenario written before the field existed keeps
working untouched. Add Ship gained a hardpoint editor: rows are *groups* ("30 x Triple Beam
Turret") rather than one row per mount, checked against the hull's Hardpoint or Firmpoint
allowance, with gunner skill on each row. Re-arming a ship clears its stale queued fire
orders, since `weapon_id` is an index into the list.

The allowance is advisory throughout — the engine never validates armament, so a referee can
still exceed it deliberately. `player.rs` rejects only what would panic later: turret sizes
outside 1-3, and more than `MAX_SHIP_WEAPONS` (see TODO 5).

The list below stays as reference: these are the designs the book leaves unarmed, and they
are now fittable in the UI rather than stuck empty.

Designs with empty mounts in the book, recorded here as `"weapons": []`:
Ship's Boat, Slow Boat, Pinnace, Slow Pinnace, Modular Cutter, Shuttle,
Seeker Mining Ship - Type J, Scout - Serpent, Safari Ship - Type K,
Mercenary Cruiser - Type C (**8 empty triple turrets**), X-Boat Tender - XT
(2 empty single turrets + 1 empty pop-up single turret).

## TODO 4 — Defensive screens are unrepresented

No schema field exists for these; they were dropped:

- Colonial Cruiser - Kinunir (p215): Nuclear Dampers x5, Black Globe Generator
- Fleet Escort - P.F. Sloan (p232): Meson Screens x2, Nuclear Dampers x2

---

## TODO 5 — `MAX_SHIP_WEAPONS` binds long before the rules do

`player.rs:25` caps client-supplied armament at **64 weapons**:

```rust
const MAX_SHIP_WEAPONS: usize = 64;
```

Hardpoints are one per 100 tons, so the cap binds at **6,400 tons** — a 6,500-ton design
wants 65 mounts and is rejected outright, with a "more than the limit of 64" error that
says nothing about tonnage. The limit is not a rules constraint; it exists only to stop a
malformed or hostile request allocating an unbounded weapon list.

**Not urgent.** The library stops at 5,000 tons (50 hardpoints) because larger designs were
deliberately out of scope for the import, and per TODO 6 much bigger work gates real
capital ships anyway.

**When fixing:** derive the cap from displacement rather than raising the constant to
another arbitrary number — the allowance is already computable from the design. Keep an
absolute ceiling for the malformed-request case, but make the normal path scale. Bays
complicate it slightly: a Large Bay costs 5 hardpoints but is still 1 weapon, so a
displacement-derived cap is an upper bound, never an exact count.

## TODO 6 — Capital ships need batteries, not longer weapon lists

Traveller hulls go to **1,000,000 tons**, which is 10,000 hardpoints. Raising
`MAX_SHIP_WEAPONS` does not get us there: `Ship.weapons` is a flat `Vec<Weapon>` and
`weapon_id` is an index into it, so a capital ship would carry ten thousand individually
addressable weapons and combat would resolve ten thousand separate attacks. The
representation gives out well before the cap does.

The book's own answer is **batteries** — turrets grouped and fired as one unit. That is the
model to adopt, and note the UI already works this way: the Add Ship hardpoint editor
groups identical mounts into a single row with a count (`fe/callisto/src/lib/hardpoints.ts`),
because no design in the library has more than five distinct mount/weapon combinations. The
data model should follow the same shape rather than the editor flattening groups out on
submit.

Blocking real capital ships, roughly in order:

- **Batteries** — grouped turrets resolved as one attack, per above.
- **Spinal mounts** — no `WeaponMount` variant exists at all. This is now the only
  *mount* still missing, and it needs more than a variant: spinal weapons carry a
  ×1,000 Damage Multiple, scale their dice with tonnage, and take negative DMs at close
  range (High Guard p. 36).
- ~~Torpedoes~~, ~~meson guns, railguns, repulsors, mass drivers~~ — done, see TODO 1.
- **Defensive screens** — meson screens, nuclear dampers, black globes (see TODO 4).

**Current position: the 5,000-ton import limit stands and is fine.** None of the above is
worth starting until we actually want ships above that.

---

## TODO 7 — Ion weapons (HIGH PRIORITY)

Ion cannon is the one genuinely missing weapon after the profile work, and it matters:
it is the standard way pirates and customs ships disable a target rather than destroy
it, which is a scenario the tool should support.

The mechanic is new but small (High Guard p. 30, *Weapon Trait: Ion*):

- On a hit, roll damage **ignoring the target's armour entirely**.
- Deduct that from the target's **Power**, not its hull. Nothing is permanently damaged.
- The reduction lasts until the target finishes its next set of actions.
- If the attack's Effect is **6 or more**, it lasts **D3 rounds** instead.
- Hardened systems are immune: the crew may allocate Power to them before the deduction.

**Most of the plumbing already exists.** `Ship` has `current_power`, and `best_thrust`
already derives thrust from it, so a power deduction produces the right manoeuvre
penalty without new code. What is missing is the temporary-deduction bookkeeping (a
duration, and restoring power when it lapses) and the hardened-systems carve-out.

Ion is sold as a barbette and as all three bay sizes, never a turret:

| Mount | TL | Range | Damage |
|---|---|---|---|
| Ion Cannon (barbette) | 12 | Medium | 7D |
| Small Ion Bay | 12 | Medium | 6D |
| Medium Ion Bay | 12 | Medium | 8D |
| Large Ion Bay | 12 | Long | 10D |

Note the fleet-battle rules (p. 132) give ion weapons a *separate* damage track
(Effect per Weapon: barbette 75, bays 200/500/3,500) used only at fleet scale. That is
a different system from the ship-scale rule above and should not be conflated with it.

---

## ~~TODO 9 — Point Defence Laser Batteries~~ DONE (2026-09-07)

**Shipped.** Designed in `docs/pd_batteries_design.md`, built as described there.

A point-defence battery is **not a weapon** (High Guard p. 40). It has no attack roll,
no damage, no range band, no gunner and no offensive mode; it "automatically intercepts"
a number of missiles each round, which the defender may spread across salvoes as they
like. So it is modelled as `WeaponType::PointDefense` in a `WeaponMount::Battery(grade)`,
where the grade is the book's Type and sets the Intercept: **2D / 4D / 6D for Type
I / II / III**.

Resolution is a **single per-round pool** per ship, which is how the book totals it.
Batteries contribute their Intercept, and every queued gunner contributes the Effect of
one check — "a gunner may only attempt Point Defence once every round… the Effect of the
check will remove that many missiles from the salvo" (Core Rulebook p. 171). Each
incoming object then drains the pool.

That replaced an earlier model which popped **one weapon per incoming missile**, so a
gunner only ever rolled if enough missiles arrived to use up the previous weapon's
surplus. That was wrong twice over: it silently capped a ship at one check per missile
rather than one per gunner, and nothing in the rules stops two gunners engaging the same
missile. It also made the order of the list matter, which it should not.

Batteries are rolled per mount rather than pooled into one throw, so a critical hit
disabling one removes exactly its share. Because they sit in `Ship::weapons()` like
anything else, `active_weapons` and the `ShipSystem::Weapon` crit already treat them as
destructible hardware with no new code.

Designs migrated (each re-verified against the book, not taken from the table above):

| Design | Was | Now |
|---|---|---|
| System Defence Boat - Dragon (p193) | `Pulse`/`Turret(2)` | `PointDefense`/`Battery(2)` |
| Colonial Cruiser - Kinunir (p215) | `Pulse`/`Turret(3)` | `PointDefense`/`Battery(3)` |
| Fleet Escort - P.F. Sloan (p232) | `Pulse`/`Turret(3)` x2 | `PointDefense`/`Battery(3)` x2 |
| Midu Agasham | *omitted entirely* | `PointDefense`/`Battery(3)` x2 (appended) |

No existing `weapon_id` moved: three were in-place field rewrites and the fourth was an
append, so queued actions and per-ship armament overrides keep working.

**How much this changed:** the stopgap resolved through the ordinary point-defence path,
which rolls 2D vs 8 and removes *Effect* missiles. On the Dragon that was worth about
1.7 missiles per round where the book says 14 — a factor of eight, and the reason this
was worth doing before Ion.

**Torpedoes** are half as easy to stop: "a torpedo salvo halves the Effect of any
successful point defence taken against it, rounding down" (High Guard p. 39). Since we
resolve against one summed pool rather than per-check, halving is expressed as a torpedo
costing two pool points where a missile costs one — `floor(pool / 2)` torpedoes stopped,
the same arithmetic applied to the total. The Fleet Battles rule prices it identically
("double the amount taken from the pool", p. 113), which corroborates the aggregate
reading. A pool too small for a torpedo stops nothing and keeps its leftover point for a
missile.

The same paragraph gives torpedoes **DM-2 on attack rolls against ships under 2,000
tons**, since they are built to kill capital ships. That is now applied in `attack()`.

Still not built, and deliberately: **Point Defence Gauss Batteries** (p. 40). Same
tonnage and same 2D/4D/6D, but tuned against torpedoes with DM penalties by target
Thrust, plus ammunition. No design in the library carries one.

---

## Mixed-turret refactoring

Per the Core Rulebook ("Double and Triple Turrets"), a turret holding *different* weapon
types may only fire one type per round, while same-type weapons fire together for bonus
damage. Mixed turrets were therefore regrouped into uniform ones, conserving gun count.

**Cargo Carrier - MK Mora (p223)**

- Book: `Triple Turrets (long range, high yield pulse lasers x2, sandcaster) x6`
  = 12 pulse + 6 sand -> **4x Pulse Turret(3) + 2x Sand Turret(3)**. Exact, no loss.
- Book: `Triple Turrets (missile racks x2, accurate, high yield beam laser) x4`
  = 8 missile + 4 beam -> **3x Missile Turret(3) + 1x Beam Turret(3)** = 9 missile + 3 beam.
  Deliberate rounding (decided 2026-09-05): **+1 missile rack, -1 beam laser**, turret count
  preserved at 4.

**Corsair - Type P (p196)** — `Triple Turrets (beam laser x1) x3` is not mixed, merely
under-filled, so it is encoded literally as 3x `Beam Turret(1)`.

**Merchant Cruiser - Leviathan (p217)** — `Double Turrets (energy efficient x3 beam lasers)
x6` read as beam lasers in double turrets with Energy Efficient applied x3 (6 tons / 6
turrets confirms doubles), i.e. 6x `Beam Turret(2)`. Not mixed.

---

## Judgment calls worth a second look

- **Jump Shuttle (p177)** — crew box reads "Engineers" (plural, no count). Drives + power
  plant total 33 tons; High Guard requires 1 engineer per 35 tons, so `crew: 3`
  (Pilot, Astrogator, 1 Engineer). If the book intended 2, this should be 4.
- **Corsair - Type P (p196)** — same issue. 48.5 tons of drives + power plant -> 2 engineers,
  giving `crew: 11` (Pilot, Astrogator, Engineers x2, Gunners x3, Thugs x4).
- **Mercenary Cruiser - Type C (p202)** — crew box reads "Stewards" (plural, no count).
  Counted as 1, giving `crew: 12`. Its 30 barracks hold troops, who per the Kinunir's own
  design note count as basic passengers needing no steward.
- **X-Boat Tender - XT (p204)** — crew box reads "Captain Pilot," with no comma.
  **Resolved 2026-09-05: two people (Captain, Pilot), `crew: 7`.** Not an open question.
- **Express Boat (p158)** — has no manoeuvre drive at all, so `maneuver: 0`. Its power plant
  is Power 20; a separate High-Efficiency Battery supplies the 40 power the jump drive needs.
  Only the power plant value is recorded, so `power: 20`.
- **Laboratory Ship - Type L (p185)** — 360-ton dispersed-structure hull whose drives are
  rated for 400 tons. `displacement: 360` (actual hull), `hull: 160`.
- **Far Trader** — High Guard has two (Empress Marava p167, Type A2 p169); both were added.
  The pre-existing `far_trader.json` is **the Core Rulebook's Far Trader Type A2**
  (Core p196) with two single beam turrets fitted — it matches Core exactly on every field
  including armour 2, power 90 and crew 5. It is *not* a Marava variant; an earlier note
  here said so and was wrong. Note this means `far_trader.json` (Core A2) and
  `far_trader_type_a2.json` (High Guard A2) are the same ship from two editions that
  disagree — see below.

## Designs skipped as already present

Light Fighter, Free Trader, Gazelle, Scout/Courier, System Defence Boat (generic 200t),
Patrol Corvette - Type T, Midu Agasham. Note that several existing files diverge from the
April 2024 printing — e.g. `gazelle.json` is TL15/hull 176/thrust 6/computer 30 where the
book's Gazelle is TL14/hull 160/thrust 5/computer 20bis. See the Core Rulebook section
below — those files are Core Rulebook versions, not divergences.

## Out of scope

Hulls over 5,000 tons were not converted: Light Carrier - Skimkish (29,000t) through
Dreadnought - Tigress (500,000t), pp. 235-285. They depend on spinal mounts, meson weapons
and mount counts in the hundreds.

---

# Core Rulebook conflicts (surveyed 2026-09-05)

The Core Rulebook (2022 update, 11-12-2024 printing) carries 24 ship designs on pp. 190-227.
**Every one overlaps something already in this directory.** 13 match our files exactly;
11 conflict.

**Key finding: our pre-existing designs are Core Rulebook versions, and the designs added
from High Guard (April 2024) are the revised versions of the same ships.** The directory
now mixes two editions. Proof by distinctive values: `patrol_corvette_type_t.json` has
power 405 / fuel 124, which is Core exactly (High Guard says 300 / 122); `gazelle.json` is
TL15 / thrust 6 / power 540 / fuel 128 / computer 30 / hull 176, which is Core exactly
(High Guard says TL14 / thrust 5 / power 570 / fuel 130 / computer 20bis / hull 160).

## Designs I added from High Guard that contradict the Core Rulebook

| File | Field | Ours (High Guard) | Core Rulebook |
|---|---|---|---|
| `far_trader_type_a2.json` | power | 75 | 90 |
| | crew | 4 | 5 |
| `laboratory_ship_type_l.json` | displacement | 360 (Dispersed) | 400 (Standard) |
| | crew | 2 | 4 |
| `survey_scout_donosev.json` | hull | 144 (Dispersed) | 160 (Standard) |
| | maneuver | 3 | 2 |
| | crew | 10 | 5 |
| `subsidised_liner_type_m.json` | tl | 12 | 14 |
| | crew | 20 | 6 |
| `mercenary_cruiser_type_c.json` | armor | 3 | 4 |
| | power | 540 | 750 |
| | fuel | 250 | 252 |
| | crew | 12 | 6 |
| `ship_s_boat.json` | power | 23 | 30 |
| `shuttle.json` | power | 48 | 60 |
| `passenger_shuttle.json` | tl | 9 | 12 |
| | sensors | Civilian | Basic |

## Pre-existing (Core) files that High Guard revises

These were skipped during the High Guard import, so they remain at Core values:

| File | Core (ours) | High Guard |
|---|---|---|
| `gazelle.json` | TL15, thrust 6, power 540, fuel 128, computer 30, hull 176 | TL14, thrust 5, power 570, fuel 130, computer 20bis, hull 160 |
| `patrol_corvette_type_t.json` | power 405, fuel 124 | power 300, fuel 122 |
| `light_fighter.json` | sensors Military | sensors Improved |

`light_fighter.json` also has `computer: 10` where **both** books say Computer/5 — that one
is a local change, not an edition difference.

Designs where both books agree and our file matches: Scout/Courier, Seeker Mining Ship,
Free Trader, Safari Ship, System Defence Boat, Yacht, Subsidised Merchant, Launch,
Slow Boat, Pinnace, Slow Pinnace, Modular Cutter.

## Missing design

The Core Rulebook's plain **Gig** (p219: 20 tons, TL12, thrust 7, no armour, Basic sensors,
Computer/5, one empty single turret, crew 1) is a *different ship* from High Guard's
"Military Gig - Close Escort Variant" (TL14, armour 4, thrust 8, Stealth (Improved), fixed
pulse laser, crew 2). We have only the military variant. The base Gig is not in the
directory.

## RESOLVED — High Guard wins (2026-09-05)

Where the two books disagree, **High Guard (April 2024) is canon**. Applied:

| File | Change |
|---|---|
| `gazelle.json` | hull 176->160, maneuver 6->5, power 540->570, fuel 128->130, crew 21->20, computer 30->20, tl 15->14 |
| `patrol_corvette_type_t.json` | power 405->300, fuel 124->122 |
| `light_fighter.json` | sensors Military->Improved |

The eight High-Guard-sourced files that contradicted Core (`far_trader_type_a2`,
`laboratory_ship_type_l`, `survey_scout_donosev`, `subsidised_liner_type_m`,
`mercenary_cruiser_type_c`, `ship_s_boat`, `shuttle`, `passenger_shuttle`) were already at
High Guard values and were left alone.

### Far Trader consolidation

`far_trader.json` now holds the **Empress Marava** stats but keeps `"name": "Far Trader"`,
and `far_trader_empress_marava.json` was deleted. Only two fields actually changed
(armour 2->0, and its two single beam turrets became doubles) — the Core Type A2 our file
came from and the High Guard Marava were already identical on everything else.

**Why the name was kept:** the template table is keyed on the `name` field
(`ship.rs::load_ship_templates_from_dir`), scenarios store `"design": "<name>"`, and
`TemplateNameOnly` (ship.rs:865) errors with "Could not find design" on a miss, failing the
scenario load. **Filenames are never used for lookup — renaming a file is free, renaming a
design is a breaking change.** Verified: all 54 designs parse, names are unique, and every
`design` reference across `scenarios/` and `tests/scenarios/` resolves.

Caveat for the dev loop: `merge_ship_templates` (ship.rs:78) only inserts, never removes, so
a deleted or renamed design survives in a running server's memory until restart.

### The trader family (clarified)

These are **two ship types**, not three versions of one. Free Trader = jump 1;
Far Trader = jump 2.

| Design | `name` | File | Jump | In |
|---|---|---|---|---|
| Free Trader Type A (Beowulf) | `Free Trader` | `free_trader.json` | 1 | Core p194 **and** High Guard p171 — identical but for crew |
| Far Trader Empress Marava | `Far Trader` | `far_trader.json` | 2 | High Guard p167 only |
| Far Trader Type A2 (Hero) | `Far Trader - Type A2` | `far_trader_type_a2.json` | 2 | Core p196 **and** High Guard p169 — they disagree |

Both books carry the Free Trader Type A and the Far Trader Type A2; only High Guard has the
Empress Marava. Applying High-Guard-wins to the two shared designs:

- `free_trader.json` crew **5 -> 4** — Core lists Pilot, Astrogator, Engineer, Medic,
  Steward; High Guard drops the Medic. (Note our file is armed with two double turrets the
  book does not have, so add gunners if you want the armament crewed.)
- `far_trader_type_a2.json` already held High Guard's power 75 / crew 4 against Core's
  90 / 5, so it needed no change.

A crew-by-crew sweep of all 24 Core designs found no other divergence: every remaining
mismatch was an edition conflict already resolved in High Guard's favour.

### Crewing locally-added armament

High Guard's Crew Requirements table (p22) sets gunners at **1 per turret, barbette and
screen** for commercial ships (military ships get 2 per turret). Two of our files carry
armament the book does not, so they were crewed accordingly:

| File | Book | Ours | Crew |
|---|---|---|---|
| `free_trader.json` | unarmed, crew 4 | pulse double + sand double turret | 4 -> **6** |
| `scout_courier.json` | `Double Turret (empty)`, crew 3 | pulse double turret fitted | 3 -> **4** |

`far_trader.json` needed no adjustment — High Guard already crews the Empress Marava with
`Gunners x2` for its two double turrets (crew 5).

### Resolved

- `light_fighter.json` `computer: 10` -> **5**. Both books say Computer/5; treated as a typo.
- Free Trader / Far Trader duplication: there is exactly one Free Trader (`Free Trader`,
  jump 1) and two Far Traders (`Far Trader` = Empress Marava, `Far Trader - Type A2`).
  No design is duplicated.

### Gig — resolved

**We use High Guard's Gig.** High Guard's only Gig is the *Military Gig - Close Escort
Variant* (p138), already in the directory as `military_gig_close_escort_variant.json` and
verified to match the book exactly (TL14, 20t, hull 8, armour 4, thrust 8, power 30, fuel 1,
crew 2, Basic sensors, Stealth (Improved), Computer/5, fixed pulse laser). The Core
Rulebook's plain starport Gig (p219) will **not** be imported.

### Final audit

All six hand-edited pre-existing files were re-checked field-by-field against their High
Guard pages: `far_trader`, `free_trader`, `gazelle`, `light_fighter`,
`patrol_corvette_type_t`, `scout_courier` — all match, with the only intentional deviations
being the added gunners noted above.

One note on the Gazelle: its prose says the ship makes Jump-4/Thrust-4 with drop tanks
fitted, Jump-5 once they are jettisoned, and Jump-3/Thrust-5 on internal tankage alone. We
record the stat block's **Thrust 5 / Jump 5**, consistent with how every other design here
was read.


---

# Ships of the Reach (imported 2026-09-05)

Source: **Pirates of Drinax, Book 3 — Ships of the Reach**, pp. 2-95 (printed page numbers).
33 designs total: 18 "Ships of the Reach" + 15 Aslan ships.

**Excluded, over the 5,000-ton limit:** Ritchey-class Escort (8,000t, p44),
Galoof-class Megafreighter (30,000t, p48), Planet-class Heavy Cruiser (75,000t, p52).

**Skipped, already present:** Buccaneer, Herald Fast Messenger, Indigo Pirate Carrier,
Star Ray Interceptor, Ekawsiykua Escort — see "Divergences" below, which are NOT yet resolved.

**Written: 25 designs.** All carry the new `role` and `source` metadata fields
("Ships of the Reach" / "Ships of the Reach (Aslan)").

## Weapon substitutions

| Book weapon | Encoded as | Designs |
|---|---|---|
| Small Fusion Gun Bay | `Particle` / `Bay: Small` | Gunship - Fiery (p12) |
| Medium Fusion Gun Bay | `Particle` / `Bay: Medium` | Assault Carrier - Sakhai (p93) |
| Superior Stealth | `stealth: "Advanced"` | The Ghost of the Reach (p15) |

`Superior Stealth` has no equivalent in our `Stealth` enum (Basic/Improved/Enhanced/Advanced);
mapped to the highest tier we have. Consistent with the fusion-gun -> Particle mapping already
used for High Guard.

"Pop-up" turrets (Subsidised Merchant Type RQ, p18) are encoded as ordinary turrets — the
pop-up property has no schema representation.

## Mixed-turret resolution

Per the Core Rulebook rule (only one weapon type in a mixed turret may fire per round),
mixed turrets were regrouped into uniform turrets. **Five regrouped exactly**, conserving both
weapon count and mount count:

| Design | Book | Encoded |
|---|---|---|
| Indigo Pirate Carrier (p8) | 3x Triple (2 beam + 1 missile) | 2x Beam T3 + 1x Missile T3 |
| Buccaneer (p10) | 2x Double (1 sand + 1 pulse) | 1x Sand T2 + 1x Pulse T2 |
| Tender - OwatarL (p80) | 3x Triple (2 beam + 1 sand) | 2x Beam T3 + 1x Sand T3 |
| Slaver - Hkisyeleaa (p87) | 3x Triple (beam + missile + sand) | 1x each Beam/Missile/Sand T3 |
| Pocket Warship - Halaheike (p90) | 6x Triple (2 missile + 1 sand) | 4x Missile T3 + 2x Sand T3 |

**Four did not divide evenly.** Rule applied: regroup by majority; on a tie, split into
single-weapon turrets so no gun is invented or lost. **These four need your review:**

> **SUPERSEDED 2026-09-05** — the table below records what was originally encoded and the
> question that was asked. The user has since ruled on all four; see
> "RESOLVED — mixed turrets and pre-existing divergences" at the end of this file for the
> armament actually in the files today.

| Design | Book | Encoded | Effect |
|---|---|---|---|
| Gunship - Fiery (p12) | 1x Triple (2 sand + 1 beam) | `Sand T3` | majority sand; +1 sand, -1 beam, 1 mount |
| Scout - Hraye (p62) | 1x Double (1 pulse + 1 missile) | `Pulse T1` + `Missile T1` | tie -> split; guns conserved, 1 mount -> 2 |
| Courier - Ktiyhui (p66) | 2x Double: (pulse+missile), (sand+missile) | `Missile T2` + `Pulse T1` + `Sand T1` | majority missile; guns conserved, 2 mounts -> 3 |
| Light Trader - Aoa'iw (p72) | 1x Double (1 missile + 1 sand) | `Missile T1` + `Sand T1` | tie -> split; guns conserved, 1 mount -> 2 |

Note this differs from the **Cargo Carrier - MK Mora** decision (High Guard), where the user
chose to preserve mount count and accept +1 missile / -1 beam. If mount count should win over
gun count here too, Hraye / Ktiyhui / Aoa'iw would change.

## OPEN — divergences in the five pre-existing designs

These files came from this book but do not match it. **Not yet corrected**, pending decision:

> **SUPERSEDED 2026-09-05** — all five have been ruled on; see
> "RESOLVED — mixed turrets and pre-existing divergences" at the end of this file.
> The table below is kept as a record of the divergences as first found.

| File | Ours | Book |
|---|---|---|
| `star_ray_interceptor.json` | 2x Beam **Turret(3)** | "Double Turrets (beam lasers) x 2" = Turret(2) |
| `indigo_pirate_carrier.json` | crew 27 | crew box totals **17** (Captain, Pilot, Astrogator, Engineer, Gunners x3, Fighter Pilots x10) |
| `buccaneer.json` | 2x Pulse T2 + 2x **Sand T2** | 2x Pulse T2 + (1x Sand T2 + 1x Pulse T2) — we have 4 sandcasters where the book has 2 sand + 2 pulse |
| `ekawsiykua_escort.json` | tl **12**, crew 31 | tl **13**, crew 35 |
| `herald_fast_messenger.json` | matches exactly | — |

Note `buccaneer.json` is also the source of `impl Default for ShipDesignTemplate`
(src/ship.rs), so correcting it should include that Default impl.

---

# RESOLVED — mixed turrets and pre-existing divergences (2026-09-05)

The user has ruled on the eight open questions above. Both earlier tables ("These four need
your review" and "OPEN — divergences in the five pre-existing designs") are **superseded**
by this section; they are kept above only as a record of what was originally asked.

## Mixed turrets — final armament

The four Reach designs that did not regroup evenly were re-armed by user decision. The
guiding principle is **mount count wins over gun count** (same call as Cargo Carrier - MK
Mora), and mixed turrets are collapsed to a single weapon type rather than split.

| File | Book | Now encoded as |
|---|---|---|
| `gunship_fiery.json` | 1x Triple (2 sand + 1 beam) | `Particle Bay(Small)`, `Particle Barbette`, `Beam T3`, `Sand T3` |
| `scout_hraye.json` | 1x Double (1 pulse + 1 missile) | `Pulse T2` — one mount, all pulse |
| `courier_ktiyhui.json` | 2x Double: (pulse+missile), (sand+missile) | `Missile T2`, `Pulse T2` — two mounts, no sand |
| `light_trader_aoa_iw.json` | 2x Double beam + 1x Double (1 missile + 1 sand) | `Beam T2`, `Beam T2`, `Missile T2` |

Notes:

- **Gunship - Fiery** gains a full `Beam T3` alongside its `Sand T3` (previously the mixed
  triple collapsed to sand only). Its Particle bay and barbette are unchanged.
- **Scout - Hraye** loses its missile rack entirely; the single double turret is now all
  pulse laser.
- **Courier - Ktiyhui** drops the sandcaster; the two turrets are a missile double and a
  pulse double.
- **Light Trader - Aoa'iw** keeps its two existing beam doubles unchanged; the old
  `Missile T1 + Sand T1` pair is replaced by a single `Missile T2`.

## Pre-existing designs — final decisions

| File | Decision |
|---|---|
| `star_ray_interceptor.json` | **Kept as-is by user choice.** Our `2x Beam Turret(3)` stands against the book's `Turret(2)`; this is a deliberate divergence, not an error to fix. |
| `indigo_pirate_carrier.json` | `crew` **27 -> 17**, matching the book's crew box. Nothing else changed. |
| `buccaneer.json` | Weapons now `Pulse T2`, `Pulse T2`, `Pulse T2`, `Sand T2` (3 pulse doubles + 1 sand double), matching the book's gun mix. |
| `ekawsiykua_escort.json` | `tl` **12 -> 13**, `crew` **31 -> 35**, matching the book. |
| `herald_fast_messenger.json` | **No change** — already matched the book exactly. |

**`buccaneer.json` now diverges from `impl Default for ShipDesignTemplate` in
`callisto/src/ship.rs` by design.** That Default impl happens to be named "Buccaneer" but is
a synthetic test fixture used throughout `unit_tests.rs` and `entity.rs`; the user explicitly
chose to leave it alone. Do not "fix" the two to agree — the JSON file is the ship, the
Default impl is a fixture.

---

# Design metadata backfill (2026-09-05)

`role` and `source` were previously carried only by the 25 "Ships of the Reach" designs.
Both fields are now present on **all 79** designs in `ship_templates/`. Both are
`Option<String>` in `ShipDesignTemplate`, so older files without them still load; the
backfill just means nothing is ungrouped in the design picker any more.

## `source` values

| Value | Count | Which |
|---|---|---|
| `High Guard` | 43 | The Mongoose High Guard (April 2024) imports, plus the Core Rulebook designs that were updated to High Guard values under the "High Guard wins" ruling above |
| `Ships of the Reach` | 15 | Includes the four pre-existing files identified as Reach designs: `buccaneer`, `herald_fast_messenger`, `indigo_pirate_carrier`, `star_ray_interceptor` |
| `Ships of the Reach (Aslan)` | 15 | Includes `ekawsiykua_escort` |
| `Custom` | 6 | `excelsior`, `harrier`, `gazulin`, `stretched_trader`, `threshing_oar`, `void_trader` — designs of our own, not from any book |

## `role` values

The existing Reach vocabulary was reused rather than extended, with two additions:
**`Small Craft`** for non-jump-capable auxiliary craft (launches, boats, pinnaces, cutters,
shuttles, gigs) and **`Fighter`** for fighters and the torpedo boat.

Trader 12, Small Craft 10, Escort 8, Raider 6, Scout 6, Courier 5, Cruiser 4, Fighter 4,
Liner 4, Carrier 3, Support 3, Transport 3, Warship 3, Prospector 2, Research 2, Utility 2,
Gunship 1, Slaver 1.

Judgment calls worth knowing about:

- The three Custom armed ships used as pirates in `scenarios/` — `harrier` ("Killer" in
  *First Prize* and *Tutorial*), `threshing_oar` (the Oghman raiders in *Raiders Attack*)
  and `excelsior` (the aggressor in *Marduk Encounter*) — are all **Raider**, taken from how
  the scenarios actually use them rather than from their names.
- `safari_ship_type_k` and `yacht_type_y` are **Liner**: unarmed passenger carriers, and
  the closest existing label. There is no "Yacht" or "Charter" role.
- `system_defense_boat` and `system_defence_boat_dragon` are **Warship**, not Small Craft —
  they are jump-0 but 200t and 400t of armed hull, well past auxiliary-craft scale.
- `troop_transport` (50t, jump 0) is an assault shuttle, so **Small Craft** despite its name.
- `jump_shuttle` is **Transport**, not Small Craft — it has jump 3.
- `midu_agasham` (3,000t, jump 4, particle bay + 20 turrets) is **Cruiser**.
- `survey_scout_donosev` is **Scout** rather than Research; `laboratory_ship_type_l` is
  **Research**.

## Verification

- All 79 files parse as JSON and load as `ShipDesignTemplate`.
- Zero designs missing `role` or `source`.
- No duplicate `name` values across the directory.
- `cargo test --features ci,no_tls_upgrade`: 147 unit + 24 integration + 1 doc test, all
  passing. No test expectation needed changing — the unit tests that mention "Buccaneer"
  use the `Default` fixture, not `buccaneer.json`.

## Ktiyhui displacement corrected (book typo)

`courier_ktiyhui.json` displacement **100 -> 200 tons**. The stat table prints
"Hull 100 tons, Streamlined" but the prose on the same page reads "Using a heavily
armoured 200-ton hull", and the component tonnages only balance at 200:

| Component | Printed | At 100t | At 200t |
|---|---|---|---|
| Crystaliron Armour 12 | 30 t | 15 t | **30 t** |
| M-Drive Thrust 4 | 8 t | 4 t | **8 t** |
| J-Drive Jump 3 | 20 t | 12.5 t | **20 t** |

Hull points stay at the printed 88, which is 0.44 x 200 — the *reinforced* ratio,
consistent with "heavily armoured". At 200 tons the ship gets 2 hardpoints, so its two
double turrets are legal; at the printed 100 tons they would not have been.

## Bucket cleanup

`gs://callisto-ship-templates/default_ship_templates.json` and
`old_default_ship_templates.json` were deleted (2026-09-05). They were pre-per-file-rework
aggregates — JSON *arrays* of 16 and 7 designs — which the loader cannot parse as a single
`ShipDesignTemplate`, so they logged a parse error on every load and reload. Every design
inside them already exists as an individual file in both the bucket and the repo, so
nothing was lost. Backups: `~/callisto-bucket-backup-2026-09-05/`.

**Note:** the deployed services read designs from `gs://callisto-ship-templates`, not from
this directory, and **prod and canary share that bucket**. The 60 new designs are in git
but NOT in the bucket, so they are not live anywhere yet.

## Hardpoint compliance (corrected 2026-09-05)

High Guard p.26 and p.31: ships of 100+ tons get 1 Hardpoint per 100 tons; a Fixed Mount,
Turret, Barbette, Small Bay or Medium Bay each costs 1, a **Large Bay costs 5**, a Spinal
Mount costs tonnage/100. Ships under 100 tons use **Firmpoints** instead: 1 under 35 tons,
2 for 35-69, 3 for 70-99. A Firmpoint holds one weapon; exactly one may be upgraded to a
*single* turret; a Barbette consumes three Firmpoints.

A turret costs 1 hardpoint whether single, double or triple — which is why regrouping mixed
turrets while preserving **mount** count (the policy chosen here) also preserves hardpoint
usage, while splitting them into more mounts silently overruns the allowance.

Recomputed across all 79 designs, only two genuine overruns existed:

- **`indigo_pirate_carrier`** — 6 mounts on 3 hardpoints, because it still carried the old
  *split* encoding of "Triple Turrets (beam lasers x2, missile rack) x3". **Fixed:** now
  2x Beam T3 + 1x Missile T3, conserving 6 beam + 3 missile and fitting 3 hardpoints. This
  had been missed when the other mixed turrets were resolved.
- **`excelsior`** — 3 mounts on 2 hardpoints (particle barbette + missile double + sand
  single). A custom design, not from any book. **Still open.**

`heavy_fighter` and `troop_transport` initially appeared to break the "one turret per small
craft" rule but do not: the book gives each a single turret *plus a fixed mount*, and our
schema has no `FixedMount` — both encode as `Turret(1)`. See the design doc for the
recommendation to add that variant.

**Correction (2026-09-05):** `excelsior` is *not* an overrun. It is a particle barbette plus
one triple turret holding 2 missile racks and a sandcaster — 2 mounts, 2 hardpoints, legal.
Our schema cannot express a mixed turret, so it is stored as `Missile T2` + `Sand T1` and
counts as 3. Left as-is deliberately. With that understood and `indigo_pirate_carrier`
regrouped, **no design in the library breaks the hardpoint or firmpoint rules** — all four
apparent violations were schema gaps (no `FixedMount` variant, no mixed-turret support).
