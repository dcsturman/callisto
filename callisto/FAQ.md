# Callisto FAQ

This FAQ gives additional detail on the state of the Callisto project.

## The big one: no capital ships

**Callisto supports hulls up to 5,000 tons.** Traveller goes to 1,000,000, and closing that gap is a
substantial piece of work rather than a few missing stats.  Two things block it:

* **Spinal mounts.** There is no such mount in Callisto at all.  They are not simply a bigger bay: a spinal
  weapon carries a x1,000 damage multiple, scales its damage dice with its own tonnage, takes a Hardpoint per
  100 tons, cannot exceed half its ship's displacement, and suffers escalating penalties against small or
  nearby targets.
* **Weapon batteries** — that is, grouping many identical turrets so they are aimed and resolved as one
  attack.  Callisto stores a ship's armament as a flat list and addresses each weapon by its index in that
  list, so a 1,000,000-ton hull would carry ten thousand individually targetable weapons and combat would
  resolve ten thousand separate attacks.  The representation gives out long before the rules do.  (Not to be
  confused with *point defence laser batteries*, which are a specific 20-ton installation and **are**
  supported — see below.)

Defensive **screens** — meson screens, nuclear dampers, black globe generators — are also unimplemented, and
capital ships lean on them heavily.  Designs that carry them are simply missing that protection today; the
design work is written up in `docs/screens_design.md`.

Everything below is a smaller deviation within the tonnage we do support.

## Game Mechanics and differences from Mongoose Traveller (MT)

* **Effect on a successful check.**  Mongoose uses _Effect_ both as a multiplier and as something added to a
  result, and never says what a *successful* check with an Effect of 0 is worth.  Callisto treats that as an
  oversight rather than an intent -- a check that succeeded should accomplish something.  So Effect **floors at
  1 wherever it multiplies**, and is used **as rolled wherever it is added** (weapon damage, where an Effect of
  0 is a real outcome).
* The entire "dogfight" system is ignored in Callisto.  There's no special dogfight rules or _adjacent_ range band.
* Skills are still a work-in-progress and in some cases do not always have impact:
  * Moves are simultaneous for all ships.  Therefore there is no initiative between sides, and no impact from _Tactics_ or _Leadership_ skill. 
  * _Engineering_ skills, while supported during ship construction, have not yet been implemented in play with the exception of _engineering (jump)_.
* Weapons:
  * Supported: beam and pulse lasers, missiles, torpedoes, sandcasters, particle beams, fusion guns, plasma
    guns, railguns, meson guns, mass drivers, ion cannons, repulsors and point defence laser batteries.
  * Damage, range, armour penetration and hit modifiers depend on the **mount** as much as the weapon -- a
    railgun reaches Short from a turret but Medium from a barbette, and a particle beam only reaches Distant
    out of a large bay.
  * Which weapons fit which mounts follows the book, so the editor will not offer a pairing High Guard does not
    sell: there is no torpedo turret, no laser bay and no meson gun below bay size.
  * **Repulsors** can be fitted but do nothing yet.  The book gives them "Special" damage because they deflect
    rather than destroy, and that mechanic is not implemented.
  * **Ion cannons** drain a target's Power rather than damaging its hull, and that Power returns after a round
    or two.  Hardened systems, which the book lets a crew shield from ion damage, are not modelled.
  * **Weapon modifications** -- accurate, high yield, long range, energy efficient and so on -- have nowhere to
    live in a design and are silently dropped.
  * **Mixed turrets** are supported: a turret may hold different weapons, and "only one type may be used in a
    single combat round" (Core Rulebook p. 166).  Two deviations, both deliberate:
    * A mount gets **one attack and one reaction** per round.  The book caps the action but leaves reactions
      to each reaction's own rule; capping at one keeps a mount from both dispersing sand and running point
      defence in the same round.
    * The one-type restriction is read as governing **attacks**.  A mount may attack with one gun and still
      disperse sand with another -- otherwise a sandcaster in a mixed turret could never be used without
      giving up the turret's attack, which no one would choose.  Point defence is *not* allowed alongside an
      attack, since that would be a second active use of the mount rather than a defensive add-on.
  * Ammunition is not tracked.  Missile racks, torpedo tubes and railguns never run dry or reload.
  * Fixed mounts exist and fire, but ships have no facing, so a fixed mount is just a single-weapon mount that
    cannot serve as point defence.
