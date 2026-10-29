# Capital ships and carriers: plan

Phase 4 of the fleet-control epic (`docs/fleet_control_plan.md`). This document has three parts:
- what the books say about ships of 5,000–1,000,000 tons, spinal mounts and carriers;
- how Callisto should rule on each point, in the FAQ's style;
- the work, split into PR-sized steps.

It changes no game code.

**Decisions since the first draft (2026-10-29).** The user settled facing, fighters and size. They are recorded in §9, and the rulings below follow them. New sections: §2 on facing, "What bites at 1,000,000 tons" in §3, and "Rearming: rules as written" in §4.

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
- **Every missile is its own entity.** A large missile bay fires 120 a round, so one Tigress broadside would create about 5,000 entities.
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

#### Change — the spinal fires along the thrust axis
A spinal is a pointed weapon under §2.
- **When it can fire:** only while the ship is coasting, or while every powered segment of her burn this round is within 45° of the target's bearing.
- **What it does not cost:** dodge or Assist Gunner (the user's decision).

#### Interpretation — "stationary" means no thrust last round
HG does not define a stationary target, and in Newtonian space nothing is at rest. Callisto rules that a target under 2,000 t is open to a spinal if it **spent no Thrust and took no Evasive Action in the previous round**. A ship drifting with dead drives, or coasting to bring her own spine round, can be hit. The DM−8 for 5,000 t or less still applies.

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

## 2. Facing: spinal and fixed mounts

**What the user decided** (2026-10-29):
- A ship faces along her thrust vector.
- Firing a spinal costs no dodge and no Assist Gunner, because no rule says it does.
- She cannot thrust away from a target and fire her spinal at it.

This is built as **one mechanism for every weapon that has to be pointed**: spinal mounts, and the fixed mounts on today's fighters. Turrets are unaffected.

### What the books say

- **Fixed mounts point forward.**
  - "Fixed mounts can only be fired directly forward" (CRB p. 168).
  - "a fixed mount can only fire at targets directly ahead of it, a turret rotates and can engage any target in sight" (HG p. 28; CRB p. 183).
- **A Firmpoint may point another way.** A Firmpoint is "a fixed mount, typically forward-facing but may alternatively point in any desired direction" (HG p. 26, as the HG FAQ amends it, and CRB p. 183).
- **The only arc with a printed angle is for vehicles.** "A weapon mounted in the front arc … can only fire into a 90° area in front of the vehicle. Weapons in turrets can fire in any direction" (CRB p. 139, *Fire Arcs*). That chapter's diagram shows forward and rear fixed-mount arcs, but gives no angle for them.
- **Dogfights:** the winner places the loser "in a fire arc of their choice", so that "all their forward facing weapons can attack" (CRB p. 174). Callisto has no dogfights.
- **Spinals are aimed with the whole ship.**
  - Spinals are "aimed by pointing the whole ship, with minimal aiming corrections possible by manipulating electromagnetic fields around the weapon beam" (IN p. 98).
  - HG also allows "some degree of directional adjustment … at the barrel" and puts the close-range penalties down to "their sheer size and dependency on the movement of the host ship" (HG pp. 34–35).
- **Turning is quick; changing course is not.** "Alignment can be changed almost immediately, unmasking batteries or protecting damaged areas. It is a form of fine-tuning for the situation created by momentum" (IN p. 85). "A ship can pitch, roll and yaw to bring different batteries to bear" (IN p. 85).
  - Launch tubes are rolled into alignment the same way (IN p. 97).
  - Missile and torpedo bays on the flanks are useful because they need no alignment, by contrast with the main armament (IN p. 99).
- **The Companion's vector rules ignore facing.** "Spacecraft facing is ignored. It is assumed that ships may pivot to any needed direction over the course of each six-minute space combat round, even when expending their full Thrust during a round" (*Traveller Companion* 2024, p. 169). It also notes that facing "does not have a mechanical effect" (p. 174).
- **Nothing anywhere ties facing to thrust.** No book, the HG FAQ included, has a cone, an arc in degrees for spacecraft, a turning rate, or a rule linking thrust to facing.

So the user's rule is a **Change**. The nearest RAW is the Companion's free pivot, which is exactly what is being given up. The cone below is the CRB's vehicle front arc, carried across.

### Callisto rulings

#### Change — a ship faces along her thrust
- **The direction:** while a ship is under thrust, her bow points along her acceleration vector.
- **Coasting:** a ship spending no Thrust in a round is **free to face any way she likes**.
  - There is no stored attitude. IN p. 85's "almost immediately" means a ship with her drive off can swing to any bearing within a six-minute round, which is also the Companion's pivot.
  - Keeping the last thrust direction as an attitude was considered and rejected. It adds state the plot and the network would have to carry, and punishes a ship for a burn she has already stopped.
- **Dodging does not set facing.** Evasive Action thrust jinks around whatever attitude the ship already has. It is not part of the course, and RAW gives it no direction.
- **Stations:** a station with Thrust 0 is always coasting, so her fixed and spinal weapons always bear.

#### Change — what counts as "thrusting" in a round
**When it is checked.** Callisto fires before it moves: `do_fire_actions` runs before `update_all`. A ship's facing therefore comes from the flight plan she is about to fly this round: up to two segments (`FlightPlan(AccelPair, Option<AccelPair>)`).

**The rule.** A pointed weapon bears on a target only if **every powered segment of this round's plan** is within the cone of the bearing to that target. Coasting segments do not count. A ship whose plan has no powered segment is coasting.
- **The bearing** is measured at the moment of fire, from the start-of-round positions.
- **Why every segment:** otherwise a one-second burn toward the target followed by five minutes away would be an exploit.
- **What it costs:** the classic flip-and-brake. A ship decelerating on her way in has her thrust, and so her spine, pointed away from the target. To fire, she coasts or holds her braking burn until she has fired. That is the Newtonian price of the rule, and it is the intent.

#### Decision — the cone is 45° either side of the bow
**The number.** A pointed weapon bears on a target within **45° of the thrust axis**: a 90° cone. This is the CRB's "90° area in front" for a vehicle's front arc (CRB p. 139), the only arc the books print.
- **Why not narrower:**
  - "Directly ahead" read literally would be 0°. That would make any intercept burn, which almost always has a sideways part, useless for firing.
  - Six minutes is a long round, and the plan's vector is an average over it.
  - IN p. 98 and HG p. 34 both allow some aiming at the barrel.
- **Why not wider:** it would hardly restrict anything. At 90° from the bow, burning straight sideways would still allow fire.
- **The same cone for spinals and fixed mounts.** HG gives spinals their own clumsiness already, in the close-range DMs (HG p. 35). Narrowing their cone as well would charge twice for the same thing.
- **A tunable:** it is one constant, `POINTED_ARC_HALF_ANGLE`. The value is a play decision and can move once scenarios have been flown.

#### Interpretation — which weapons are pointed
| Mount | Pointed? | Why |
|---|---|---|
| Spinal | Yes, forward only | IN p. 98 |
| Fixed mount, Firmpoint | Yes, forward by default | CRB p. 168, HG p. 28 |
| Fixed mount marked **aft** | Yes, along −thrust | A Firmpoint "may … point in any desired direction" (HG p. 26). Only fore and aft are offered. A sideways mount needs a facing frame the plot does not have |
| Turret | No | "rotates and can engage any target in sight" (HG p. 28) |
| Barbette | No | Turret-like. "capable of bringing fire to bear in a wider arc" (IN p. 99) |
| Bay | No | HG gives bays no arc. IN's flank bays (p. 99) suggest some do have one, but there is no rule to build on |
| Point-defence battery, screens | No | Automatic and defensive |

**Assumption — launchers and sandcasters in fixed mounts are exempt.**
- **Missiles and torpedoes** leave a fixed rack forward and steer onto the target with their own drive. They are Smart, and in Callisto they already fly their own course from the launcher. HG already exempts them from the Firmpoint range limit (HG p. 26).
- **Sand** is a cloud around the ship.

This is a ruling rather than RAW. The book's "directly forward" does not carve out missiles.

#### Consequence — fighters change the most
**Who is hit.** Today's light and ultralight fighters carry their pulse laser in a fixed mount, as does the military gig. A fighter can now shoot only along her burn, or while coasting. The heavy fighter's beam turret is unaffected, and its fixed missile rack is exempt.
- **What it means:** intercepting at full thrust works, because the burn points at the target. Opening the range and shooting back does not.
- **Fighter Screen and Fighter Swarm play differently,** so the step needs a before-and-after run and is part of retrain 1.

#### What it does not change
- **No dodge or Assist Gunner cost** (the user's decision).
- **The spinal's close-range DMs** and its "cannot attack under 2,000 t unless stationary" rule (§1).
- **Being shot at is not affected by your own facing.** There is no facing armour (the CRB's vehicle facings, p. 140, are not used) and no arc protection for engineering (IN p. 85's flavour).

### Data model
- **`Weapon`** gets `arc: Option<Arc>` with `Arc::{Fore, Aft}`. Spinals and fixed mounts default to `Fore`; other mounts reject it.
- **Facing check:** `tactics::bears(ship, weapon, target) -> bool` and a shared `facing::in_arc(plan, bearing, arc)`. It is used:
  - by `do_fire_actions`, which refuses with "Spinal not on target: burning 112° off its bearing";
  - by `tactics::engagement` / `BandOdds`, so expected damage respects it;
  - by the client's odds display.
- **No new ship state:** facing is computed from the plan every round.

### Frontend
- **Plot:** the acceleration arrow (`Ships.tsx` already draws it from the plan) becomes the bow.
  - **Own ships:** a ship with pointed weapons shows a translucent 90° wedge along it. Fore and aft wedges appear for aft mounts.
  - **Coasting:** a ring instead of a wedge, meaning she can face anywhere.
- **Gunner card (`WeaponUse.tsx`):**
  - pointed weapons are marked with a bow glyph;
  - a target outside the cone is greyed out with "off the bow: 112° (needs ≤45°)";
  - a one-click fix offers "Coast this round" or "Burn toward <target>", which rewrites the plan through the normal helm path.
- **Helm card:** when the plotted burn takes a pointed weapon off the ship's current Fire target, a warning names the weapon and the angle before orders are sent.
- **Enemies:** another ship's plan is not visible. The wedge is drawn around her **last observed acceleration**, labelled "last burn", and only when there is a sensor contact (with no contact, thrust shows `?`, as today). The wedge is a guess and says so.
- **`ShotOdds`:** a bearing term, "in arc" or "out of arc", alongside the range and size terms.

### AI
**The conflict.** Facing couples two heads that are independent today: Helm (where to burn) and Fire (whom to shoot). The pointed weapons follow the burn.

**First: a tactics rule, before any retraining.** After the helm produces a burn, `tactics::orders` checks the Fire target against the ship's pointed weapons. If they are out of arc, it compares three options by expected damage and by what the burn is worth, then takes the best:
1. keep the burn, and fire only turrets, bays and launchers;
2. turn the burn into the cone: keep its magnitude and swing the direction to the nearest edge of the cone;
3. coast this round.

Fighter intercepts mostly fall into option 2, and capital ships into 1 or 3.

**Retrain 1 adds an `Align` head** of size 3 (helm wins / weapons win / coast), with the rule above as its default and its mask.
- **Masks:** "weapons win" only when a pointed weapon could bear on the Fire target at all, which is false when the target is under 2,000 t for a spinal-only ship. "Coast" only when the ship has a plan.
- **New own features:**
  - share of own expected damage in pointed weapons;
  - has-spinal;
  - has-aft-mount.
- **New pair features:**
  - the cosine between my current burn and the bearing to this enemy;
  - the same for the enemy's last observed burn and the bearing to me, which says whether *her* spine is on me;
  - her pointed-damage share.
- **The heads are combined:** the Helm head's band choice and the Align head are decided together, so the network can learn to approach on a line that keeps its spine on.
- **Where the code goes:** `Head::ALL`, `side_net.rs` `PLAIN`, `sidenet.py`, `policy.py`, and the `Intent.align` field. The masks go in `side.rs` / `encode.rs`.

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

What I checked on `epic-fleet-control`:

| Where | What happens at size | Fix (step) |
|---|---|---|
| **Damage dice:** `combat::roll_dice_min(dice: u8) -> u8` | Refuses more than 42 dice (42 × 6 > 255). It **logs an error and returns 0**. A 66D Tigress spinal, or an 11× meson, would silently do no damage. Today's largest roll, the 10D large bay, fits | Return `u32`, and widen `WeaponProfile.damage_dice` past `u8` (66D fits a `u8`, but scaled spinals and summed battery dice do not) (step 5) |
| **Hull and displacement:** `ShipDesignTemplate.{displacement, hull, power, fuel, crew}` are `u32` | Fine up to 4.29 × 10⁹. A 10⁹ t station has about 6.7 × 10⁸ HP | None |
| **Hull-point formula** (HG p. 10) | 1 per 1.5 t above 100,000 t. HG gives stations no other rule, so the same bands are assumed | The step-1 check uses this |
| **Spinal damage** | 66D, max 396, + Effect, × 1,000 is about 4 × 10⁵. Fits `u32` | Do the arithmetic in `u64` anyway (step 5) |
| **Salvo size:** `Salvo::Fixed(u16)` | A 32-bay large-missile battery is 3,840 missiles; `u16` holds 65,535 | Fine. `Missile.count` should also be `u16` (step 3) |
| **Magazine:** `u32` | Fine | None |
| **`MAX_SHIP_WEAPONS = 64`** (`player.rs`) | The Tigress has more than 900 mounts. Batteries bring her to about 40–60 entries, which is close to the cap. A large station could pass it | Count batteries, and raise the cap to 256 (step 2) |
| **Crit thresholds:** `ceil(max_hull/10)` in `u64` | Fine. A single spinal hit can cross several tenths, so sustained-damage criticals come in bunches | Intended |
| **Critical-hit size bands** (HG p. 27) | Top out at "larger than 100,000 tons". There is nothing for stations beyond that | Use the same bands |
| **Battery size** (IN p. 113) | Tops out at 32 per battery "over 100,000" | Same at every size |
| **Sensor operators** (HG p. 34) | One per 7,500 t: 133 at 10⁶ t, 133,333 at 10⁹ | Cap the automatic extra jams at the number of salvos actually incoming |
| **Crew-critical percentages** (HG p. 27) | Already relative, so they scale | None |
| **Crew reduction** (HG p. 22) | The table stops at "100,000+ (33%)" | Nothing in play depends on it |
| **Prompt arithmetic:** `ai-ships/src/prompt.rs` `100 * ship.current_hull / design.hull` in `u32` | Overflows above 42.9 M HP, about 64 M t. Stations only | Compute in `u64` (step 7) |
| **Admiral totals:** `admiral.rs` sums of hull in `u32` | Overflows at 4.29 × 10⁹ HP. Only reachable with stations | Compute in `u64` (step 7) |
| **Network size features:** `log10(hull)/4` | Gives 1.45 at 10⁶ t and 2.2 at 10⁹ t. Armour/20 reaches about 1.5 with military-hull armour (HG p. 12 doubles the maximum) | Clamp to [0, 2], and put stations in retrain 1's scenario mix (step 7) |
| **Expected-damage features** | Are shares of the target's hull, so they scale | None |
| **Power:** `power * 10 / displacement` in `u32` | A 10⁹ t station's Power, about 2 × 10⁸, overflows when × 10 | Compute in `u64` (step 7) |
| **Hardpoints:** `hardpoints.ts` `floor(displacement/100)` | 10,000 at 10⁶ t. Fine in JS | Show with thousands separators |
| **Frontend numbers** | Hull and damage print as raw integers, e.g. "666667" | Use `toLocaleString` everywhere tons, hull and damage are shown (step 7) |
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

#### Open — rearming and refuelling
The user wants the rules as written first. They are set out under "Rearming: rules as written" at the end of this section, with options A–E and no choice made.

**Fuel is not tracked:** a fighter's power plant runs for weeks (HG p. 18).

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

### Rearming: options for Callisto (not decided)

| Option | What happens | For | Against |
|---|---|---|---|
| **A. No rearming in a scenario** | A recovered craft keeps what it has left | Closest to RAW's silence. No new data | Recovering craft becomes only about repairs and safety. A missile fighter is one-shot |
| **B. Free restock in a full hangar** | One full round aboard in a hangar restores the craft's magazine. A docking space or recovery deck does nothing | Simple, and follows the hangar/docking-space split RAW does make (HG p. 61) | Carriers never run dry. Ignores the carrier's stores |
| **C. Restock from the carrier's magazine** | As B, but the missiles and sand come out of the carrier's own `magazine`, a design field. A carrier with no spares cannot rearm | Uses HG's tonnage-based storage (p. 36) and the existing `Magazine`. Carriers can run out, which is a real constraint | Every published carrier needs a stated spare load. Most HG designs give none, so we need a default (e.g. a ton per 12 craft-missiles) |
| **D. As C, with a turnaround time** | Rearming takes N rounds per craft, with only so many crews at a time (for example the EC pod's 24 support crew, or the HG maintenance count) | The most realistic. Makes hangar capacity matter | Time and crew numbers are invented. More state, and the network needs a "rounds until ready" feature |
| **E. Rearm anywhere aboard, one round** | The CRB's one-round rack reload, applied to any berth | Mechanically simplest, and echoes CRB p. 168 | Contradicts HG p. 61 and IN p. 97, which say there is no room to work in a docking space |

Options B–D only matter in scenarios long enough for fighters to come home. Step 9 can ship with A and add B, C or D behind a scenario flag once a decision is made.

---

## 5. Other High Guard rules that matter at fleet scale

| Rule | Book | Callisto | Plan |
|---|---|---|---|
| **Meson screens, nuclear dampers** | HG p. 41, HG FAQ p. 2 | Implemented | Fix the FAQ's opening section and the two stale doc headers (step 1) |
| **Black globe generators** | HG pp. 41–42 | Missing | Omission. A referee's artefact, not something that can be bought |
| **Repulsor as tractor beam** | HG p. 34 | Point-defence half only | Omission. It needs a "held" state on a ship |
| **Missile salvo grouping** | CRB p. 169; HG p. 33 bays | One entity per missile | Step 3: one entity per salvo, which a 120-missile bay needs |
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
  - **Pointed weapons** (spinal, fixed): the arc display in §2 shows whether they bear.
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
  - **Facing** (spinal and fixed mounts) gets its own tactics rule and the `Align` head (§2, AI).
- **Size features** stay as they are: `log10(hull)/4` reaches about 1.45 at 1,000,000 t. Damage features are already scaled to the target's hull. A retraining run is needed anyway, because 10⁴–10⁵-point hits and 30,000-HP hulls are far outside anything the network has seen.

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
- Steps 2–4 can go in any order.
- 5 needs 4.
- 6 needs 2 and 5.
- 8 needs everything before it.

| # | Step | Size | Retrain? |
|---|---|---|---|
| 1 | Size rules and fixes | S–M | No (evaluate) |
| 2 | Weapon batteries | L | No (evaluate) |
| 3 | Missile salvos as one entity | M | No (evaluate) |
| 4 | Facing for pointed weapons | M | No (evaluate; fighters change) |
| 5 | Spinal mounts | L | No |
| 6 | Large-ship criticals, frozen watch, sensor operators | M | No |
| 7 | Capital designs to the Tigress, scenarios, size hardening | M–L | No |
| 8 | **Retrain 1:** capital ships, facing, the `Align` head | M–L + GPU run | **Yes** |
| 9 | Embarked craft: engine | L | No |
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
   - The stale FAQ line "Ammunition is not tracked", which magazines now contradict.
   - *Tests:* unit tests per threshold, including exactly 2,000 t; a template test. *Risk:* low. It changes play for the 3,000 t and 5,000 t designs, so run a 200-game baseline before and after.
2. **Weapon batteries.**
   - `count` in `Weapon`, battery forming by hull limit, per-battery mount state.
   - Fire resolved per mount and reported once; the point-defence pool by battery; power by battery.
   - `MAX_SHIP_WEAPONS` counting batteries, raised to 256.
   - Frontend gunner card and summaries; `hardpoints.ts` speaking `count`.
   - *Tests:* round-trips of old and new JSON; that a battery of N matches N single mounts in distribution; a point-defence pool test. *Risk:* weapon indices are aligned with `crew.gunnery` and `active_weapons`, and the wire format is shared with the client and `ai-ships`. Keep `count: 1` the identity.
3. **Missile salvos as one entity.**
   - `Missile.count: u16`, with point defence, jamming and impact per missile inside one entity.
   - The network's missile planner counting salvos.
   - *Tests:* existing missile tests unchanged at count 1; 120 missiles in one entity against a pool; an entity-count check. *Risk:* frontend missile drawing; the landing-round grouping in `network/missiles.rs`.
   - **Do this first among the performance work.** It is the largest single cut in entity count.
4. **Facing for pointed weapons (§2).**
   - `Weapon.arc`, `facing::in_arc` over every powered plan segment, the 45° cone, the coasting exemption, and the launcher and sand exemption.
   - Refusal messages; `tactics::engagement` respecting the arc; the tactics rule that picks among keep-burn / turn-into-cone / coast.
   - Frontend wedge and ring, the gunner-card greying and one-click fixes, the helm warning, the enemy "last burn" wedge, and the `ShotOdds` term.
   - *Tests:* the cone edge at 44° and 46°; two-segment plans, one powered and one coasting; a flip-and-brake refusal; an aft mount; launcher exemption; a coasting ship bearing anything.
   - *Evaluation:* 200 games each of Fighter Screen and Fighter Swarm before and after, with the old network. *Risk:* the current network flies fighters without knowing about arcs and will lose shots until retrain 1. The tactics rule is what keeps play sane in the meantime.
5. **Spinal mounts.**
   - The mount, profiles and scaling, the size and sub-band DMs, the stationary rule, the armour-percentage rules, ×1,000, and facing through step 4.
   - Ammunition, power, builder validation.
   - **Damage dice made wide:** `roll_dice_min` returns `u32`, and `damage_dice` is no longer a `u8` (today more than 42 dice silently roll 0). The damage arithmetic moves to `u64`.
   - Frontend summary, gunner card and odds.
   - *Tests:* table-driven profile tests against HG p. 35 and the Wind and Ghalalk figures; DM tests at each size boundary; a 66D roll test; an out-of-arc refusal.
6. **Large-ship criticals, frozen watch, sensor operators.**
   - Battery-percentage weapon criticals, the crew-percentage table and its DMs, and the cargo D6 table (cargo only until step 9).
   - `frozen_watch` and Revive; one Jam Missiles per 7,500 t, capped at the salvos incoming.
   - *Tests:* severity tables, threshold DMs, revive.
7. **Capital designs to the Tigress, scenarios, size hardening.**
   - **Designs:** JSON for the Valiant (HG pp. 237–238), Ghalalk (pp. 241–243), Azhanti High Lightning (pp. 257–258), Atlantic (pp. 261–262), Wind (pp. 265–268), Antiama (pp. 269–270), Kokirrak (pp. 275–276), Plankwell (pp. 279–280) and Tigress (pp. 283–284), with the HG FAQ's spinal corrections. The carriers' craft wait for step 9.
   - **One test-only design** at 1,000,000 t, and a 10⁸ t station, for overflow tests. They are not shipped.
   - **Hardening:**
     - every row of the "what bites" table in §3: `u64` in the prompt, admiral and power arithmetic;
     - clamped network size features;
     - `toLocaleString` on the frontend.
   - **Scenarios:** a cruiser squadron action, a dreadnought with escorts, and a Tigress action. Upload them to the design and scenario buckets, since merging does not.
   - **Performance:** Phase 3's 200-ship test rerun with these designs. Measure detection, solver and wire time per round, and optimise the worst before going on. The user's ruling is to optimise, not to group fighters.
   - *Tests:* template validation; the overflow tests at 10⁶ t and 10⁸ t; the timing run.
8. **Retrain 1: capital ships, facing, the `Align` head.**
   - `tactics::engagement` pricing spinals, batteries and arcs.
   - New own and pair features (`OWN_FEATS`, `PAIR_FEATS`); the `Align` head and its masks.
   - Capital designs and fixed-mount fighters in the rl-env scenario generator; a full PPO run (games × rollout ≈ 4k per update).
   - *Gate:* the new network at least matches the old one on Fighter Screen, Thebus and Aslan Border Clash, and beats the step-4 rule on capital scenarios.
9. **Embarked craft: engine.**
   - Facility features, `embarked` and `complement`, Launch, Land and timers; the landing check and failure damage.
   - Rearming per the chosen option (open question 1, with A as the default until it is chosen).
   - Air-group squadrons, loss with the carrier, cargo criticals that hit craft, docking clamps.
   - *Tests:* launch rates per facility, pinned-carrier rules, landing DMs and failure damage, capacity, reset restoring craft. *Risk:* entity lifetime across boarding and launch, detection state, squadron membership.
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
    - Replace "The big one: no capital ships"; rewrite the fixed-mount line to describe facing.
    - Add every ruling here to *Rules decisions and assumptions*.
    - Update `docs/fleet_control_plan.md` with results.

**Plotting:** Phase 5's 100+ ship battles can start after step 8. They only need carriers after step 12.

---

## 9. Decisions and open questions

**Decided by the user (2026-10-29):**
- **Facing:** a ship faces along her thrust. Firing a spinal costs no dodge or Assist Gunner. She cannot burn away from a target and fire a pointed weapon at it. This is built as a general mechanism for spinals and fixed mounts (§2).
- **Fighters:** each fighter stays its own ship. If performance suffers, optimise first, missiles above all (steps 3 and 7). HG's squadron abstraction is not used.
- **Size:** step 7 builds every design up to the 500,000 t Tigress. The code is hardened to 1,000,000 t for ships and beyond for stations (§3, "What bites").

**Still open:**
1. **Rearming carried craft.** The rules as written are summarised in §4, with options A–E. RAW is silent on rearming carried craft, so any option is an Addition.
2. **The 45° cone and its exemptions.** Is a 90° cone (the CRB's vehicle front arc) right for both spinals and fixed mounts? And should missile racks and sandcasters in fixed mounts be exempt?

## What could not be settled from the books

- **No rule ties facing to thrust, or gives a cone for spacecraft,** in HG, the HG FAQ, IN, EC or the CRB.
  - The Companion's vector rules let ships "pivot to any needed direction" every round, even at full Thrust (Companion p. 169). That is the opposite of the user's rule.
  - The 45° cone is carried across from the CRB's vehicle front arc (p. 139).
- **No turnaround time exists for rearming carried craft,** and no rule says whether a docking space allows it. See §4.
- **HG gives space stations no hull-point rule of their own.** The ship bands are assumed.
- **HG never defines a "stationary" target** for spinal fire, or how relative velocity affects landing. Both are assumptions above.
- **Element Cruisers designs predate HG 2022.** EC writes spinals as "Particle Accelerator (improved, 1DD)", an older notation. Its hull figures include pods (the Ghalalk's 27,500 is for 50,000 t with pods). Callisto should build from HG's own Ghalalk (HG pp. 241–243), not EC's.
- **Launch and recovery rates differ between sources.** The HG FAQ says 10 per round. HG's text says one per deck, with a round to move into a tube, and EC's pod follows HG's text. We follow the FAQ.
- **The HG FAQ cites the Wind-class at "pages 266–269"**, one higher than this PDF's printed numbers (265–268). Either the errata was written against another printing or it uses PDF page numbers. The cites here are this PDF's printed pages.
