# Capital ships and carriers: plan

Phase 4 of the fleet-control epic (`docs/fleet_control_plan.md`). This document has three parts:
- what the books say about ships of 5,000–1,000,000 tons, spinal mounts and carriers;
- how Callisto should rule on each point, in the FAQ's style;
- the work, split into PR-sized steps.

It changes no game code.

**Decisions since the first draft (2026-10-29).** The user has settled every open question. They are recorded in §9, and the rulings below follow them:
- §2: no facing, as the Companion rules;
- §3, "What bites at 1,000,000 tons": the scale fixes, several of them already made;
- §4: rearming and reloading, with the rules as written first.

**Sources.**
- **HG** is *High Guard Update 2022* (the Apr 2024 PDF). Page numbers are the printed ones; the PDF page is one higher.
- **HG FAQ** is *High Guard Update 2022 FAQ*, the errata. Where it changes the book, Callisto follows it.
- **IN** is *Imperial Navy* (Corebook), printed pages.
- **EC** is *Imperial Navy: Element Cruisers*.
- **CRB** is the *Core Rulebook* 2022 update (the 11-12-2024 PDF), printed pages.
- **Companion** is the *Traveller Companion* 2024 update, printed pages.
- *Naval Campaign Sourcebook* was checked. Its "Resolving Combat" chapter (pp. 50–54) abstracts ground and boarding fights and says outright that it "is not a ship-to-ship combat system". Nothing in it applies here.

Status words are the FAQ's: **Change**, **Assumption**, **Interpretation/Decision**, **House rule**, **Addition**, **Omission**, and **Rule enforced** where Callisto does what the book says.

## Where Callisto is today

These are the facts the plan builds on. File references are to the `epic-fleet-control` branch.

- **No tonnage cap is enforced** anywhere. The largest design is the 5,000 t P.F. Sloan. The FAQ's "supports hulls up to 5,000 tons" is a statement of what has been built, not a limit in the code.
- **Hull points are stored, not derived.** `ShipDesignTemplate.hull` is a `u32` and there is no formula. A 1,000,000 t hull (666,667 HP) fits.
- **Weapons are a flat `Vec<Weapon>`,** addressed by index, and `MAX_SHIP_WEAPONS = 64` (`player.rs`).
  - The Wind-class strike carrier has 361 mounts. The Tigress has more than 900, including 430 small missile bays.
  - The only grouping there is lives in the frontend (`groupWeapons` in `lib/hardpoints.ts`).
- **Mounts:** `WeaponMount::{Turret, Barbette, Bay(Small|Medium|Large), FixedMount, Battery(type)}`. `Battery` is only the point-defence laser battery. There is no spinal mount.
- **Damage multiples** are Barbette ×3 and bays ×10/×20/×100 (`rules_tables::damage_multiple`). They are applied after armour and screens.
- **Small-target DMs** for bays and torpedoes are already enforced (HG p. 32, p. 39; the FAQ cites p. 31, one page early).
- **Criticals** follow the CRB:
  - a primary critical on Effect 6+;
  - sustained damage, one severity-1 critical per tenth of hull lost, counted across a single huge hit too.

  Nothing depends on the target's size.
- **Every missile is its own entity.** A large missile bay fires 120 a round, so one Tigress broadside would create about 5,000 entities. **Done** (step 3): a launch is one salvo entity with a count.
- **Screens are implemented:** meson screens and nuclear dampers (`ScreenType`, `roll_screen_pool`).
  - The FAQ's opening section still says they are not, and so do the headers of `docs/screens_design.md` and `docs/pd_batteries_design.md`. Step 1 fixes those words.
- **Small craft do not exist.** No docking, hangars, launch tubes or carried craft. The two "carriers" in `ship_templates/` (Sakhai, Ulfhednar) are their guns only.
- **The network** (`ai-ships/src/network/`) has:
  - 15 heads;
  - one Fire target per ship, with tactics firing every mount that bears;
  - only aggregate size features: hull share and `log10(hull)/4`;
  - 20 own and 20 enemy slots.

  Adding a head changes `HEAD_SIZES`, which `net.rs` and `side_net.rs` check, so the shipped `side.cside` must be retrained.

**A bug found on the way.** The HG FAQ changes the large particle beam bay's range from Distant to **Very Long** (HG FAQ p. 1, "Large Bay Weapons Table (page 33)"). Callisto still gives it Distant (`rules_tables.rs`, `LargeBay => … Range::Distant`), and the FAQ repeats it ("a particle beam only reaches Distant out of a large bay"). Step 1 fixes both.

---

## 1. Spinal mounts

### What the book says

- **The four types** (HG p. 35):

  | Type | TL | Range | Base size | Per multiple | Max size | Traits |
  |---|---|---|---|---|---|---|
  | Mass driver | 10 | Short | 5,000 t | +250 Power, +4D, +MCr1500 | 100,000 t | AP 15, Orbital Bombardment |
  | Meson | 12 | Long | 7,500 t | +1,000 Power, +6D, +MCr2000 | 75,000 t | AP ∞, Radiation |
  | Particle | 11 | Very Long | 3,500 t | +1,000 Power, +8D, +MCr1000 | 28,000 t | Radiation |
  | Railgun | 10 | Medium | 3,500 t | +500 Power, +4D, +MCr500 | 21,000 t | AP 20 |

  - A mount comes in whole multiples of its Base Size, and damage, Power and cost scale with the multiple. Example: a 15,000 t meson mount is 2,000 Power, 12D, MCr4000.
  - **Spinal Mount Improvements** (HG p. 35): +1, +2 or +3 TL above the weapon's TL takes −10%, −15% or −20% off the tons and adds +10%, +20% or +30% to the cost.
    - The published designs agree. The Wind-class's TL15 meson is 18,000 t, which is 3 × 6,000 t (HG p. 266). The Ghalalk's TL14 particle is 2,800 t, which is 1 × 2,800 t (HG p. 243).
- **Damage multiple ×1,000** (HG p. 29, p. 34). Damage is armour first, then the multiple (HG p. 29).
  - **Many dice may be rolled the short way:** a 60D shot can be rolled as 6D×10 (HG p. 35).
  - **Particle spinals:** "damage … is reduced by 3% per point of armour … before applying the Damage Multiple" (HG p. 36).
  - **Railgun spinals:** 2% per point, and they carry 5 rounds, each extra round 20 t (HG p. 36).
  - **Meson spinals** ignore all armour and radiation shielding (HG p. 35). Meson screens apply as normal (HG p. 35).
  - **Mass driver ammunition** is 50 t and Cr500,000 per attack (HG p. 35).
- **Target size** (HG p. 34):
  - DM−4 against 10,000 t or less;
  - DM−8 against 5,000 t or less;
  - "cannot attack targets of less than 2,000 tons unless they are stationary or are caught in the blast by accident".
- **Close range** (HG p. 35, Spinal Weapon Range Adjustments). The DMs grow with the *attacker's* size:

  | Range | Attacker <10,000 t | 10,000–50,000 t | 50,000–250,000 t | >250,000 t |
  |---|---|---|---|---|
  | Adjacent | −4 | −6 | −8 | −10 |
  | Close | −2 | −4 | −6 | −8 |
  | Short | −1 | −2 | −4 | −6 |
- **Fitting** (HG p. 34):
  - one Hardpoint per 100 tons, rounding up;
  - no larger than half the ship;
  - more than one is legal but "rarely done" (HG p. 35).
- **Criticals:** "Spinal mount weapons can always (and will!) cause critical hits" (HG p. 27).
- **Crew:** military crewing is 1 gunner per 100 tons of spinal weaponry (HG p. 23). IN p. 114 keeps that in full ("Spinal weapons need their full complement").
- **Aiming.** Spinals are "aimed by pointing the whole ship, with minimal aiming corrections possible" (IN p. 98).
  - HG gives the same reason for the close-range penalties: "their sheer size and dependency on the movement of the host ship" (HG p. 34–35).
  - No book has a facing rule for spacecraft. Facing is §2.
- **Power:** high-efficiency batteries are suggested as a design option (HG p. 35). No charge-time rule exists.
- **Readiness:** IN gives a spinal 4D+12 minutes from "weapons safe" to ready (IN p. 82). That only matters if a ship is surprised with its weapons cold.

### Callisto rulings

#### Rule enforced — spinal mounts as HG prints them
- **Size and damage come from the multiple.** A spinal is one mount of a type and a whole multiple. Tons, Power, damage dice and cost come from the table, with the improvement for how far the design's TL is above the weapon's. Range and traits are the type's.
- **The design is checked:**
  - multiple × base size, after the TL reduction, cannot exceed the type's maximum or half the hull;
  - one Hardpoint is used per 100 t.
- **Damage:** dice + Effect, less armour, then ×1,000. The size DMs and the "cannot attack under 2,000 t" rule are enforced.

#### Decision — all the dice are rolled
HG's divide-by-ten shortcut (p. 35) is there to save a referee rolling 60 dice. The server can roll them, and rolling all of them gives the right bell curve. The shortcut is a convenience, not a rule, so it is not used.

#### Interpretation — a railgun spinal has AP 20 *and* the 2% rule
The table gives AP 20 and the text gives "reduced by 2% per point of armour", without saying how they combine.
- **Callisto's reading:** the AP is taken off the armour first, and whatever armour is left costs 2% per point.
- **Why:** against armour 20 or less the railgun loses nothing, which is what AP 20 means anywhere else. Against heavier armour the percentage replaces subtraction, which is what the paragraph is for.
- **Particle spinals** have no AP, so they lose 3% per point of the full armour.
- **Floor:** both are capped at 100%, so damage can reach zero but never goes negative.

#### Rule enforced — a spinal fires at any bearing
There is no facing (§2), so a spinal bears on any target in range and costs no dodge or Assist Gunner. Its clumsiness is HG's size and close-range DMs.

#### Interpretation — "stationary" means no thrust last round
HG does not define a stationary target, and in Newtonian space nothing is at rest. Callisto rules that a target under 2,000 t is open to a spinal if it **spent no Thrust and took no Evasive Action in the previous round**. A ship drifting with dead drives, or simply holding still, can be hit. The DM−8 for 5,000 t or less still applies.