* **Point defence** works differently in shape, because Callisto has no missile salvoes to defend against:
  * Every point-defence weapon a ship queues rolls **once per round** and their Effects total into one pool for
    that ship; each incoming missile then drains a point from it.  The book instead has a gunner concentrate on
    a single salvo.
  * **Point defence laser batteries** are automatic -- no action, no gunner, no decision -- and add 2D/4D/6D to
    that pool for a Type I/II/III.
  * A **torpedo** costs two points where a missile costs one, matching the book's halving of point defence
    against torpedoes.
* Missiles have a different implementation than in Mongoose Traveller.  
  * They are not launched in salvos.  Instead each ship can fire a number of missiles equal to the number of turrets it has.  So a ship with 3 turrets can fire 3 missiles per turn.  Each missile then guides towards its target, adjusting its own course each turn, and has its own chances to hit as a singleton.
  * Torpedoes work the same way, as the book intends -- they are simply larger, harder-hitting missiles that
    are also harder to shoot down.
  * Missiles are effective at close range.
  * Missiles have a burn limit of 10 turns and acceleration of 10G.
  * Missile and torpedo **warhead types** (nuclear, advanced, decoy and the rest) are not modelled; every
    missile carries a standard warhead.
  * Missile launch is detected by all ships.
* **Radiation** is tracked on weapons that have the trait but has no effect on crews yet.
* _Boarding actions_ are outside the scope of Callisto.
* Planets currently do not support gravity.  Currently we found the movement of ships near planets was just difficult hard to get right and there'd often be collisions.  This may be addressed in a future release.
* _Astrogation_ checks for jump are considered to automatically succeed.  _Engineering (Jump)_ skill is used for the check to see if the jump is successful.  

## Sensors, detection and stealth

Callisto implements initial detection (High Guard pp. 76-77) and the loss of
contact with stealthed ships (p. 77). A few points where it makes a ruling the book does
not, or deliberately leaves something out.

* **Nothing can be done to a ship you have not detected.** An undetected ship is
  not there as far as you are concerned: it cannot be fired on, sensor locked,
  jammed or set as a navigation target. Point defence is the exception -- it
  engages missiles, which are objects arriving at you, not the ship that fired
  them, so a battery defends against a launcher you have never seen.
* **Ordinary ships start a scenario detected; stealthed ships do not.** An
  ordinary hull would be found within a round or two anyway, and opening a fight
  with a coin flip over whether the two sides can see each other is worse than
  opening it resolved. A stealthed hull is the case worth playing out, so it has
  to be found. This means a stealth ship reliably gets a free opening round.
* **Detection is re-rolled every round**, for each pair of ships that is not
  already in contact, and only by ships running active sensors.
* **The two High Guard detection tables are treated as one, and the rows stack.**
  Initial Detection (p. 76) and Stealthed Ships (p. 77) share four identical
  rows; the rest differ only because of when each table is used. A ship is as
  loud as the sum of what it is doing -- running active sensors, thrusting,
  running its power plant, firing, and leaking heat from criticals -- whether or
  not anyone has seen it before. Firing therefore helps someone find you even if
  they never had contact, which is what stops a stealth ship shooting from total
  concealment indefinitely.
* **Going dark drops your sensor locks but keeps your contacts.** RAW does not
  settle this. A lock is deliberate, continuous illumination -- the Stealthed
  Ships table charges DM+2 for "sensor locks, electronic warfare or other
  deliberate use of active sensors" -- so it cannot survive going quiet, while
  detection is "maintained under most circumstances" once established. Keeping
  both would make going dark free; dropping both would make it useless.
* **Contact is lost beyond Distant** (50,000 km) and cannot be acquired there,
  since past that range everything is an undifferentiated blip. This is what
  makes running away work.
