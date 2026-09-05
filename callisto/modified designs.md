# Modified Designs — memo to ourselves

Ship designs in `ship_templates/` converted from **Mongoose Traveller High Guard (April 2024
update)**, pp. 136–232 (printed page numbers). Only hulls of **5,000 tons or less** were
converted; the system does not currently support larger ships.

Everything below records a place where the book's design could **not** be represented
faithfully in `ShipDesignTemplate` (`callisto/src/ship.rs`). These are not transcription
errors — they are gaps in our model. **We need to go back and fix these ASAP.**

---

## TODO 1 — Implement the missing weapon types (HIGH PRIORITY)

`WeaponType` is currently only `Beam | Pulse | Missile | Sand | Particle`, and
`WeaponMount` only `Turret(n) | Barbette | Bay(Small|Medium|Large)`. The book uses many
more. Weapons below were **substituted with the nearest analogue** to keep the ships armed:

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

## TODO 2 — Weapon modifiers are silently dropped

High Guard weapon modifications have nowhere to live in the schema. Dropped from:

| Design | Dropped modifiers |
|---|---|
| Military Gig - Close Escort Variant (p138) | intense focus, high yield |
| System Defence Boat - Dragon (p193) | size reduction x3 (on the Small Missile Bay) |
| Destroyer Escort - Fer-de-Lance (p211) | accurate, high yield |
| Merchant Cruiser - Leviathan (p217) | energy efficient x3 |
| Cargo Carrier - MK Mora (p223) | long range, high yield, accurate |

## TODO 3 — `add ship` needs weapon customization (HIGH PRIORITY)

The "add ship" flow currently pins a ship to its design's fixed weapon list. Real play needs
per-ship armament: a Free Trader or Far Trader may be fitted with almost anything, and 11 of
these designs ship with **empty mounts** the crew is expected to fill. This is not a small
feature — but it is why so many designs below have `"weapons": []`.

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

| File | Ours | Book |
|---|---|---|
| `star_ray_interceptor.json` | 2x Beam **Turret(3)** | "Double Turrets (beam lasers) x 2" = Turret(2) |
| `indigo_pirate_carrier.json` | crew 27 | crew box totals **17** (Captain, Pilot, Astrogator, Engineer, Gunners x3, Fighter Pilots x10) |
| `buccaneer.json` | 2x Pulse T2 + 2x **Sand T2** | 2x Pulse T2 + (1x Sand T2 + 1x Pulse T2) — we have 4 sandcasters where the book has 2 sand + 2 pulse |
| `ekawsiykua_escort.json` | tl **12**, crew 31 | tl **13**, crew 35 |
| `herald_fast_messenger.json` | matches exactly | — |

Note `buccaneer.json` is also the source of `impl Default for ShipDesignTemplate`
(src/ship.rs), so correcting it should include that Default impl.