"Caught in the blast by accident" is not modelled. There is no splash damage.

#### Interpretation — the close-range table by distance, not by band
Callisto folds Adjacent and Close into Short (FAQ, "Point Defence reaches one band, then two"). Distance is continuous, though, so the spinal table can use the CRB's own limits:
- **Adjacent:** under 1 km;
- **Close:** 1–10 km;
- **Short:** 10–1,250 km.

Only spinal fire looks at these sub-bands. Everything else still sees one Short band.

#### Rule enforced — a spinal hit that does damage is always a critical
"Spinal mount weapons can always (and will!) cause critical hits" (HG p. 27). Callisto reads "will" literally:
- **Every hit:** a spinal hit that gets any damage through scores a primary critical.
- **Severity:** Effect − 5 as usual, but at least 1.
- **Size immunity does not apply:** the large-ship rules below never block it.

#### Decision — the spinal draws Power like any other weapon
A spinal's Power goes on the engineer's board with the other weapons (FAQ, "a power budget, with weapons in it"). A Wind-class's 3× meson is 3,000 Power, and shedding it is how a damaged plant keeps her flying.
- **Omission: charging batteries.** High-efficiency batteries that charge a spinal over several rounds (HG p. 35) are not modelled.
- **No charge time.** No rule gives one, so a spinal fires every round.

#### Addition — spinal ammunition is in the magazine
- **Railgun spinals** carry 5 rounds plus any extra stated.
- **Mass driver spinals** carry what the design states (50 t per attack).
- **The magazine counts both,** the same way it counts missiles and sand.

#### Rule enforced — the mass driver is a planet-killer
- **Against ships it is nearly useless:** Orbital Bombardment gives DM−12 against anything that can manoeuvre (HG p. 30).
- **Omission:** there are no ground targets yet, so it exists only so published bombardment designs can be built.

#### Decision — one gunner skill for the whole spinal
- **The crew count** is 1 per 100 t of spinal and goes into the design.
- **The roll** is the *trigger officer's* Gunnery, one skill on one mount, the same way a bay is fired today.
- **Crew criticals** reach the spinal through the large-ship crew table below. They do not hit one seat.

#### Omission — weapon readiness
Scenarios open at action stations. IN's readiness times (p. 82) would make a scenario option for an ambush on a cold ship. They are not planned.

---

## 2. Facing: none, as the Companion rules

**The user's decision (2026-10-29, second round):** Callisto follows the rules as written in the *Traveller Companion*. Ships have no facing and pivot freely within a round. This replaces the thrust-vector facing in the previous draft. There is no cone, no aft mounts, no launcher exemption and no greying out, and the facing step and its frontend and AI work are dropped.

### What the books say
- **The Companion ignores facing outright.** "Spacecraft facing is ignored. It is assumed that ships may pivot to any needed direction over the course of each six-minute space combat round, even when expending their full Thrust during a round" (*Traveller Companion* 2024, p. 169). It adds that facing "does not have a mechanical effect" (p. 174).
- **IN says turning is quick:** "Alignment can be changed almost immediately… a form of fine-tuning for the situation created by momentum", and ships "pitch, roll and yaw to bring different batteries to bear" (IN p. 85).
- **Fixed mounts nominally point forward:** "Fixed mounts can only be fired directly forward" (CRB p. 168; HG p. 28; CRB p. 183). A Firmpoint "may alternatively point in any desired direction" (HG p. 26, as the HG FAQ amends it).
- **Spinals are "aimed by pointing the whole ship"** (IN p. 98).
- **The only printed arc is for vehicles:** a 90° front arc (CRB p. 139). No book gives one for spacecraft or ties facing to thrust.

### Callisto rulings

#### Rule enforced — no facing (Companion p. 169)
- **Free pivot:** a ship can point any way in any round, whatever her thrust.
- **Every weapon bears on every target in range,** spinals and fixed mounts included.
- **What remains of the spinal's clumsiness** is HG's own: the size DMs, the 2,000 t limit, and the close-range DMs (§1).

#### Consequence — a fixed mount is a single-weapon mount
Since a ship can always turn her bow onto a target within the round, "directly forward" restricts nothing. A fixed mount keeps the differences it already has in Callisto:
- it cannot be used for point defence;
- the editor treats it as a mount of its own.

It gets no arc. The FAQ line that a fixed mount is "just a single-weapon mount" (FAQ, *Weapons*) stays true. The user accepts that, since giving fixed mounts special meaning isn't interesting.

---

## 3. Size: hulls, criticals, batteries, crew

### What the book says

- **Hull points** (HG p. 10):
  - 1 per 2.5 t;
  - 1 per 2 t for 25,000–99,999 t;
  - 1 per 1.5 t for 100,000 t and up.

  Modifiers stack on top:
  - Close Structure is +0%;
  - Dispersed Structure is −10% and Buffered Planetoid +50% (HG p. 11);
  - Reinforced is +10% and Light −10% (HG p. 12).

  The Military Hull option doubles maximum armour, for capital ships only (HG p. 12).
- **Critical hits on large ships** (HG p. 27):
  - over 2,000 t, criticals from turrets and barbettes are ignored;
  - over 10,000 t, all criticals are ignored except those from medium and large bays and spinals;
  - over 100,000 t, all are ignored except large bays and spinals;
  - sustained damage still causes criticals at every size.
- **The large-ship critical tables** (HG p. 27) are "needed when two large ships square off":
  - **Cargo and Weapons by severity:**
    - 1: 2D% disabled;
    - 2: D3×10% disabled;
    - 3: 1D×10% destroyed;
    - 4: all destroyed;
    - 5–6: all destroyed and Hull severity +1.
  - **Choosing what is hit:** "if the weapons are multiple … use the table", against one system chosen at random.
  - **Ships with little cargo:** over 5,000 t, the Cargo result is a D6 table for that ship, for example "1–2: fighters, 3: other small craft, 4: frozen watch, 5–6: cargo".
  - **Crew by severity:**
    - 1: 1% killed or disabled;
    - 2: 5%, and life support fails in 1D hours;
    - 3: 10%, DM−1 to all checks until the crew is back above 90%;
    - 4: 15%, life support fails in 1D rounds;
    - 5: 20%, DM−2 until above 70%;
    - 6: 25%, life support fails, DM−4 until above 25%.
  - **Frozen watch:** spare crew in low berths can be revived to replace crew lost to criticals, taking effect the following turn (HG p. 27).
- **Radiation on large crews** (HG p. 31): a cumulative DM−1 to all of the ship's actions for every 10% of Hull lost to Radiation weapons. With radiation shielding it is every 40%. Meson weapons ignore shielding.
- **Batteries** (IN p. 113):
  - turrets and barbettes of the same type can be grouped into batteries that "all fire at the same target", up to a maximum set by the hull:

    | Hull | Mounts per battery |
    |---|---|
    | 100–999 t | 4 |
    | 1,000–9,999 t | 8 |
    | 10,000–99,999 t | 16 |
    | over 100,000 t | 32 |

  - the usual way to resolve one is "a separate attack roll for each turret or barbette";
  - **harmonised:** DM+1 per four mounts, but only one mount's damage;
  - **focused:** one attack and one mount's damage, with +2 per damage die per four mounts;
  - bays "may or may not be organised as batteries" (IN p. 99);
  - a battery needs one gunner (IN p. 114);
  - HG's fleet rules use the same grouping: "weapons grouped in batteries of the same type" (HG p. 115–116).
- **Several sensor operators** (HG p. 34): one on duty per full 7,500 t, and each can use Electronic Warfare on a different incoming salvo.
- **Crew reduction** for ships over 5,000 t (HG p. 22) changes crew numbers only. It has no combat effect.
- **The command bridge** (HG p. 19) gives DM+1 to Tactics (naval), used for fleet initiative (HG p. 115).
- **The Fleet Battles chapter** (HG pp. 105–124) is an *alternative*, abstract resolution system. HG p. 105 says the standard space-combat rules "will suffice, no matter the tonnage of the ships involved … capital ships in the multi-100,000-ton range".

### Callisto rulings

#### Rule enforced — hull points follow the size bands
Designs keep their stated `hull`.
- **The test:** a check recomputes HG's figure from displacement, configuration and reinforced/light, and fails on any template that disagrees, unless the template is listed as an exception with the reason (a published typo, or a hull figure that includes modules, as the Ghalalk's 27,500 does).
- **The builder** shows the computed figure next to the stated one.

#### Rule enforced — big ships shrug off small guns
HG p. 27's thresholds apply to **primary** criticals. Sustained damage still gives criticals at every size.

This already changes things below 5,000 t. Today a laser turret can crit the 3,000 t Midu Agasham or the 5,000 t P.F. Sloan, and it should not. Designs of exactly 2,000 t (Sakhai, Ulfhednar) are not "larger than 2,000", so turrets can still crit them.

#### Interpretation — the large-ship critical tables apply to any target over 5,000 t
HG says to use them "when two large ships square off" and "on a large ship by another large ship". Callisto keys them on the **target** only.
- **Why:** the attacker's size changes little here. A small attacker can only crit a big ship through sustained damage or a big gun anyway. Judging "large attacker" for a destroyer with one medium bay would be arbitrary.
- **Size threshold:** 5,000 t, HG's own definition of a capital ship (HG p. 5) and the cargo table's own limit.

#### Rule enforced — weapons criticals on big ships hit a battery by percentage
Over 5,000 t, a Weapons critical of severity 2 or more works as follows:
- **What is hit:** one battery, bay or spinal, chosen at random by entry.
- **How much:** the Cargo and Weapons table decides what fraction of that battery is lost, at least one mount.
- **Severity 1** stays the CRB's DM−1 to attacks, as it is today.
- **Spinals:** a spinal is a single mount, so any severity of 2 or more puts it out.

#### Rule enforced — crew criticals on big ships are percentages
Over 5,000 t the crew table replaces the CRB row:
- **Crew lost:** recorded as a percentage of the complement.
- **DMs:** while the crew is below the thresholds, a DM−1, −2 or −4 applies to every crew check (attack, pilot, sensors, engineering, repair).
- **Life support:** failures are reported the same way they are today.