* **Only stealthed ships can be lost by opening the range**, and only when the
  range actually opens a band. Closing it is never a trigger.
* **Transponder and radio comms are one flag, and it defaults to off.** High
  Guard prints them as a single row at +6 -- the largest on the table -- and
  they are the same emission to anyone listening. It defaults off rather than
  on: RAW expects transponders lit in civilised space, but a ship left
  transmitting by accident is simply found, which would quietly undo stealth for
  any scenario whose author had not thought about it. A scenario opts into the
  noise deliberately, per ship, when the ship is added. Receiving a transmission
  does not count -- listening is passive, only sending gives you away.
* **Sensor hand-offs are not implemented.** Squadrons sharing contacts over
  comms needs a concept of sides, which Callisto does not yet have. When they
  arrive they will also need a way to record that a ship is *transmitting*:
  High Guard's +6 row is "transponder **or radio comms**", so a stealth ship
  that radios a team-mate -- or receives a hand-off -- gives away its position
  while it does. That tension is the point of the rule.
* **Detecting missile launch is not modelled** as a check. The Core Rulebook has
  the target roll to notice an incoming salvo, harder if the firing ship was
  itself undetected. Callisto shows every launch to everyone, as noted under
  missiles above.
* **Electronic warfare against detection** (jamming a sensop rather than
  missiles) is not modelled; there is no RAW for it, and `Jam Comms` is a comms
  action rather than a sensors one.

## Teams and sensor hand-offs

Ships can be assigned to one of four colour-coded teams, or left unaligned.

* **Team-mates always know where each other are.** A squadron shares a plot as
  a matter of course, so no sensor check is needed to find your own wingman --
  even when both ships are stealthed and running silent. This is not a hand-off:
  hand-offs share contacts on *third parties*, and cost Bandwidth and an
  emission to do.
* **Ships will not fire on their own side.** Only attacks are blocked. Plotting
  a course to a team-mate, sensor locking one or jamming one are all still
  possible -- there are legitimate reasons to want each, and the referee is
  better placed than the engine to judge them.
* **Sensor hand-offs** (High Guard p. 77) let a team share its sensor picture.
  Turn on *Hand-off* in the sensor panel and that ship's contacts pass to every
  team-mate automatically. It is automatic in the book too: no check, no action,
  only a point of computer Bandwidth at each end.
* A hand-off **conveys contacts the recipient could never have acquired alone**.
  That is the point: a squadron can post one picket with excellent sensors
  running fully active while everyone else stays quiet and still shoots at what
  the picket sees.
* **Sharing means broadcasting.** Turning hand-off on forces the transponder and
  comms flag on and holds it there -- a ship cannot pass its contacts in
  silence, and the picket pays DM+6 to everyone hunting it. Turning hand-off off
  releases the hold but does not switch the ship to silent on its own.
* **Receiving does not light you up.** Listening is passive.
* Hand-offs **break beyond Distant**, and are a single hop: a ship that receives
  shared data cannot pass it on again.
* **Jam Comms breaks hand-offs.** A ship whose comms are jammed can neither send
  nor receive for that round, so jamming the picket cuts its whole squadron off
  from what it can see. This is the first thing Jam Comms actually does --
  before hand-offs existed it rolled a check and reported the result without
  affecting anything.
* **A contact once shared belongs to the receiver.** It does not lapse if the
  link breaks or the host is destroyed: the crew has the plot, and killing the
  ship that gave it to them does not take it back.

## Rules decisions and assumptions

The sections above describe *what* Callisto does. This one records *why*, with
the citation and the reasoning, so a call can be revisited later without having
to re-derive it. Three kinds of entry:

- **Change** — we do something the book does not say, or contradicts.
- **Assumption** — the book is silent or ambiguous and we had to pick.
- **Omission** — a rule exists and we have not implemented it.

Add to this whenever a rules call is made. Citations are Mongoose Traveller 2nd
edition: **HG** = High Guard (Apr 2024), **CRB** = Core Rulebook (2022 update).

### Combat structure

