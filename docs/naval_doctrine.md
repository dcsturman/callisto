# Naval doctrine: findings

Working notes toward a training manual for naval officers at an imagined
Imperial command school, drawn from fights in Callisto. Each finding says what
we saw, the evidence, and whether it rests on the rules as written (RAW) or on
one of Callisto's own rulings. Append as things turn up; reorganise later.

Status marks: **Finding** (seen and explained), **Open** (needs runs or
numbers), **Superseded** (kept for the record).

---

## Armour decides what can hurt you at all

**Finding.** Armour is subtracted from a hit's damage *before* any damage
multiple (High Guard 2022 FAQ errata, "Screens (page 40)": "calculate the
damage of the opposing weapon, subtract Armour, and apply the Damage
Multiple"). A weapon whose dice cannot beat the target's armour does nothing,
however large its multiple: a large particle bay (about 21 a hit, ×100)
against armour 30 is 0 × 100.

- **At capital scale:** in two Keelhaven runs (2026-10-29) the Hadrian battle
  rider's armour 30 stopped every laser, every fusion and particle turret
  battery, and about 1,300 missiles: "189 of 192 hit, 0 damage".
- **At the smallest scale:** a heavy fighter has armour 15, and its weapons are
  a 4D missile (average 14) and a beam laser turret. Heavy fighters can barely
  hurt each other.
- **Doctrine:** before any engagement, ask which of your weapons can damage
  which of theirs. Mass of fire is worthless below the armour line.
- **RAW.**

## Mesons are the capital-killer

**Finding.** Meson weapons ignore armour (High Guard). A 3× meson spinal
averages about 67 before the multiple, so about 67,000 damage, against the
Hadrian's 27,500 hull. Particle spinals lose 3% of their damage per point of
armour (90% against armour 30); railgun spinals 2% per point beyond AP 20.

- **Doctrine:** against heavily armoured ships, fleet striking power is the
  number of meson spinals (and meson bays) you can bring to bear. The meson
  screen is the defence that matters, and it is weak against a spinal.
- **RAW** (HG pp. 35–36).

## Screens act after the damage multiple, one attack each

**Finding.** Callisto first applied screens before the multiple, which made the
Hadrian immune to spinals: a 74-point meson hit was screened to 0 before the
×1,000 was ever applied. The 2022 errata puts screens after the multiple, and
each screen's gunner angles it against one attack a round. Nine meson screens
take a few thousand off a spinal hit of tens of thousands.

- **Doctrine:** screens blunt bay fire; they do not save a capital ship from a
  spinal hit.
- **RAW** (HG 2022 FAQ p. 2). The Callisto bug is being fixed (2026-10-29);
  Keelhaven results from before the fix are void.

## Stealth is a raider's tool

**Finding.** Stealth hides a ship only until she fires, and fleet actions are
decided by sustained fire. It matters for raiders and pickets, not lines of
battle. (From the orders-design study, see `docs/fleet_control_plan.md`.)

- **RAW**, with Callisto's detection model.

## Missiles must overwhelm point defence

**Finding.** In Fighter Screen, a cruiser firing missiles one target at a time
lost 78% of them to point defence. Sharing missiles side-wide and massing them
into packages that saturate the defence won 126 games to 68 against the old
behaviour (`docs/fleet_control_plan.md`, phase 0).

- **Doctrine:** mass missiles against one target, or don't fire them. They are
  excellent against fighters, which carry no point defence.
- **Callisto AI finding** under RAW point defence.

## Missile salvos: armour is paid per missile either way

**Finding.** RAW salvo damage is (4D − armour) × Effect, with Effect capped at
the missiles left (CRB p. 173), and the attack roll gets DM+1 per missile in
the salvo. Armour is subtracted once, before the multiply, so every missile
that counts still pays the target's armour. Callisto rolls each missile
separately (user's ruling, 2026-10-29) and each hit does its own 4D − armour.
Neither version lets missiles beat armour that a single missile cannot.

- **Open:** how many missiles count differs. RAW gives one roll for the whole
  salvo; Callisto gives one per missile. They should agree for big salvos, but
  RAW may favour small ones (one lucky roll counts them all). Compare the
  distributions for salvos of 1–12 against a few armour values.

## The faster side chooses the range

**Finding.** A ship's thrust is shared between moving and dodging. A slower side
does best holding still and dodging, letting the enemy come to it. (From the
orders-design study; it is in the admiral's prompt.)

- **RAW**, with Callisto's no-facing ruling (Companion p. 169).

## Capital designs have probably never been fought

**Open.** High Guard's capital ships were built and costed by the construction
rules. Fleet fights at this scale are close to unplayable at a table, so
Callisto may be the first real test of them. Keelhaven's first runs suggest the
Hadrian is near-immune to anything without a meson. Separate three causes
before changing anything: a Callisto bug, the book's numbers, or the scenario.
Tune scenarios first; change RAW only by deliberate decision.

## Line ships fire their spinals every round

**Finding.** High Guard suggests high-efficiency batteries "to allow a spinal
weapon to be charged over time" (HG p. 34). None of Keelhaven's four capitals
needs them. By their printed Power Requirements, each can drive at full thrust,
run screens and sensors, and fire every weapon including the spinal, every
round, with the jump drive idle:

| Ship | Plant | Basic + M-drive + sensors + weapons + screens | Spare |
|---|---|---|---|
| Hadrian | 48,000 | 10,000 + 30,000 + 22 + 6,305 + 450 = 46,777 | 1,223 |
| Jaellemo | 43,740 | 6,500 + 30,000 + 308 + 3,325 + 450 = 40,583 | 3,157 |
| Wind | 78,000 | 15,000 + 45,000 + 22 + 7,340 + 450 = 67,812 | 10,188 |
| Valiant | 24,000 | 6,000 + 15,000 + 22 + 2,015 + 180 = 23,217 | 783 |

- **Doctrine:** a line ship's spinal is an every-round weapon. A plant hit
  changes that: with little spare, losing Power forces a choice between thrust
  and the spinal.
- **Open:** a ship built small around an oversized spinal (batteries instead of
  plant) would fire every few rounds. Callisto does not model high-efficiency
  batteries; none of the shipped designs uses them.
- **RAW** (printed Power Requirements, HG pp. 238, 247, 255, 266).

## Capital fights are decided by the first spinal exchange

**Finding.** Keelhaven, re-run with the errata's screens (seed 1, 30 rounds,
2026-10-29). Event logs: `scr-close-ev.jsonl`, `scr-net-ev.jsonl`.

- **Heuristic commanders both sides:** four of the five capitals died in three
  rounds (10–12), almost entirely to spinals: the Valiant *Intrepid* to two
  Red spinals (67,000 and 44,000); the Hadrian *Cleon* to the *Dauntless*
  (21,880) and *Sirocco* (72,440), her screens taking 120 and 560; the
  Jaellemo and the Wind traded spinals and both died. 7 spinal hits in all. No
  winner by round 30: Red kept 5.5% of its hull, Blue 22.7%.
- **Network commanders both sides:** the *Dauntless*'s 1× spinal alone killed
  both Red capitals, two hits each. The *Cleon* fired her spinal once and
  missed. Red lost 77 ships, Blue 12 (98% of its hull left), yet still no
  winner by round 30.
- **The Hadrian:** in both runs only spinals hurt her; armour 30 absorbed
  everything else.
- **Doctrine:** a capital-ship action is a duel of spinal mounts that is over in
  two or three rounds once ranges close. Screens are a rounding error against a
  spinal. Everything else in the fleet matters for what it can do before and
  after that exchange, and against the other side's small ships.
- **Caveats:** one seed each. The network was never trained on capital ships
  (retraining comes after carriers), so its run says more about the untrained
  AI than about doctrine. "No winner" with Blue at 98% suggests the scenario's
  end conditions need a look.
- **RAW**, with Callisto's per-missile salvos and batteries.