#### Addition — the frozen watch
- **The design field:** a `frozen_watch` count of spare crew in low berths.
- **Reviving:** the captain can revive them as a Leadership action. Next round the crew percentage rises by that many, up to the complement, and the crew-critical DMs lift as the thresholds are crossed.
- **Losing them:** a Cargo critical can destroy the frozen watch (see the carrier D6 table below).

#### Rule enforced — radiation wears down large crews
**The DM:** a ship carries a cumulative DM−1 to all crew checks:
- for every 10% of hull lost to Radiation-trait weapons;
- for every 40% if the design has `radiation_shielding` and the damage was not meson.

This is the first effect Callisto gives radiation. The FAQ's "has no effect on crews yet" changes to say so.
- **Every ship is affected.** HG frames the rule as a simplification for large crews, but it is written for all ships, and dropping it below some size would be an invented threshold.

#### Addition — batteries
**The design format.** A design may give a weapon entry a `count` (default 1). `{"kind":"Beam","mount":{"Turret":3},"count":200}` is 200 triple beam turrets.

**Forming batteries.** The engine splits each such entry into batteries no bigger than IN's limit for the hull (4/8/16/32). Each battery is **one weapon** to the rest of Callisto:
- one index;
- one gunner skill (IN p. 114: one gunner per battery);
- one target per round;
- one place in point defence, power and criticals.

`MAX_SHIP_WEAPONS` then counts batteries rather than mounts. The Wind-class becomes about 40 entries and the Tigress about 60.

**Which mounts.** Turrets, barbettes and bays can be batteries. Spinal mounts and point-defence batteries cannot.

**Interpretation — fire is resolved separately per mount**, IN's default ("the usual is to make a separate attack roll for each turret or barbette"). The server rolls each mount and the results are summed into one line: "Particle barbette battery (12): 7 hit, 213 damage, 1 critical".
- **Criticals:** each mount still has its own chance of one, subject to the large-ship limits.
- **Omission: harmonised and focused fire.** They can come later as a per-battery setting. Neither is needed to make big ships work.

**Decision — a missile battery is one salvo.** A battery of 32 small missile bays throws 384 missiles at one target as a single salvo (step 3).

**Decision — a laser-turret battery defends as a unit.** Assigned to point defence, it adds every mount's score to the pool. Assigned to attack, it gives none.

This is HG's fleet-rule split (HG p. 116), "some for point defence and some for attacks". The split is chosen battery by battery.

#### Addition — one Jam Missiles action per 7,500 tons
- **How many:** a ship has max(1, ⌊tons / 7,500⌋) sensor operators, each able to jam a different salvo (HG p. 34).
- **Which salvos:** the sensor station's action covers the first. The rest go automatically to the largest other incoming salvos at the same skill.
- **Why automatic:** a 75,000 t ship would otherwise need ten separate actions every round.

#### Change — no fleet-battle abstraction
Callisto plays HG's space-combat rules at every scale, which HG p. 105 allows. The Fleet Battles chapter is not adopted:
- its armour ÷3.5;
- its "damage under half armour does nothing";
- its fleet initiative and morale.

Two things are borrowed from it: batteries, and launching fighters during the manoeuvre step (see §4).

#### Change — the command bridge does nothing in play
Callisto has no initiative, so Tactics (naval) has nothing to modify (FAQ, "no initiative, so Tactics (Ship) does nothing"). A command bridge is still costed and built, and Phase 1's Squadron Command role can show it as the flagship's.

#### Omission — crew reduction
HG's capital-ship crew multipliers (p. 22) only change the crew count in the design. They are copied from the published designs and not computed.

#### Rule enforced — no special rules for small craft against large ships
Beyond the bay, torpedo and spinal DMs, and criticals ignored on large hulls, HG gives nothing more:
- no armour threshold outside fleet battles;
- no damage cap against fighters.

A medium bay hit on a 10 t fighter is overkill, and that is correct.

### What bites at 1,000,000 tons and beyond

The plan is built to a limit of **1,000,000 t for ships**. Space stations go further: HG p. 64 runs from 1,000 t up to a "1 billion tons – orbital highport". So nothing should break below 10⁹ t.

What I checked on `epic-fleet-control`. The rows marked **Done** are fixed on `capital-ships`, each in its own commit with tests:

| Where | What happens at size | Fix (step) |
|---|---|---|
| **Damage dice:** `combat::roll_dice_min(dice: u8) -> u8` | Refused more than 42 dice (42 × 6 > 255): it **logged an error and returned 0**. A 66D Tigress spinal would silently have done no damage | **Done** (`caa590be`). `roll_dice_min` totals into a `u16` (255 dice × 6 = 1,530). `damage_dice` stays `u8`: the largest legal spinal is the 20× mass driver at 80D. The `u8` check roller `roll_dice` saturates and logs instead of returning 0; every caller asks for at most 6D |
| **Hull and displacement:** `ShipDesignTemplate.{displacement, hull, power, fuel, crew}` are `u32` | Fine up to 4.29 × 10⁹. A 10⁹ t station has about 6.7 × 10⁸ HP | None |
| **Hull-point formula** (HG p. 10) | 1 per 1.5 t above 100,000 t. HG gives stations no other rule, so the same bands are assumed | The step-1 check uses this |
| **Spinal damage** | 66D, max 396, + Effect, × 1,000 is about 4 × 10⁵. Fits `u32` | Do the arithmetic in `u64` anyway (step 5) |
| **Salvo size:** `Salvo::Fixed(u16)` | A 32-bay large-missile battery is 3,840 missiles; `u16` holds 65,535 | **Done** (step 3): `Missile.count` is a `u16`, and a battery's salvo saturates rather than wrapping |
| **Magazine:** `u32` | Fine | None |
| **`MAX_SHIP_WEAPONS`** (`player.rs`) | Was 64. The Tigress has more than 900 mounts, and batteries bring her to about 40–60 entries | **Done** (`dd736dfc`): 256. No frontend mirror exists. Batteries (step 2) count against it |
| **Crit thresholds:** `ceil(max_hull/10)` in `u64` | Fine. A single spinal hit can cross several tenths, so sustained-damage criticals come in bunches | Intended |
| **Critical-hit size bands** (HG p. 27) | Top out at "larger than 100,000 tons". There is nothing for stations beyond that | Use the same bands |
| **Battery size** (IN p. 113) | Tops out at 32 per battery "over 100,000" | Same at every size |
| **Sensor operators** (HG p. 34) | One per 7,500 t: 133 at 10⁶ t, 133,333 at 10⁹ | Cap the automatic extra jams at the number of salvos actually incoming |
| **Crew-critical percentages** (HG p. 27) | Already relative, so they scale | None |
| **Crew reduction** (HG p. 22) | The table stops at "100,000+ (33%)" | Nothing in play depends on it |
| **Prompt arithmetic:** `ai-ships/src/prompt.rs` `100 * hull / design hull` in `u32` | Overflowed above 42.9 M HP, about 64 M t | **Done** (`23b31a25`): `hull_percent` in `u64`, and the side's hull sums in `u64` |
| **Hull sums:** `admiral.rs`, `fleet.rs` (squadron progress), `network/commander.rs`, `network/encode.rs` row 79 | Overflowed at 4.29 × 10⁹ HP | **Done** (`23b31a25`): summed in `u64`. The network's ratio is unchanged. `rl-env/src/game.rs` reward sums are still `u32`: training scenarios never come near it |
| **Network size features:** `log10(hull)/4` (6 places in `encode.rs` and `side.rs`) | Gave 1.45 at 10⁶ t and 2.2 at 10⁹ t. Armour is already clamped (`.min(20)/20`), and the hull shares are capped at 1 | **Done** (`23a41fb0`): `hull_size()` caps at 1.5, which is 10⁶ hull points. A test checks every shipped design gets the identical bits. Stations still belong in retrain 1's scenario mix (step 8) |
| **Expected-damage features** | Are shares of the target's hull, so they scale | None |
| **Power:** `power * 10 / displacement` in `u32` (`thrust_from_power`, `best_thrust`) | Wrapped for a station's Power above about 4.3 × 10⁸ | **Done** (`23b31a25`): in `u64`. Power-board draws (`hull/10 × thrust`) fit `u32` to 10⁹ t |
| **Hardpoints:** `hardpoints.ts` `floor(displacement/100)` | 10,000 at 10⁶ t. Fine in JS | Show with thousands separators |
| **Frontend numbers** | Hull and damage print as raw integers, e.g. "666667" | Left as is: readable, not broken. `toLocaleString` can come with the step-7 designs |
| **Plot geometry** | A million-ton hull is about 2–3 km long, against a 250 km `IMPACT_DISTANCE` and a 1,250 km Short band. Fine as a point | None |
| **Performance: entities** | 300 fighters per carrier (Antiama, Tigress), and 120 missiles per large bay (fixed by step 3) | Phase 3's 200-ship test with these designs (step 7) |
| **Performance: detection** | O(n²) pairs each round. 600 ships is about 360,000 checks | Measure. Skip pairs already in contact (most of them), and cache per team |
| **Performance: flight solver** | One solver call per AI ship per round. 300 fighters means 300 calls | Measure first. The fighters of a squadron share a goal, so a solved plan can be reused for ships at the same spot |
| **Performance: network passes** | One pass per 20 ships | 600 ships is 30 passes. Fine |
| **Performance: wire update size** | Every ship goes to every client each round | Phase 3 measures this. Send deltas, as Phase 3 plans |

The user's ruling on fighters (each stays its own ship) means performance work comes before any grouping. Missiles are the bigger issue, and step 3 handles them first.

---

## 4. Carriers and small craft

### What the book says

- **Docking clamp** (HG p. 57), carried on the outside of the hull:
  - Types I–V take craft of 1–30, 31–99, 100–300, 301–2,000 and 2,001+ tons, at 1, 5, 10, 20 and 50 t for the clamp;
  - "It takes three full rounds to release or clamp a ship of less than 2,000 tons, during which time neither ship can expend any Thrust or make any attack rolls. Larger ships take D3+3 rounds";
  - Thrust is recalculated on the combined tonnage;
  - "ships held in a docking clamp can be targeted separately".