#### Change — no initiative, so Tactics (Ship) does nothing
Moves and attacks resolve simultaneously for every ship, so there is no turn
order to influence. *Tactics (Ship)* therefore has no effect in Callisto, and
neither does the initiative-related use of *Leadership*. Leadership is still
used for its other purpose, granting boosts.

#### Assumption — Effect floors at 1 where it multiplies, and is used as rolled where it adds
Mongoose uses *Effect* both as a multiplier and as an addend, and a negative or
zero multiplier is nonsense. Callisto floors Effect at 1 wherever it multiplies
and uses it as rolled wherever it is added.

#### Change — a reaction may use a sandcaster in the same turret that attacked
The action economy is one attack and one reaction per mount. Strictly read, a
mixed turret that fires its laser could not then disperse sand. We allow it.
Sand is far less valuable than an attack, and a great many published designs
carry mixed laser/sand turrets that would otherwise be half dead weight. Using a
*laser* for point defence alongside an attack is **not** allowed, which is the
case where the exemption would actually matter.

#### Change — no reaction penalty
An earlier version applied DM-1 to reactions. That rule is from personal combat
(CRB p. 75) and does not apply to ship combat. Removed.

---

### Sensors and detection

#### Change — the two detection tables are treated as one, and the rows stack
HG prints **Initial Detection** (p. 76) and **Stealthed Ships** (p. 77) as
separate tables. Four rows are word-for-word identical between them, including
the same worked example, and the rows that differ do so only because of *when*
each table is used: the first describes an approach, where nothing is shooting
and nothing has taken a critical; the second describes a ship that has already
gone dark, so its power plant is off by assumption.

The book collapses them itself in prose, listing the giveaways as one set: a
powered-down ship stays hidden *"until they reveal themselves with a tell-tale
sign: use of active sensors, transponder, manoeuvre drives or firing a weapon,
just to name a few."*

Callisto uses the union for every check. Precisely what each table gained:

| Row | Initial Detection | Stealthed Ships |
|---|---|---|
| TL difference, +1 per higher TL | had it | had it |
| Active sensors, +2 | had it | had it |
| Manoeuvre drive, +1 per Thrust | had it | had it |
| Transponder or comms, +6 | had it | had it |
| Power plant, +1 | had it | **gained** |
| Fires weapons, +2 | **gained** | had it |
| Damaged, +1 per Severity | **gained** | had it |
| Stealth coating, −2/−4/−6 | had it | applied to both already |

Rows stack, since the book lists them separately.

The firing row is the one that matters. While it appeared only on the
reacquisition table, a stealthed ship could run dark and fire every round at a
contact it already held with its target having *no chance at all* — not a poor
chance, but a DM low enough that the best possible 2D roll could not reach 8.

Two rows are still unmodelled: **extended sensor array, +2** (arrays are not
built), and *"passive sensors only, +0"*, which is a no-op by definition.

#### Assumption — active sensors are needed to acquire a contact, not to keep one
HG p. 77 is explicit that *"attempting to locate a ship with this level of
accuracy requires the use of active sensors"* — that sentence is the tail of the
Initial Detection discussion, which runs over from p. 76. Nothing says what is
needed to *maintain* a contact; p. 77 only offers *"after initial contact,
sensor detection is maintained under most circumstances."*

We rule that maintaining needs nothing. A ship that goes dark keeps every
contact it holds and acquires no new ones. The exception is the one the book
does give: a **stealthed** target whose range opens by a band must be
reacquired.

#### Assumption — going dark drops sensor locks but keeps contacts
RAW does not say. A lock is deliberate, continuous illumination — the Stealthed
Ships table charges DM+2 for *"sensor locks, electronic warfare or other
deliberate use of active sensors"* — so it cannot survive going quiet, while
detection is *"maintained under most circumstances"*. Keeping both would make
going dark free; dropping both would make it useless.

#### Change — transponder and radio comms are one flag, defaulting to off
HG prints them as a single row at +6, and they are the same emission to anyone
listening, so Callisto has one `transmitting` flag. RAW expects transponders lit
in civilised space; we default it **off** anyway. It is the largest row on the
table by some margin, and a ship left transmitting by accident is simply found —
a default that quietly undoes stealth for any scenario whose author had not
thought about it. Scenarios opt into the noise per ship.

#### Assumption — contact cannot be held beyond Distant
HG p. 76 says that beyond Distant range (50,000 km) objects *"simply appear as
blips on a display, difficult to differentiate from each other."* We take that
as a hard limit: no acquisition past Distant, and existing contacts drop. It is
also what makes running away work.

#### Assumption — ordinary ships start a scenario detected, stealthed ships do not
Pragmatic rather than derived. An ordinary hull would be found within a round or
two anyway, and opening a fight with a coin flip over whether the two sides can
see each other is worse than opening it resolved. A stealthed hull is the case
worth playing out. Contacts are only seeded within Distant.

#### Change — the detection check is made once per round
CRB gives the check a duration of 1D minutes. Callisto rolls it once per combat
round for every pair not already in contact.

---

### Teams

Teams are not a Mongoose concept at all; they exist so hand-offs have something
to be shared along. Everything here is invention.

#### Change — team-mates always know where each other are
No sensor check is needed to find your own wingman, even when both ships are
stealthed and running silent. A squadron launched together, is in comms, and is
not hunting itself. This is deliberately *not* a hand-off: hand-offs share
contacts on third parties and cost Bandwidth and an emission.

#### Change — ships will not fire on their own side
Only attacks are blocked. Plotting a course to a team-mate, sensor locking one
or jamming one all remain possible; there are legitimate reasons to want each,
and the referee is better placed than the engine to judge them.

---

### Sensor hand-offs

#### Assumption — hand-offs are automatic, not an action
The only thing HG says a hand-off *requires* is a point of computer Bandwidth at
each end. There is no check and no step, so it is a standing setting rather than
something the sensop spends an action on.

#### Assumption — only transmitting lights you up; receiving does not
Listening is passive. The Bandwidth cost at both ends is a computer-capacity
limit, not an emission. Turning hand-off on therefore forces `transmitting` on
and holds it there: a ship cannot share its contacts in silence.

#### Assumption — an inherited contact belongs to the receiver outright
It does not lapse when the link breaks or the host is destroyed. The crew has
the plot, and killing the ship that gave it to them does not take it back.

#### Change — Jam Comms breaks hand-offs
RAW lets a target break a squadron's link with an electronic warfare check. We
attach that to the existing *Jam Comms* action: a jammed ship can neither send
nor receive for that round. Before this, Jam Comms rolled a check and affected
nothing at all.

---

### Weapons

#### Assumption — a torpedo barbette holds three and fires one at a time
Keeps it below a small bay, which is the intent of the progression. Worth
revisiting.

#### Change — no missile salvoes
Each launcher fires its own missiles, each guiding and rolling to hit
individually, rather than salvoes tracked as units. Point defence and the
torpedo halving are re-expressed to match; see `FAQ.md`.

#### Omission — detecting missile launch
CRB has the target roll to notice an incoming salvo — Routine (6+), or Average
(8+) if the firing ship was itself undetected, with DM+1 per full 10 missiles.
Callisto shows every launch to everyone. Implementing it needs salvo grouping,
which does not exist.

---

### Not implemented

Recorded so nobody assumes they were considered and rejected.

- **Extended sensor arrays** (+2 to be detected, and longer detail range).
- **Electronic warfare against a sensop** as distinct from jamming comms. There
  is no RAW for it, and *Jam Comms* is a comms action.
- **Squadron hand-off limits by Bandwidth count** — the cost of one point at each
  end is enforced, but a host is not capped at the number of recipients its
  spare Bandwidth allows.
- **Sensor hand-off relaying.** The single-hop rule *is* enforced.

---

## Known gaps being considered for future versions

* **Capital ships**: hulls above 5,000 tons, which needs spinal mounts and weapon batteries before anything
  else -- see the top of this document.
* **Defensive screens**: meson screens, nuclear dampers and black globe generators.
* **Ship Design Editor**: new ship designs can be created and saved by users.
* **Scenario editor**: entire scenarios can be created and saved by users.