- **Docking space** (HG p. 61):
  - the craft's tons +10%;
  - 1D minutes to enter or leave;
  - in combat, "It takes D3 rounds to release or recover a ship of less than 2,000 tons, during which time neither ship can expend Thrust or make attack rolls. Larger ships take 1D rounds".
  - No room for maintenance (IN p. 97).
- **Full hangar** (HG p. 61):
  - twice the craft's tons;
  - repairs and maintenance aboard;
  - 2D minutes to enter or leave.
- **Launch tube** (HG pp. 61–62):
  - 10 × the largest craft's tons, 1 Power per ton;
  - each craft also needs a docking space or hangar.
  - The FAQ changes the timing to "A launch tube can release up to 10 craft per space combat round". The book's extra round to move into firing position is dropped.
  - After launch, "both the mothership and launched spacecraft may expend Thrust and make attack rolls during the same round".
- **Recovery deck** (HG pp. 62–63):
  - 10 × the largest craft's tons, 1 Power per ton;
  - the FAQ changes the rate to "A recovery deck recovers up to 10 craft per round" (the book says one);
  - open to vacuum, so it is not a hangar.
- **The landing check** (HG p. 104), on carriers with a recovery deck:
  - Routine (6+) Pilot, DEX;
  - the craft must be within Short range of the carrier at the start;
  - DM−1 for every point of Thrust spent by the craft *or* the carrier that round;
  - DM−1 for every other craft landing that round, unless all are in the same squadron;
  - **failure:** 1D × (the sum of both ships' Thrust) damage to both, ignoring armour.
- **Launch timing in fleet play:** fighters launch during the manoeuvre step, "almost invariably … from launch tubes" (HG p. 116).
- **UNREP** (HG p. 63): 20 t per ton of system per hour. That is far too slow to matter in a fight.
- **Fighter squadrons** (HG pp. 102–103):
  - up to about 12 fighters within Close range fly as one ship and pool their hull;
  - lose one fighter per (total ÷ count) hull, and one per critical;
  - fly at the worst fighter's performance;
  - make one attack per weapon type at DM+1 per fighter after the first, with damage as for a missile salvo (×Effect);
  - fighters with lasers may give point defence to ships within Adjacent or Close range (HG p. 104).
- **Published carriers** (HG):

  | Ship | Size | Craft | Launch and recovery | Pages |
  |---|---|---|---|---|
  | Skimkish light carrier | 29,000 t | 77 × 50 t docking spaces | 1 launch tube, 1 recovery deck | pp. 234–235 |
  | Wind strike carrier | 75,000 t | 80 light fighters in 10 t docking spaces | 1 tube and 1 deck, each 100 t | pp. 265–268 |
  | Antiama fleet carrier | 100,000 t | 300 heavy fighters | 3 tubes, 3 decks | pp. 269–270 |
  | Tigress dreadnought | 500,000 t | 300 heavy fighters | 3 tubes, 3 decks | pp. 283–284 |

  - The Element-class **hangar pod** (EC p. 23) is 2,600 t: 24 light fighters and 6 launches, 2 × 20 t tubes and 4 × 20 t decks, launching "twenty … simultaneously and four recovered at once". That is the FAQ's rates, at 10 per tube and 1 per deck under the book's wording.

### Callisto rulings

#### Addition — craft aboard are not on the plot
A craft aboard a carrier is not an entity in space. It is stored inside its carrier with its whole state: hull, criticals, crew, magazine, orders. It cannot be detected, targeted or ordered.
- **Launching** puts it into `Entities.ships`.
- **Recovery** takes it back out.
- **The carrier's role:** it shows its embarked craft and owns the actions that move them.

#### Rule enforced, with a change — launching
- **Who orders it:** the carrier's captain picks the craft to launch. Flight operations is a command job (IN p. 97), so it belongs to the Captain.
- **Launch tubes:** up to 10 craft per tube per round (HG FAQ).
  - **Change:** craft launched in a round appear **at the end of that round**, at the carrier's position and velocity and a little ahead of her (the way missiles appear today). They act from the next round.
  - **Why:** RAW lets them thrust and fire in the round they launch. In a simultaneous game nobody has plotted a craft that did not exist when orders were written, so the round is lost. The carrier is not delayed at all.
- **Docking space or full hangar without a tube:** D3 rounds for craft under 2,000 t, 1D for larger ones.
  - During that time the carrier spends no Thrust and makes no attacks, and the craft emerges at the end of the last round.
  - **Interpretation:** HG prints the round count under docking space only. A full hangar's "2D minutes" against docking space's "1D" shows it is the slower of the two, but not by rounds, so Callisto uses the same D3/1D for both.
  - **Several craft:** they can launch from their own docking spaces in parallel. The carrier is pinned until the last one is out.
- **Docking clamp:** release takes 3 rounds (D3+3 for 2,000 t and up), and neither ship thrusts or attacks.
  - **Thrust:** a ship with something clamped flies at the Thrust its drive gives the combined tonnage.
  - **Targeting:** the clamped ship can be targeted on its own. It is an entity on the plot that moves with its carrier.

#### Rule enforced, with an assumption — recovering
- **Who orders it:** the craft's pilot orders *Land on <carrier>*, and resolution follows HG p. 104.
- **When:** the check is made at the **end** of the round's movement.
  - **Range:** the craft must be within Short of the carrier.
  - **Velocity:** relative velocity must be no more than **the craft's Thrust × 60 seconds**, about one minute's burn.
    - **Assumption:** HG's abstract rules never mention relative velocity. In a Newtonian game a craft passing at 30 km/s is not landing, and the flight solver's rendezvous already makes a craft that is homing on its carrier match velocity.
- **The check:** Routine (6+) Pilot.
  - DM−1 for every G of Thrust either ship spent that round;
  - DM−1 for every other craft landing on that carrier that round that is not in the craft's own flight.
- **Success:** the craft is aboard, if a deck has room this round (10 per recovery deck). A craft that finds no room simply waits.
- **Failure:** 1D × (the craft's G + the carrier's G) damage to both, ignoring armour, and the craft stays outside.
- **Without a recovery deck:** recovery is through docking space or hangar, using the same D3/1D rounds as launching. The carrier must hold still (no Thrust, no attacks) until it is done, and there is no Pilot check.

#### Decision — follow the FAQ's rates
- **Tubes:** 10 craft per round.
- **Recovery decks:** 10 per round. The book's one a round would make the Wind-class take 80 rounds to recover her fighters.

The Element pod's "four recovered at once" with four decks is the book's original rate. EC was written against it.

#### Decision — rearming from the carrier's magazine
See "Reloading and rearming: decided" at the end of this section: one round aboard, in any berth, drawn from the carrier's stores.

#### Rule enforced — big carriers' cargo criticals hit their craft
Over 5,000 t, a Cargo critical rolls on a D6 table built from what the ship carries, on HG p. 27's pattern:
- 1–2: fighters;
- 3: other craft;
- 4: frozen watch;
- 5–6: cargo.

The severity table decides what fraction is lost, and a craft "destroyed" aboard is gone.

#### Rule enforced — the carrier's loss is her craft's loss
Craft aboard a destroyed carrier are destroyed. Craft on a carrier that jumps go with her.

#### Decision — launched craft join the carrier's air group
- **The squadron:** launched craft join a squadron named after the carrier ("Wind air group"), on her team, and take that squadron's orders.
- **The cap:** past the 20-ship squadron cap, they spill into "Wind air group 2" and so on.
- **The scenario author** can name the squadrons instead.

#### Decision — each fighter is its own ship
The user decided that the HG p. 102 squadron abstraction is not used. The Antiama launching 300 heavy fighters puts 300 ships on the plot.

If that is slow, the answer is optimisation, not grouping:
- missiles as salvos (step 3);
- detection, solver and wire costs measured and cut (step 7).

#### Omission — battle riders and tenders
The Hadrian battle rider (HG pp. 254–255) is a 50,000 t ship meant to ride a tender. Docking clamps support riding in general (a Type V clamp takes 2,001 t and up), but no tender is planned.

#### Omission — UNREP, construction decks, breakaway hulls, docking-space boarding
Not combat-relevant at Callisto's timescale.

### Rearming: rules as written

**What the books say:**
- **Magazines and storage.**
  - Turret racks hold 12 missiles (CRB p. 168, HG p. 28), and racks on Firmpoints hold four (HG p. 28).
  - Missile bays hold 12 salvos (HG p. 33), and torpedo bays the same (HG p. 34).
  - Extra stock is bought by the ton: "Twelve missiles consume one ton … missiles can be replenished after use" (HG p. 36). Three torpedoes are a ton (HG p. 38) and twenty sand canisters a ton (HG p. 38).
  - HG has no separate "magazine" component. Spare ammunition is cargo-like tonnage in the design (EC's cruisers list a "Missile Magazine (600 missiles)", 50 t, EC pp. 31–32).
- **Reloading during combat.**
  - "It takes one round to reload a missile rack" (CRB p. 168), and the same for a sandcaster (CRB p. 168).
  - *Reload Turret (Gunner)*: "A turret may be reloaded by any Traveller with the Gunner skill but it will be unable to make any attacks in that round" (CRB p. 172).
  - These are rules for a ship reloading **its own** turret from **its own** stores. Nothing in RAW covers reloading another craft.
- **Where a carried craft can be worked on.**
  - **Docking space:** "barely enough room for crew and passengers to scramble aboard. Most repairs and maintenance require the craft to be launched first" (HG p. 61). IN agrees: "There is no room to conduct maintenance in a docking space, requiring a return to base in order to service the craft" (IN p. 97).
  - **Full hangar:** "allows for repairs and maintenance of the craft when it is on board its mother ship" and "includes specialised testing and repair equipment" (HG p. 61). IN's carriers "rotate [craft] through the hangars for inspection and minor repairs" (IN p. 97).
  - **Recovery deck:** "may be used to store small craft … but it cannot function as a full hangar since it is open to vacuum" (HG p. 63).
  - **Launch tube:** only launches, and "each craft … must have a docking space or full hangar" (HG p. 62).
  - **None of them mentions rearming,** or gives a time for it.
- **Time.**
  - **No rule gives a turnaround time** for rearming or refuelling a fighter.
  - The only timings are for getting in and out: docking space 1D minutes, full hangar 2D minutes, and in combat D3 or 1D rounds through a docking space (HG p. 61).
  - IN mentions "a long turnaround time" only for a fleet's approach (IN p. 91).
- **Fuel.**
  - Only jump drives and power plants use fuel (HG p. 18).
  - Power-plant fuel is "10% of their size … per month of operation" (HG p. 18), so a fighter has weeks of endurance.
  - Refuelling a ship takes "1D hours" at a starport (CRB p. 156).
  - UNREP moves "20 tons of fuel, cargo or ordnance every hour" per ton of system (HG p. 63).
  - **Fuel is irrelevant over a fight of a few hours.**
- **Crew.**
  - A military carrier carries 1 pilot per small craft and an engineer per 35 t of the craft's drives and power plant (HG p. 23).
  - Maintenance crew are counted per 500 t "of ship and contained small craft" (HG p. 23).
  - IN's flight operations officer is responsible for "launch and recovery, but also maintenance and repairs" (IN p. 97).
- **The Element hangar pod** (EC p. 27) is the most concrete source:
  - "Up to 24 support crew can work on the fighter complement at once, using the ship's open-plan workshop";
  - "It is possible to keep a 48-fighter group in operation almost indefinitely … providing sufficient spares and supplies are available";
  - a small UNREP system lets fighters "replenish life support supplies without entering the recovery/hangar cycle".

  That is description, not a rule with numbers.

**In short:** RAW lets a carrier hold spare missiles as tonnage and lets a hangar maintain craft. It gives **no rule** on whether a carried fighter can be rearmed during a battle, how long it takes, or whether a docking space allows it. The closest mechanic is the CRB's one-round rack reload. Anything Callisto does here is an Addition.

### Rearming: the options that were considered

| Option | What happens | For | Against |
|---|---|---|---|
| **A. No rearming in a scenario** | A recovered craft keeps what it has left | Closest to RAW's silence. No new data | Recovering craft becomes only about repairs and safety. A missile fighter is one-shot |
| **B. Free restock in a full hangar** | One full round aboard in a hangar restores the craft's magazine. A docking space or recovery deck does nothing | Simple, and follows the hangar/docking-space split RAW does make (HG p. 61) | Carriers never run dry. Ignores the carrier's stores |
| **C. Restock from the carrier's magazine** | As B, but the missiles and sand come out of the carrier's own `magazine`, a design field. A carrier with no spares cannot rearm | Uses HG's tonnage-based storage (p. 36) and the existing `Magazine`. Carriers can run out, which is a real constraint | Every published carrier needs a stated spare load. Most HG designs give none, so we need a default (e.g. a ton per 12 craft-missiles) |
| **D. As C, with a turnaround time** | Rearming takes N rounds per craft, with only so many crews at a time (for example the EC pod's 24 support crew, or the HG maintenance count) | The most realistic. Makes hangar capacity matter | Time and crew numbers are invented. More state, and the network needs a "rounds until ready" feature |
| **E. Rearm anywhere aboard, one round** | The CRB's one-round rack reload, applied to any berth | Mechanically simplest, and echoes CRB p. 168 | Contradicts HG p. 61 and IN p. 97, which say there is no room to work in a docking space |

The user chose **C**, with a one-round turnaround (from D's idea) and any berth. See below.

### Reloading and rearming: decided

The user decided this on 2026-10-29. It covers every ship's own magazine-fed mounts as well as carried craft, so it is more than a carrier rule. Option C of the RAW options above is the one taken for craft.

**How Callisto works today:**
- **The magazine is one ship-wide stock.** `Magazine { missiles, torpedoes, sand }` lives on the ship (`ship.rs`). A launcher fires straight out of it, there is no load per mount, and nothing ever reloads.
- **Where the stock comes from:** a design that states a magazine keeps it. One that does not is given exactly what its racks hold (12 missiles per rack, 3 torpedoes per launcher, 20 barrels per sandcaster; `Magazine::for_design`), which is no spare stock at all. A reset restocks.
- **Stale FAQ text:** "Ammunition is not tracked. Missile racks, torpedo tubes and railguns never run dry or reload" (FAQ, *Weapons*) contradicted the FAQ's own *Addition — magazines*. Step 4 rewrote both.

#### Addition — a mount holds a load; the magazine is the spare stock
- **Every launcher and sandcaster mount** carries `loaded`: what is in the mount ready to fire. Its capacity is the book's:

  | Mount | Holds | Source |
  |---|---|---|
  | Turret missile rack | 12 missiles | HG p. 28; CRB p. 168 |
  | Firmpoint missile rack | 4 missiles | HG p. 28 |
  | Missile barbette | 25 missiles (5 salvos) | HG p. 31 |
  | Torpedo barbette | 3 torpedoes | HG p. 31 |
  | Missile and torpedo bays | 12 salvos | HG pp. 33–34 |
  | Hardpoint sandcaster | 20 canisters | HG p. 29 |
  | Firmpoint sandcaster | 4 canisters | HG p. 29 |

- **The ship's `magazine` becomes what is left in stores.**
- **Designs stay as they are:** a stated magazine is the ship's *total*. On load, the mounts are filled first and the rest is spare, so no design JSON changes and no ship carries more or less than today.
  - A design with no stated magazine has exactly its mounts' worth and no spares, so its racks go dry for good. That is a change of feel, since today such a ship's single pool is shared across its racks, but the total is the same.
- **Railgun and mass-driver rounds are not part of this,** per the user: only launchers and sandcasters.

#### Rule enforced, with an addition — reloading a mount
- **RAW:** "It takes one round to reload a missile rack" (CRB p. 168), and a sandcaster the same (CRB p. 168). *Reload Turret (Gunner)*: a turret "may be reloaded by any Traveller with the Gunner skill but it will be unable to make any attacks in that round" (CRB p. 172).
- **The ruling:** a reload refills one mount from stores, up to its capacity or what stores have, at the end of the round.
- **Spare gunners reload for free.**
  - **Who counts:** the crew's `gunnery` list is index-aligned with the mounts, so entries beyond the number of mounts are gunners with no mount of their own.
  - **How many:** each spare gunner reloads one mount a round, **when the gunnery station chooses Reload for it** (user, 2026-10-29). Nothing reloads by itself.
  - **The order of the pool:** each Reload chosen takes a spare gunner while any are free. A mount reloaded by a spare gunner still fires (or defends) with its own gunner that round, so a mount can **fire and reload** in one round -- if Reload is chosen before it runs dry.
  - **The gunner card:** the card shows each mount's crew as one entry, even when the mount needs several gunners. "Spare" means gunners beyond those cards, which is what the `gunnery` list already records.
- **With no spare gunner left, reloading is the mount's action for the round.** The Reload that finds the pool at 0 uses the mount's own gunner, so that mount makes no attack that round, as CRB p. 172 says.
  - **Assumption:** it makes no reaction either (sand, point defence). The gunner is busy with the reload, and one mount gets one use a round (FAQ, *Mixed turrets*).
- **What can be reloaded:** only mounts with a launcher (missile, torpedo) or a sandcaster that are below capacity, on a ship with stores left. A mixed turret (say, laser and rack) that reloads gives up its laser shot.

**How the choice is offered: Reload is chosen like Point Defence.**
- **Today:**
  - the gunner card (`WeaponUse.tsx`) lists a pseudo-target, `POINT_DEFENSE_NAME = "<Point Defense>"`, alongside enemy ships;
  - choosing it sends a `ShipAction::PointDefenseAction { weapon_id, protecting }` (`action.rs`) instead of a `FireAction`;
  - the merge in `action.rs` (the `FireAction` / `PointDefenseAction` arms around ll. 491–532) keeps one of the two per `weapon_id`.
- **New:**
  - **Gunner card:** a `"<Reload>"` choice, offered only on launcher and sandcaster mounts that are below capacity, on a ship with stores left. It shows "Reload (6 of 12 loaded; 30 in stores)".
  - **The spare pool on the card:** the card heads the mounts with "Spare gunners: 2 of 3 free". Choosing Reload while any are free takes one, and the mount keeps its target ("reloaded by spare crew"). At 0 free, Reload replaces the mount's target ("reloads instead of firing"). Withdrawing a Reload gives its spare gunner back.
  - **Wire:** `ShipAction::ReloadAction { weapon_id }`, mirroring `PointDefenseAction`. It is **not** in the one-per-`weapon_id` merge with Fire and Point Defence: the server gives spare gunners to Reloads in the order they were chosen, and only a Reload with none left drops that mount's Fire or Point Defence.
  - **Resolution:** reloads are applied after fire and before movement.
  - **Leadership:** boosts have nothing to apply to a reload, so it gets no `BoostTarget`.
- **AI:**
  - `tactics::Intent` gains `reload: Vec<usize>`, like `defend: Vec<usize>`: the mounts to reload this round. `tactics::orders` turns it into `ReloadAction`s and leaves those mounts out of fire and defence.
  - **The heuristic in tactics:**
    - while spare gunners are free, reload below-capacity launchers and sandcasters, emptiest first, since it costs nothing;
    - with none free, an empty single-kind mount is reloaded with its own gunner, and a partly empty one only when nothing it could fire on is in range;
    - a mixed mount is reloaded only when its other gun has no target.
  - **No new head.** The side network gets an own feature, the loaded share of launcher capacity, at retrain 1.
  - **The admiral** sees "racks 4/12, stores 30" in the design line.

#### Decision — carried craft reload aboard their carrier, from the carrier's magazine (option C)
- **Which craft:** a fighter with no stores of its own (most of them: its racks are all it carries) reloads only aboard its carrier.
  - It returns, lands (§4, recovery), and spends **one full round aboard**.
  - In that round its mounts are refilled **from the carrier's magazine**, by the carrier's crew, at no action cost to anyone.
  - Then it launches again as usual.
- **Assumption — any berth will do.** Docking space, full hangar or recovery deck all work.
  - HG p. 61 and IN p. 97 keep *maintenance and repairs* to a full hangar, and that still applies to repairs.
  - Reloading a rack is the CRB's one-round job, not maintenance.
- **Stores run out:** a carrier whose magazine is empty cannot rearm her craft. Carrier designs in step 7 state spare stores, from the design's own ammunition entries where HG lists them, otherwise set by the scenario author. With nothing stated, a carrier has no spares.
- **Fuel is not tracked:** a fighter's power plant runs for weeks (HG p. 18).
- **AI:** a carried craft with empty racks and its carrier in reach prefers *recover*. That is the `Flight` head (§7), and the heuristic in step 11 before it.


---

## 5. Other High Guard rules that matter at fleet scale

| Rule | Book | Callisto | Plan |
|---|---|---|---|
| **Meson screens, nuclear dampers** | HG p. 41, HG FAQ p. 2 | Implemented | Fix the FAQ's opening section and the two stale doc headers (step 1) |
| **Black globe generators** | HG pp. 41–42 | Missing | Omission. A referee's artefact, not something that can be bought |
| **Repulsor as tractor beam** | HG p. 34 | Point-defence half only | Omission. It needs a "held" state on a ship |
| **Missile salvo grouping** | CRB p. 169; HG p. 33 bays | One entity per salvo, each missile rolling to hit | **Done** (step 3) |
| **Large-salvo decay:** halve every five rounds of travel; torpedoes are exempt | HG p. 37 | Missiles burn 10 rounds | Omission. Callisto's burn limit already caps endurance |
| **Advanced warheads** (nuclear vs dampers, anti-torpedo, multi-warhead) | HG pp. 36–40 | Standard warhead only | Omission, as in the FAQ. Dampers do nothing against missiles until nuclear warheads exist |
| **Fighters giving point defence to nearby ships** | HG p. 104 | Point Defence software lets a ship cover another at Short (/1) or Medium (/2) | Fighters already do this through software. No change |
| **Firing into dogfights** | HG p. 104 | No dogfights | Out of scope, as in the FAQ |
| **Command bridge, flagship morale, fleet manoeuvres** | HG pp. 19, 115–124 | Not used | Out of scope (§3) |
| **Boarding** | HG pp. 125–134 | — | Out of scope, as in the FAQ |
| **Large particle bay range** | HG FAQ p. 1 | Distant | Bug. Becomes Very Long (step 1) |

---

## 6. Data model

### Design JSON, by example

```jsonc
{
  "name": "Wind-class Strike Carrier",
  "displacement": 75000,
  "hull": 41250,               // checked against HG p. 10 (step 1)
  "armor": 11,
  "radiation_shielding": true, // new: HG p. 31
  "frozen_watch": 0,           // new: spare crew
  "weapons": [                 // illustrative; PD battery type not checked
    { "kind": "Meson",   "mount": { "Spinal": 3 } },              // new: multiple; arc defaults to "Fore" (§2)
    { "kind": "Missile", "mount": { "Bay": "Small" },  "count": 32 },  // new: count
    { "kind": "Repulsor","mount": { "Bay": "Medium" }, "count": 9 },
    { "kind": "Beam",    "mount": { "Turret": 3 },     "count": 200 },
    { "kind": "Sand",    "mount": { "Turret": 3 },     "count": 100 },
    { "kind": "Fusion",  "mount": { "Turret": 2 },     "count": 10 },
    { "kind": "Particle","mount": { "Turret": 1 },     "count": 10 },
    { "kind": "PointDefense", "mount": { "Battery": 3 }, "count": 8 }
  ],
  "features": [
    { "name": "Docking space (10 t) ×80", "kind": { "DockingSpace": { "craft_tons": 10, "count": 80 } } },
    { "name": "Launch tube (10 t)",       "kind": { "LaunchTube":   { "craft_tons": 10 } }, "power": 100, "default_on": true },
    { "name": "Recovery deck (10 t)",     "kind": { "RecoveryDeck": { "craft_tons": 10 } }, "power": 100, "default_on": true }
  ],
  "complement": [ { "design": "Light Fighter", "count": 80 } ]  // default embarked craft
}
```

- **Mount:** `WeaponMount::Spinal(u8)` holds the multiple. The weapon's TL, and so the improvement, comes from the design's `tl`.
  - Profiles: `weapon_profile(kind, MountClass::Spinal)` gives the base and `spinal_profile(kind, multiple, design_tl)` scales it.
  - `damage_multiple` returns 1,000.
  - A `SpinalArmour::{Subtract, Percent(3), Percent(2)}` field on the profile carries the particle and railgun rules.
- **Weapons:** `Weapon` gets `count: u16`, with serde default 1, so every existing template still loads.
  - **On load:** an entry is split into batteries by the hull limit.
  - **In play:** `Ship` keeps the batteries as its weapon list, with per-battery state: mounts working, mounts destroyed.
  - **Wire format:** batteries go out to the client with their mount counts.
- **Features:** new `FeatureKind` variants for `DockingClamp{max_tons}`, `DockingSpace{craft_tons,count}`, `FullHangar{craft_tons,count}`, `LaunchTube{craft_tons}` and `RecoveryDeck{craft_tons}`. Tubes and decks draw Power, so they go on the power board for free and an unpowered tube does not launch.
- **Spare crew:** `radiation_shielding: bool` and `frozen_watch: u32`, both defaulting to false and 0.

### Scenario and live state

```jsonc
// scenario ship
{ "name": "Wind", "design": "Wind-class Strike Carrier", "team": "Blue",
  "embarked": [ { "design": "Light Fighter", "count": 80, "prefix": "Wind Red", "crew": { "pilot": 2, "gunnery": [1] } } ] }
```

**On the carrier:**
- `Ship.embarked: Vec<Embarked>`;
- `Embarked { craft: Box<Ship>, berth: BerthKind, state: Aboard | Launching { rounds_left } | Recovering { rounds_left } | Rearming }`.

`embarked` defaults to the design's `complement`, so a scenario that says nothing still has its fighters.

**On the craft:** `carrier: Option<String>` (its home ship), used by *Land on* and the network features.

**Actions:**
- `ShipAction::Launch { craft: Vec<String> }`, on the carrier, Captain;
- `ShipAction::Land { carrier: String }`, on the craft, Pilot;
- `ShipAction::ReviveWatch { count }`, Captain.

**Wire types:** they live in `payloads.rs`. `ShipActionList` validates them against capacity and range.

**Entities:**
- `Entities.ships` holds only ships in space.
- `update_all` grows a launch/recovery pass after movement and before detection.
- Reset rebuilds `embarked` from the scenario.

**Missiles:** `Missile` gets `count: u16`.
- **Point defence** takes from the count (a torpedo costs two points, as now).
- **Jamming** works on the whole salvo.
- **On impact,** each surviving missile rolls its own attack. This keeps the FAQ's "a missile does its damage dice and nothing more" and its singleton odds, in one entity and one result line.
- **Salvo size:** a battery's launchers fire as one salvo.

### Frontend

- **Ship summary and contact detail:**
  - spinals are written the HG way, "Spinal – meson (TL15) – 3×" (HG p. 35);
  - batteries as "Particle barbette ×12 (battery)", with working/total;
  - carriers add "Craft: 80 Light Fighter (62 aboard)".
- **Gunner card (`WeaponUse.tsx`):** one row per battery, with a count badge.
  - **`ShotOdds`:** shows the spinal's size and sub-band DMs, and greys out targets under 2,000 t with the reason ("too small; stationary targets only").
  - **Reload:** a `"<Reload>"` pseudo-target beside `"<Point Defense>"` on launcher and sandcaster mounts, with loaded/stores counts (§4).
  - **Results:** one line per battery: "7 of 12 hit, 213 damage".
- **Flight Deck card** (carrier, Captain):
  - embarked craft grouped by design and berth, with state: aboard, launching (rounds left), rearming;
  - tube and deck capacity this round;
  - "Launch N" or "Launch all", and a pick-list for named craft;
  - pinned-carrier warnings for a docking-space launch.
- **Helm card (craft):** "Land on <carrier>" when in range, with the Pilot odds from the DM breakdown and the capacity left this round.
- **Plot:** a badge with the count aboard on carriers. Launched craft come out at the carrier, in her team colour.
- **Add Ship (`AddShip.tsx`):** an "Embarked craft" editor (design × count, name prefix, crew skill), filled in from the design's complement.
- **Builder / `hardpoints.ts`:** spinal Hardpoints (tons/100), the half-hull limit, and the `count` field mapped onto the existing `groupWeapons` rows.
- **Admiral situation** (`admiral.rs`): designs listed with spinal, battery and craft lines, and squadrons with "N aboard".

---

## 7. AI

**Spinals and batteries need no new head.**
- **Fire** stays one target per ship. Tactics fires every battery and spinal that bears, and `share_fire` divides batteries among targets as it divides mounts now.
- **What changes is what the network sees:**
  - `tactics::engagement` / `BandOdds` must price spinals (size DMs, sub-bands, ×1,000) and batteries (×count). Every expected-damage feature then follows.
  - New own features: has-spinal, and spinal damage share.
  - A new pair feature: *spinal can engage this target*, which is false under 2,000 t unless stationary.
- **Size features** are capped: `hull_size()` stops at 1.5, which is 10⁶ hull points (done; §3). Damage features are already scaled to the target's hull. A retraining run is needed anyway, because 10⁴–10⁵-point hits and 30,000-HP hulls are far outside anything the network has seen.

**Carriers need a head.**
- **First, without the network:**
  - **Heuristic:** launch everything when an enemy is detected within Very Long; recover when no enemy is detected within Distant, or before a jump.
  - **The admiral's orders:** the `Directive` gains a `flight` field on a step (`launch`, `recover`, `hold`), so the LLM can say "keep the fighters aboard until the cruisers commit".
- **Then a `Flight` head** for every own ship, of size 3: none, launch, recover/land.
  - **Carriers:** "launch" sends out as many as capacity allows, and "recover" opens the deck.
  - **Craft:** "recover" means plot home to their carrier and *Land* when the checks allow.
  - **Masks:**
    - launch only with craft aboard and a tube or berth free;
    - land only within Short and below the velocity limit;
    - recover only if the craft has a carrier.
  - **New own features:** craft aboard (fraction), launch rate (craft per round, out of the total), rounds to launch everything, is-a-carried-craft, range and Δv to the carrier, the carrier's hull share, magazine fraction.
  - **New side feature:** total craft aboard ÷ ships on the plot.
- **Code changes:** `Head::ALL`, `PLAIN` in `side_net.rs`, `sidenet.py`, `policy.py`, `Intent` and `tactics::orders`. The masks go in `side.rs` / `encode.rs`, and the scenario generator in `rl-env` needs carriers.
- **Craft aboard take no own-ship slot.** Their value shows up as carrier features, so the 20-slot cap is unaffected until they launch.

---

## 8. Work breakdown

Sizes: **S** is under ~300 changed lines, **M** ~300–900, **L** above that. Each step is one PR into `capital-ships`, off canary, as the epic's table says.

**Order:**
- Steps 2–5 can go in any order. Reloading (4) is easier after salvos (3).
- 6 needs 2 and 5.
- 9 builds on 4: rearming craft uses the same mount loads.
- 8 needs everything before it.

| # | Step | Size | Retrain? |
|---|---|---|---|
| 1 | Size rules and fixes | S–M | No (evaluate) |
| 2 | Weapon batteries — **done** (branch `batteries`) | L | No (evaluate) |
| 3 | Missile salvos as one entity — **done** (branch `salvos`) | M | No (evaluate) |
| 4 | Reloading: mount loads, spare gunners, the Reload choice — **done** | M | No (evaluate) |
| 5 | Spinal mounts | L | No |
| 6 | Large-ship criticals, frozen watch, sensor operators | M | No |
| 7 | Capital designs to the Tigress, scenarios, the remaining size hardening | M | No |
| 8 | **Retrain 1:** capital ships and reloading | M + GPU run | **Yes** |
| 9 | Embarked craft: engine, rearming aboard | L | No |
| 10 | Carrier frontend | M | No |
| 11 | Carrier AI: heuristic and admiral orders | M | No |
| 12 | **Retrain 2:** the `Flight` head | M + GPU run | **Yes** |
| 13 | FAQ and docs | S–M | No |

1. **Size rules and fixes.**
   - Criticals ignored on hulls over 2,000/10,000/100,000 t; spinal hits always crit (in place for step 5).
   - The large particle bay at Very Long.
   - The radiation DM, with `radiation_shielding`.
   - The hull-point formula check over every template.
   - The FAQ and doc-header fixes for screens and point-defence batteries.
   - *Tests:* unit tests per threshold, including exactly 2,000 t; a template test. *Risk:* low. It changes play for the 3,000 t and 5,000 t designs, so run a 200-game baseline before and after.
2. **Weapon batteries.** **Done**; see "Steps 2 and 3: what was built" below.
   - `count` in `Weapon`, battery forming by hull limit, per-battery mount state.
   - Fire resolved per mount and reported once; the point-defence pool by battery; power by battery.
   - `MAX_SHIP_WEAPONS` counting batteries. It is already 256 (`dd736dfc`).
   - Frontend gunner card and summaries; `hardpoints.ts` speaking `count`.
   - *Tests:* round-trips of old and new JSON; that a battery of N matches N single mounts in distribution; a point-defence pool test. *Risk:* weapon indices are aligned with `crew.gunnery` and `active_weapons`, and the wire format is shared with the client and `ai-ships`. Keep `count: 1` the identity.
3. **Missile salvos as one entity.** **Done**; see below.
   - `Missile.count: u16`, with point defence, jamming and impact per missile inside one entity.
   - The network's missile planner counting salvos.
   - *Tests:* existing missile tests unchanged at count 1; 120 missiles in one entity against a pool; an entity-count check. *Risk:* frontend missile drawing; the landing-round grouping in `network/missiles.rs`.
   - **Do this first among the performance work.** It is the largest single cut in entity count.
4. **Reloading (§4, "Reloading and rearming: decided").** **Done**; see "Step 4: what was built" below.
   - `loaded` per launcher and sandcaster mount, with capacity from the HG table. The ship's `magazine` becomes stores, filled mounts first from the stated total, so no design changes.
   - Reloads by spare gunners (gunnery entries beyond the mounts), when Reload is chosen.
   - `ShipAction::ReloadAction { weapon_id }` beside `PointDefenseAction`, outside the one-per-mount Fire/Point Defence merge; reload after fire.
   - The `"<Reload>"` pseudo-target in `WeaponUse.tsx` beside `"<Point Defense>"`, with loaded/stores counts and the "reloaded by spare crew" note.
   - `Intent.reload: Vec<usize>` and the tactics heuristic; the admiral's design line.
   - The FAQ: rewrite "Ammunition is not tracked" (stale) and *Addition — magazines*.
   - *Tests:*
     - a rack empties after 12 rounds and refills in one;
     - a Firmpoint rack holds 4;
     - a spare gunner reloads with no action;
     - a reloading mount does not fire or react;
     - stores run out;
     - a Reload sits beside the mount's Fire or PD, and keeps its place when the list is resent;
     - old scenario JSON loads with the same totals.
   - *Evaluation:* 200 games of Fighter Screen before and after. The heavy fighter's 60-missile magazine becomes 4 loaded and 56 in stores, and the Kinunir's racks now need reloading.
   - *Risk:* fighters that today fire every round from a shared pool will pause to reload.
5. **Spinal mounts.**
   - The mount, profiles and scaling, the size and sub-band DMs, the stationary rule, the armour-percentage rules, ×1,000. It fires at any bearing (§2).
   - Ammunition, power, builder validation.
   - **Damage dice:** already wide enough (`caa590be`: a `u16` total, to 255 dice). The ×1,000 damage arithmetic moves to `u64`.
   - Frontend summary, gunner card and odds.
   - *Tests:* table-driven profile tests against HG p. 35 and the Wind and Ghalalk figures; DM tests at each size boundary; a 66D spinal hit doing damage end to end.
6. **Large-ship criticals, frozen watch, sensor operators.**
   - Battery-percentage weapon criticals, the crew-percentage table and its DMs, and the cargo D6 table (cargo only until step 9).
   - `frozen_watch` and Revive; one Jam Missiles per 7,500 t, capped at the salvos incoming.
   - *Tests:* severity tables, threshold DMs, revive.
7. **Capital designs to the Tigress, scenarios, size hardening.**
   - **Designs:** JSON for the Valiant (HG pp. 237–238), Ghalalk (pp. 241–243), Azhanti High Lightning (pp. 257–258), Atlantic (pp. 261–262), Wind (pp. 265–268), Antiama (pp. 269–270), Kokirrak (pp. 275–276), Plankwell (pp. 279–280) and Tigress (pp. 283–284), with the HG FAQ's spinal corrections. The carriers' craft wait for step 9.
   - **One test-only design** at 1,000,000 t, and a 10⁸ t station, for overflow tests. They are not shipped.
   - **Hardening:** most of §3's table is done. What is left:
     - the `rl-env` reward sums;
     - the ×1,000 arithmetic (step 5);
     - `toLocaleString` on the frontend.
   - **Scenarios:** a cruiser squadron action, a dreadnought with escorts, and a Tigress action. Upload them to the design and scenario buckets, since merging does not.
   - **Performance:** Phase 3's 200-ship test rerun with these designs. Measure detection, solver and wire time per round, and optimise the worst before going on. The user's ruling is to optimise, not to group fighters.
   - *Tests:* template validation; the overflow tests at 10⁶ t and 10⁸ t; the timing run.
8. **Retrain 1: capital ships and reloading.**
   - `tactics::engagement` pricing spinals and batteries.
   - New own and pair features (`OWN_FEATS`, `PAIR_FEATS`): has-spinal, spinal-can-engage, the loaded share of launchers. No new head.
   - Capital designs and stations in the rl-env scenario generator; a full PPO run (games × rollout ≈ 4k per update).
   - *Gate:* the new network at least matches the old one on Fighter Screen, Thebus and Aslan Border Clash, and beats the heuristics on capital scenarios.
9. **Embarked craft: engine.**
   - Facility features, `embarked` and `complement`, Launch, Land and timers; the landing check and failure damage.
   - Rearming aboard: one round in any berth refills a craft's mounts from the carrier's stores (option C).
   - Air-group squadrons, loss with the carrier, cargo criticals that hit craft, docking clamps.
   - *Tests:* launch rates per facility, pinned-carrier rules, landing DMs and failure damage, capacity, reset restoring craft, a rearm drawing down the carrier's stores. *Risk:* entity lifetime across boarding and launch, detection state, squadron membership.
10. **Carrier frontend.**
    - The Flight Deck card, *Land* on the helm, the plot badge, the embarked-craft editor, the admiral situation lines.
    - *Tests:* Vitest on the card and the editor; a manual play of the Wind scenario.
11. **Carrier AI: heuristic and admiral orders.**
    - Default launch and recover in `tactics`; `flight` in `Directive`, the prompt, the schema and a parsed example.
    - *Tests:* a harness baseline of carrier against cruiser.
12. **Retrain 2: the `Flight` head.**
    - The head, masks and features (§7); carriers in the scenario generator; a full run.
    - *Gate:* beats the step-11 heuristic over 200 games and holds on the old scenarios.
13. **FAQ and docs.**
    - Replace "The big one: no capital ships". Note that there is no facing (Companion p. 169), so fixed mounts stay single-weapon mounts.
    - Add every ruling here to *Rules decisions and assumptions*.
    - Update `docs/fleet_control_plan.md` with results.

**Plotting:** Phase 5's 100+ ship battles can start after step 8. They only need carriers after step 12.

---

### Steps 2 and 3: what was built (2026-10-29)

Step 3 is on branch `salvos`; step 2 is on `batteries`, which is cut from it.

**Salvos.**
- `Missile.count: u16`, defaulting to 1 so old scenarios (and the bucket's) load, and written only above 1, so a lone missile is on the wire as before. `LaunchMissileMsg` carries the count. A mount's launch is one salvo: one guidance solve a round, one wire entry.
- **On impact** (`entity.rs` `update_all`): point defence drains the target's pool missile by missile, across every salvo that lands that round; a torpedo still costs 2. The rest each roll their own attack through `combat::volley`, which runs `resolve_attack` once a missile and reports one line, "missile salvo (12, 3 shot down) -> X: 7 of 9 hit, 31 damage", with the rolls and damages on the hover. Criticals keep their own lines. One explosion is drawn per salvo.
- **Jamming** destroys its Effect's worth of missiles chosen at random from everything inbound, across salvoes. A salvo that loses some flies on with the rest; one line per salvo hit.
- **The network** counts missiles, not entities, in every feature (`orders::missiles_in`). A test encodes a salvo of three and three lone missiles and checks every own, foe, side and pair feature is bit-identical. The missile planner's in-flight pool cost counts the same way.
- **Frontend**: the Gunner card's Salvoes inbound and Ours in flight, the sensor station's inbound salvoes, plot labels ("name ×12"), the entity selector and the hover window count missiles (`lib/missileCount.ts`).

**Scale test**, 200 ships and 10,000 missiles, `ai-ships/examples/scale.rs`, sides idle (the server alone). "Before" is every missile its own launch, which is also what `SCALE_SALVO=1` gives now; "after" is `SCALE_SALVO=12`, a small missile bay's salvo, so 834 salvoes:

| | Before | After (salvoes of 12) |
|---|---|---|
| Entities in flight | 10,000 | 834 |
| World update | 3.09–3.37 MB | 0.43–0.45 MB |
| Serialising it | 8.4–9.0 ms | 0.9–1.2 ms |
| Resolving a round in flight | 37–56 ms | 15–19 ms |
| Observation, per side | 5.4–5.9 ms | 0.8–1.0 ms |
| Results, worst round | 1.65 MB: 5,245 lines, 2,005 impacts, 1,395 exhausted | 0.36 MB: 1,081 lines, 424 impacts, 48 exhausted |
| Results, other landing round | 0.63 MB | 0.25 MB |
| Network decision, per side (network commander) | 42–46 ms | 25–27 ms |

With `SCALE_SALVO=1` the update and the results are byte-for-byte the old ones: a lone missile writes no `count`, and resolves exactly as before. Remaining results lines in the landing rounds are mostly criticals and destructions, one per event.

**Batteries.**
- `Weapon.count: u16` (default 1, written only above 1). `ship::form_batteries` splits each entry by `battery_limit(displacement)` (4/8/16/32), full batteries first, on template load and on a scenario's own armament; a test checks every shipped design comes out identical. `MAX_SHIP_WEAPONS` (256) counts the formed entries.
- Fire: `volley` again, an attack per mount, one line: "particle beam barbette ×12 (battery) -> X: 7 of 12 hit, 213 damage, 1 critical". A battery of four is shown roll for roll equal to four single mounts.
- A missile battery launches one salvo of every launcher's missiles. Power draws for every mount.
- Defence: a laser battery on point defence makes a check per mount (one line, "mount #3 (battery of 16) on point defence can stop 23 missiles"); sandcaster, repulsor and point defence battery entries act per mount.
- AI: `tactics::engagement` and `launch_size` price a battery as its mounts, so `share_fire` and the missile planner assign it whole at its whole weight. For existing designs every number is the same (× 1).
- Frontend: "×16 (battery)" names; one Gunner button per battery size with the batteries left to order and a ×16 badge; Add Ship keeps batteries as one row of mounts and splits them back by the hull with a gunner each, with a "Fire as batteries" switch in the detail editor; design summaries read "200 Triple Beam Turrets in 13 batteries".

**Rulings made on the way** (also in the FAQ):
- A hull under 100 tons takes IN's first row, 4 a battery.
- Only turrets, barbettes and bays can be batteries. An entry of several fixed mounts or point defence batteries is that many single weapons (so the §6 example `{"kind":"PointDefense","mount":{"Battery":3},"count":8}` is eight batteries).
- A design's `crew_skills` gunnery is index-aligned with the formed batteries, not the file's entries.
- A Weapons critical still switches off the whole battery it picks: per-battery mount state and HG's percentage table wait for step 6.
- A called shot is called for the whole battery; the captain's Assist Gunner +1 lands on every mount of the first battery fired.
- Each shot of a laser battery can draw a sand cloud while the target has any; a sandcaster battery throws a cloud per mount. Several clouds against one battery read as one line.
- Each missile or mount still uses up the defender's dodge as a separate attack did; the dodges are reported in one line per volley.
- Jamming picks missiles uniformly at random from all inbound missiles, so bigger salvoes lose more.

**For the user to check by eye:** a scenario with a battery design (none ships yet: give a design a `count`); the Gunner card's battery button and badge; Add Ship on such a design, including the switch; a results log with a salvo and a battery line, and the hover arithmetic; the plot's salvo labels and the swarm with a few hundred salvoes.

### Step 4: what was built (2026-10-29)

On `epic-fleet-control`.

**Loads and stores.**
- `Ship.loaded: Vec<Magazine>`, index-aligned with the weapons; `Ship.magazine` is now stores. `ship::mount_capacity` gives the book's load (the FAQ has the table), and `Ship::stock` fills the mounts in order from a total and puts the rest in stores.
- **Where the total comes from:** a stated magazine. A design that states none carries what it did before (12 a rack, 20 a caster, 3 a torpedo launcher), raised to the mount's load where that is more: a barbette's 25, a bay's twelve salvos. `Magazine::for_weapons` also counts every mount of a battery now.
- **Old JSON:** a ship read without `loaded` takes its `magazine` as its whole stock and fills the mounts from it, so old scenarios and saves carry what they did. A test checks that every shipped design carries its old total, apart from the bay and barbette cases above.
- Launches come out of the firing mount (`LaunchMissileMsg.mount`, not on the wire). Sand clouds come out of the casting mount (`combat::SandCloud`), and a sandcaster with an empty load throws nothing.

**Reload.**
- `ShipAction::ReloadAction { weapon_id }` and the anti-action `DeleteReloadAction`. A resent list does not move a Reload already queued, so the order in which spares are handed out is kept.
- `reload::assign` gives spares in order and drops a Reload that would put nothing in the mount (full, no launcher, or stores out), with a line saying why. Own-gunner mounts lose their Fire and PD for the round, and their sand through `create_sand_counts`. Their queued orders are kept.
- `reload::apply` runs after the jam passes, before `update_all`, and writes a line per reload. Reloads are cleared at the end of the round.

**AI.**
- `Intent.reload: Option<Vec<usize>>`. `None`, used by the LLM, the network and the heuristic commanders, means `tactics::reloads` chooses, as §4 describes.
- `tactics::orders` leaves own-gunner reloads out of fire and defence.
- The network's magazine feature reads `Ship::aboard()` (mounts plus stores) over the design's stock. A test in `ai-ships` checks that the split between racks and stores does not change any encoding.
- The admiral's design and squadron lines carry "missile racks 24/24, stores 48".

**Frontend.**
- The gunner card shows "Spare gunners: N of M free" and a `<Reload>` pseudo-target. Choosing it lists the launcher and sandcaster mounts with Reload buttons and the notes from §4. A chosen Reload can be withdrawn.
- The magazine section shows stores and each mount's load.

**Rulings made on the way** (also in the FAQ):
- **A load is per turret, as both books word it:** "each turret with one or more missile racks holds 12 missiles" (CRB p. 168, HG p. 28), and the same for sandcasters. The table above, "Turret missile rack 12", reads either way. Because of this ruling, a silent design keeps today's total by holding the rest in stores (a triple rack turret has 12 loaded and 24 in stores), rather than having no spares as §4 assumed.
- **A Reload that would put nothing in the mount takes no spare,** and it does not cost an own gunner the shot. A mount that is full when the round starts cannot be "fired and reloaded", so the card only offers Reload below capacity.
- **A mount whose own gunner reloads keeps its Fire order** for the next round. The card says "its order waits a round" rather than deleting it.
- **A fixed-mount rack loads as a turret's** (12, or 4 on a Firmpoint), which fits the heavy fighter's 4 loaded and 56 in stores.
- **The LLM's orders have no reload field yet;** tactics reloads for it. The prompt's "Magazine" line counts everything aboard.

**Not done:** the 200-game Fighter Screen evaluation before and after. That is for the user or a later run.

**For the user to check by eye:**
- The gunner card on a missile ship with a spare gunner in its crew: the header, `<Reload>`, both notes, and withdrawing.
- The magazine section's per-mount lines.
- A results log with a spare-crewed reload and an own-gunner reload.

## 9. Decisions and open questions

**Decided by the user (2026-10-29).** Nothing is open.
- **Facing:** none, as the Companion rules (p. 169): ships pivot freely within the round. Spinals fire at any bearing with HG's size and close-range DMs, and fixed mounts are ordinary single-weapon mounts (§2). The earlier thrust-vector facing was dropped, with its cone, step, frontend and AI work.
- **Reloading:**
  - **The ship's own launchers and sandcasters:** reloading is free while the ship has more gunners than mounts. Otherwise a gunner picks **Reload** in place of a target, the same way Point Defence is picked, and that is the mount's action for the round.
  - **Carried craft without stores** go home, land, spend one round reloading from the carrier's magazine, and launch again (option C) (§4).
- **Fighters:** each fighter stays its own ship. If performance suffers, optimise first, missiles above all (steps 3 and 7). HG's squadron abstraction is not used.
- **Size:** step 7 builds every design up to the 500,000 t Tigress. The code is planned for 1,000,000 t ships and larger stations. The scale fixes in §3 marked Done are already made.

## What could not be settled from the books

- **Facing:** the Companion (p. 169) ignores it, and nothing else gives a spacecraft arc or ties facing to thrust. Callisto follows the Companion.
- **No turnaround time exists for rearming carried craft,** and no rule says whether a docking space allows it. The user's one round in any berth is an Addition (§4).
- **HG gives space stations no hull-point rule of their own.** The ship bands are assumed.
- **HG never defines a "stationary" target** for spinal fire, or how relative velocity affects landing. Both are assumptions above.
- **Element Cruisers designs predate HG 2022.** EC writes spinals as "Particle Accelerator (improved, 1DD)", an older notation. Its hull figures include pods (the Ghalalk's 27,500 is for 50,000 t with pods). Callisto should build from HG's own Ghalalk (HG pp. 241–243), not EC's.
- **Launch and recovery rates differ between sources.** The HG FAQ says 10 per round. HG's text says one per deck, with a round to move into a tube, and EC's pod follows HG's text. We follow the FAQ.
- **The HG FAQ cites the Wind-class at "pages 266–269"**, one higher than this PDF's printed numbers (265–268). Either the errata was written against another printing or it uses PDF page numbers. The cites here are this PDF's printed pages.
