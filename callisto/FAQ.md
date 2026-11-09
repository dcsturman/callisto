# Callisto FAQ

This FAQ gives additional detail on the state of the Callisto project.

## The big one: no capital ships

**Callisto's shipped designs stop at 75,000 tons.** Traveller goes to 1,000,000, and closing that gap is a
substantial piece of work rather than a few missing stats.  The rules that change with size are now in --
spinal mounts, weapon batteries, High Guard's size thresholds on criticals and its large-ship critical
tables, the frozen watch, radiation on crews and one sensor station per 7,500 tons (see *Critical hits*
below).  The first four capitals ship with the *Keelhaven* scenario (see *The capital designs* below); the
rest of High Guard's capitals, and carriers' embarked craft, are still to come:

* **Spinal mounts** are supported: a design gives a weapon the mount `{"Spinal": 3}`, a multiple of its
  type's Base Size, and it has the book's x1,000 damage multiple, dice and Power by the multiple, a Hardpoint
  per 100 tons, the half-hull limit, and the penalties against small or nearby targets.  See *Weapons*
  below.
* **Weapon batteries** — grouping many identical turrets, barbettes or bays so they are aimed as one weapon
  — **are** supported: a design gives a weapon entry a `count`, and it becomes batteries of up to 4, 8, 16 or
  32 mounts by hull size (Imperial Navy p. 113), each with one gunner and one target.  See *Weapons* below.
  (Not to be confused with *point defence laser batteries*, a specific 20-ton installation, also supported.)

Defensive **screens** are in for meson screens and nuclear dampers (`ScreenType`, angled by a screen
gunner each round), played to the High Guard 2022 FAQ errata: after armour and the Damage Multiple,
each screen against one attack a round.  See *Screens* below.  Black globe generators are not in,
being a referee's artefact rather than something a design buys.  The design record is
`docs/screens_design.md`.

### The capital designs

The Hadrian battle rider, the Jaellemo planetoid monitor, the Wind strike carrier and the Valiant light
cruiser are built from their High Guard 2022 pages, with the HG FAQ's spinal multiples (meson 3×, 2×, 3×
and 1×). Their turrets and bays are entries with a `count` (the Hadrian's 50 medium missile bays are one
entry), which the loader forms into batteries of up to 16; the stated magazine is the mounts' own load plus
the book's storage, as for the Dragon and the Sloan. What their pages carry that Callisto does not model:

* **Embarked craft.** The designs carry their berths, tubes and decks (see *Carriers and small craft*
  below), and Keelhaven starts the Wind's 80 light fighters and the Jaellemo's 30 heavy fighters aboard.
  The Hadrian's six pinnaces and the Valiant's five modular cutters have their docking spaces but no
  scenario puts craft in them. The crew figures still count the pilots. The Jaellemo carries her fighters'
  2,400 missiles in stores, with no launcher of her own to fire them: they rearm her fighters.
* **Railgun bay ammunition.** Railgun bays fire without using any; only a railgun spinal counts rounds.
  The Hadrian's 200 stored shots are left out.
* **Sensor fittings beyond the grade**: distributed and extended arrays, enhanced signal processing, deep
  penetration scanners and the Jaellemo's three Advanced suites are all just Advanced sensors.
* **Fittings with no play effect**: the command bridge, the Jaellemo's armoured bulkheads and backup
  computer, fuel processors and scoops, UNREP, briefing rooms, medical bays, workshops and the armoury.
* **The Jaellemo's hull.** HG prints a buffered planetoid but Hull 31,250, which is a planetoid's +25% on
  50,000 tons rather than a buffered planetoid's +50%. The design keeps the printed figure.
* None of the four has low berths, so none has a frozen watch.

Everything below is a smaller deviation within the tonnage we do support.

## Game Mechanics and differences from Mongoose Traveller (MT)

* **Effect on a successful check.**  Mongoose uses _Effect_ both as a multiplier and as something added to a
  result, and never says what a *successful* check with an Effect of 0 is worth.  Callisto treats that as an
  oversight rather than an intent -- a check that succeeded should accomplish something.  So Effect **floors at
  1 wherever it multiplies**, and is used **as rolled wherever it is added** (weapon damage, where an Effect of
  0 is a real outcome).  Missiles and torpedoes are the exception: they do their damage dice alone (see
  Weapons below).
* The entire "dogfight" system is ignored in Callisto.  There's no special dogfight rules or _adjacent_ range band.
* Skills are still a work-in-progress and in some cases do not always have impact:
  * Moves are simultaneous for all ships.  Therefore there is no initiative between sides, and no impact from _Tactics_ or _Leadership_ skill. 
  * _Engineering_ skills now carry repairs of their own systems, _Mechanic_ repairs everything else,
    and _Electronics (remote ops)_ flies the repair drones.
* Weapons:
  * Supported: beam and pulse lasers, missiles, torpedoes, sandcasters, particle beams, fusion guns, plasma
    guns, railguns, meson guns, mass drivers, ion cannons, repulsors and point defence laser batteries.
  * Spinal mounts: mass drivers, mesons, particle accelerators and railguns (High Guard pp. 34-36).
  * Damage, range, armour penetration and hit modifiers depend on the **mount** as much as the weapon -- a
    railgun reaches Short from a turret but Medium from a barbette.  (A large particle bay reaches Very
    Long, not the book's Distant: the HG FAQ, p. 1, corrects the Large Bay Weapons table.)
  * Which weapons fit which mounts follows the book, so the editor will not offer a pairing High Guard does not
    sell: there is no torpedo turret, no laser bay and no meson gun below bay size.
  * **Repulsors** work only against missiles: a repulsor bay on point defence deflects 1D x Effect missiles a
    round, doubled for a medium bay and five times for a large one (High Guard p. 33). Pushing ships and
    other objects about is not implemented.
  * **Ion cannons** drain a target's Power rather than damaging its hull, and that Power returns after a round
    or two.  A house rule spills a tenth of that into the computer's Bandwidth, which a hardened
    (/fib) computer shrugs off -- the only thing hardening does here.
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
  * **Launchers and sandcasters hold a load and reload from stores** (see *Addition — magazines* and
    *Rule enforced, with an addition — reloading a mount* below).  Railgun and mass-driver rounds are not
    tracked: those never run dry.
  * Fixed mounts exist and fire, but ships have no facing, so a fixed mount is just a single-weapon mount that
    cannot serve as point defence.
* **Point defence** works differently in shape: it defends the ship, not against one salvo.
  * Every point-defence weapon a ship queues rolls **once per round** and their Effects total into one pool for
    that ship; each incoming missile then drains a point from it, missile by missile, across every salvo that
    arrives that round.  The book instead has a gunner concentrate on a single salvo.
  * **Point defence laser batteries** are automatic -- no action, no gunner, no decision -- and add 2D/4D/6D to
    that pool for a Type I/II/III.
  * A **torpedo** costs two points where a missile costs one, matching the book's halving of point defence
    against torpedoes.
* Missiles have a different implementation than in Mongoose Traveller.  
  * A launch from one mount is one **salvo**: a triple turret throws 3, a barbette 5, a small bay 12, a large bay
    120.  The salvo flies as one object, guiding towards its target and adjusting its course each turn, and is
    reported as one line ("7 of 12 hit, 31 damage").  But each missile in it still rolls to hit on its own and,
    if it hits, does its own damage -- the book's shortcut of multiplying one missile's damage by the Effect is
    not used.
  * Torpedoes work the same way, as the book intends -- they are simply larger, harder-hitting missiles that
    are also harder to shoot down.
  * Missiles are effective at close range.
  * Missiles have a burn limit of 10 turns and acceleration of 10G.
  * Missile and torpedo **warhead types** (nuclear, advanced, decoy and the rest) are not modelled; every
    missile carries a standard warhead.
  * Missile launch is detected by all ships.
* **Radiation** wears the crew down, as HG p. 31 has it: every 10% of hull lost to Radiation-trait weapons
  is a cumulative DM-1 on everything the crew does, every 40% if the design has `radiation_shielding`,
  which meson weapons ignore.  See *Critical hits* below.  Rads on individuals are not tracked.
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

Every place Callisto parts company with the book, with the citation and the
reasoning, so a call can be revisited later without re-deriving it. Rules we
follow as written are not listed: if a mechanic is missing from this section,
Callisto does what Mongoose says.

- **Change** — we do something the book does not say, or contradicts.
- **Assumption** — the book is silent or ambiguous and we had to pick.
- **Interpretation** / **Decision** — the book admits two readings, or leaves
  the choice to the table, and we made one.
- **House rule** — a deliberate departure, for play reasons.
- **Addition** — something the book leaves to the referee that Callisto has to
  track to run at all.
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

---

### Movement and range

#### Change — movement is Newtonian, not the Core Rulebook's Thrust-cost table
The range bands are the book's exactly (CRB p. 167). What differs is how ships
get between them. The CRB prices each band as a Thrust cost paid over as many
rounds as it takes, with no momentum, so a ship that stops thrusting stops
changing range; Callisto integrates real kinematics over the round, so velocity
accumulates and is kept.

Disengagement is the practical difference, and it is far easier here: Medium to
Distant at Thrust 2 is 21 rounds by the book and 7 in Callisto. This is not a
house invention — the *Traveller Companion*'s Vector-Based Space Combat
(pp. 170-178) is the same model with the same bands, though it accumulates
velocity at about half the physical rate.

---

### Sensors and detection

#### Change — the two detection tables are treated as one, and the rows stack
HG prints Initial Detection (p. 76) and Stealthed Ships (p. 77) as separate
tables. Four rows are word-for-word identical, and the rows that differ do so
only because of *when* each table is used. Callisto uses the union for every
check, rows stacking: the first table gains *fires weapons, +2* and *damaged,
+1 per Severity*, the second gains *power plant, +1*.

The firing row is the one that matters. While it belonged only to the
reacquisition table, a stealthed ship could run dark and fire every round with
its target having no chance at all — not a poor chance, but a DM low enough
that the best possible 2D roll could not reach 8.

Two rows are unmodelled: *extended sensor array, +2* (arrays are not built),
and *passive sensors only, +0*, a no-op by definition.

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

#### Change — scenarios open with no contacts at all
Ships start not knowing about each other, whatever they are, and the first
detection pass runs at the end of the opening round. A scenario *author* can
override this per ship when adding one, which is scenario creation rather than a
default: that is how you set up an engagement already in progress instead of an
approach.

#### Assumption — a course can be plotted toward a ship with no contact
Nothing can be *done* to an undetected ship, but a course can be laid toward
one. HG p. 76 has a ship beyond detection as "an undifferentiated blip": the
sensors know something is there and where it is going, they just cannot say
what it is. So the navigation computer accepts a blip as a target, using its
position and velocity but **not** its acceleration -- a ship you have no contact
on shows `?` for thrust everywhere else, and the computer must not know more
than the sensors do. The pilot is told the course assumes the blip holds its
velocity. Without this, a player with no contact was left hovering over a dot
with a calculator, which is not play.

#### Change — the navigation computer never refuses a reachable target
The flight solver finds a rendezvous: arrive *and* match velocity. Against a
target that out-accelerates you and is already receding there is no such
course, and it used to come back as an error -- in every scenario built so
far, since the Oars and the Tai'ao can never rendezvous with a burning
Executor. It now tries three things in turn and reports which it got:
**intercept** (rendezvous), **pursuit** (cross the target's projected position
at any speed, the missile guidance problem pointed at a ship), and
**shadowing** (burn flat out at where the target will be next turn, which is
the direction that lets the range grow least -- closed form, cannot fail).
Re-plotting each turn walks back up the ladder as the geometry improves.

#### Change — the detection check is made once per round
CRB gives the check a duration of 1D minutes. Callisto rolls it once per combat
round for every pair not already in contact.

---

#### Addition — one Jam Missiles action per 7,500 tons
"Assume that a ship has one on-duty sensor operator for every full 7,500 tons", each able to jam a
different salvo (HG p. 34). The sensor station's Jam Missiles order is the first station's, and works as
it always has, across everything inbound. The other stations go automatically to the largest other
salvoes still inbound, one station to a salvo, each a check at the same operator's skill without the
captain's inspire, and never more stations than there are salvoes left: a 75,000-ton ship would
otherwise need ten orders a round. A ship under 15,000 tons has one station.

### Teams

Teams are not a Mongoose concept at all; they exist so hand-offs have something
to be shared along. Everything here is invention.

#### Change — a captain can concentrate the sensop on one particular ship
Detection happens every round for free, and searching is not an action: it costs
no sensor slot and there is nothing to order. What a captain can do is put the
sensop's attention on one check, which is worth the usual DM+1.

The boost therefore names a *pair* rather than a ship, and it appears in the
action list for every ship of another side that has not been found yet — the
only place leadership can affect detection at all. Nothing is offered against a
team-mate, whose position is known anyway.

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

#### Assumption — only transmitting counts as communication; receiving does not
Sending a hand-off counts as communication for detection purposes, and so adds
the transponder-or-comms modifier to anyone trying to detect that ship.
Receiving one does not: listening is passive, and the Bandwidth cost at both
ends is a computer-capacity limit rather than an emission.

Turning hand-off on therefore forces `transmitting` on and holds it there — a
ship cannot share its contacts in silence. Jamming comms prevents hand-offs
entirely, in both directions, for the round.

#### Assumption — an inherited contact belongs to the receiver outright
It does not lapse when the link breaks or the host is destroyed. The crew has
the plot, and killing the ship that gave it to them does not take it back.

---

### Weapons

#### Change — a missile's Effect adds nothing to its damage
A salvo "rolls for damage as if for a single missile ... but do not apply the
Effect of the attack roll. Instead, any damage is multiplied by the Effect ...
but the Effect cannot be higher than the number of remaining missiles in the
salvo" (CRB p. 169). Callisto rolls each missile in a salvo separately
instead, so a missile or torpedo that hits does its damage dice and nothing
more. Every other weapon still adds the Effect.

#### Rule enforced — bay weapons struggle against small ships
"All bay weapons suffer DM-2 when attacking targets of 2,000 tons or less and
DM-4 when attacking targets of 100 tons or less. Missile and torpedo salvoes do
not use these modifiers" (HG p. 31). It shows in a shot's breakdown as "small
target", the same term as the torpedo's own DM-2; a torpedo bay is a salvo and
takes only the torpedo's.

#### Rule enforced — called shots only at Short, and not with missiles
"Ships at a range of Short or less may attempt to make called shots with
direct-fire weapons (not missiles or torpedoes)" (CRB p. 169). The server holds
every client to this: a called shot from further out, or from a launcher, is
fired as an ordinary shot, without the DM-2.

#### Assumption — a torpedo barbette holds three and fires one at a time
Keeps it below a small bay, which is the intent of the progression. Worth
revisiting.

#### Change — a salvo rolls missile by missile
A launch from one mount flies as one salvo, as the book has it, and is shown
and reported as one: one object on the plot with its count, one line in the
results. But "each one rolls to hit": every missile in it makes its own attack
roll and does its own damage dice, so a salvo of twelve does exactly what
twelve lone missiles would. The book's single roll with damage multiplied by
the Effect (CRB p. 169) is not used.

#### Change — point defence and jamming defend the ship, not one salvo
The book's point defence gunner and electronic warfare operator each pick a
salvo (CRB p. 171, HG p. 34). In Callisto a ship's point defence -- gunners on
point defence and its batteries -- is one pool for the round, and it takes the
missiles arriving that round one at a time, whichever salvo they came in, a
torpedo costing two points. Jam Missiles likewise destroys its Effect's worth of
missiles chosen from everything inbound at the ship, across salvoes. A salvo
that loses some missiles flies on with the rest.

#### Rule enforced — screens come after the Damage Multiple
The High Guard 2022 FAQ (p. 2) replaces the screens text on HG p. 40: "First, calculate the damage of
the opposing weapon, subtract Armour, and apply the Damage Multiple. Make a Gunner (screen) check for
each screen. Every successful check reduces the damage of the attack by the amount indicated in the
Damage Reduction column of the Screens table multiplied by the Effect of the check. A gunner may only
angle screens against one attack per round." It deletes the p. 41 Angle Screens box and the meson
screen's one-check-for-a-battery sentence. A meson screen takes 2D × 10 × Effect off a meson weapon's
damage after its ×10, ×20, ×100 or ×1,000; a nuclear damper 2D × Effect off a fusion weapon's after its
×3 or ×10. The spinal text says the same (HG p. 35): after the ×1,000, "apply damage reduction from any
meson screens as normal". So screens matter against meson and fusion bays and barely dent a spinal:
nine meson screens take about 1,300 off a 3× meson spinal's 65,000.

Every screen has its own gunner and makes its own check; a failed check takes nothing off. The
results hover shows the order: "x1000 mount = 64000, -1260 screens (9 screens) = 62740".

#### Decision — each screen meets one attack: the largest not yet screened
"A gunner may only angle screens against one attack per round", and each screen is its gunner's, so a
screen is angled at exactly one hit a round and its whole reduction goes on that hit; what the hit did
not need is lost. Nobody is asked which hit: every attack in the round is rolled first, then each ship's
screens are taken in order, each going on the largest hit it can meet that no screen is on yet. When
every such hit has one, a further screen goes on whichever hit has the most damage left, so several
screens can be on one attack, each its own gunner's one. A screen whose check failed is angled all the
same. Hits a ship's screens cannot meet land as they are rolled, as they always have.

#### Addition — weapon batteries
A design may give a weapon entry a `count`: `{"kind":"Beam","mount":{"Turret":3},"count":200}` is 200
triple beam turrets. They are split into batteries no bigger than the hull allows -- 4 mounts up to 999
tons, 8 to 9,999, 16 to 99,999 and 32 above (IN p. 113); a hull under 100 tons takes the first row -- and
each battery is one weapon: one gunner (IN p. 114), one target a round, one entry on the power board and
one place for a critical to land. Turrets, barbettes and bays can be batteries (IN pp. 99, 113); an entry
of several fixed mounts or point defence batteries is that many separate weapons. A design without counts
is exactly as it was.

#### Interpretation — every mount of a battery rolls its own attack
IN's usual way, "a separate attack roll for each turret or barbette" (p. 113). The results give one line
for the battery: "particle beam barbette ×12 (battery) -> X: 7 of 12 hit, 213 damage". A battery of four
does exactly what four single mounts with the same gunner would.
- **Omission: harmonised and focused fire** (IN p. 113). They may come later as a setting per battery.
- **A missile battery is one salvo:** sixteen small bays throw 192 missiles at one target together.
- **A called shot** is called for the whole battery: every shot takes the DM-2 and aims at the system.
- **The captain's Assist Gunner** +1 goes on the first fire order of the round, which for a battery is
  every mount in it.

#### Decision — a battery defends with every mount
A laser battery on point defence makes a check for every mount in it, at its one gunner's skill, and they
all add to the pool. A sandcaster battery throws a cloud from every mount. Against a laser battery, each
mount's shot can draw a cloud while the target has any to throw, as that many single shots would.

#### Rule enforced — a weapons critical on a big ship takes a share of one battery
On a ship over 5,000 tons a Weapons critical of severity 2 or more picks one battery, bay or spinal at
random by entry and takes a share of its mounts from the Cargo and Weapons Critical Hit Severity table
(HG p. 27): D3 x 10% disabled at severity 2, 1D x 10% destroyed at 3, all remaining destroyed at 4, and
at 5 and 6 all destroyed with the Hull's severity raised by one. The share is rounded up and is never
less than one mount, so a spinal, being one mount, goes out at any severity of 2 or more. The battery
fires, defends and draws Power for the mounts it has left ("×12 (3 down)" on the gunner card); disabled
mounts come back when the Weapon system is repaired, destroyed ones do not. Severity 1 stays the CRB's
DM-1. On a ship of 5,000 tons or less a Weapons critical switches off the whole weapon it picks, as the
CRB has it, a battery included.

#### Addition — spinal mounts
A design gives a weapon the mount `{"Spinal": 3}`: one weapon the length of the ship, a whole multiple of
its type's Base Size (HG pp. 34-35). Mass drivers, mesons, particle accelerators and railguns come as
spinals, and nothing else does. Its damage dice and Power are the table's figures times the multiple, so a
3× meson is 18D and draws 3,000 Power; its range and traits are the type's; its damage multiple is 1,000
(HG p. 29). A spinal is always a single weapon with one gunner and never a battery; an entry with a
`count` is that many spinals, which the book allows but calls rare (HG p. 35).

#### Rule enforced — a spinal is sized by its multiple and its TL
Its tons are the Base Size times the multiple, less 10%, 15% or 20% for building it one, two or three TLs
above the weapon's own (HG p. 35, Spinal Mounts Improvement): a TL15 3× meson is 18,000 tons, the
Wind-class's (HG p. 266). It takes a Hardpoint per 100 tons, and it may not be bigger than its type's
largest or than half the ship (HG p. 34). A design that breaks either, or carries a spinal below its
weapon's TL, is refused when it loads, and Add Ship refuses such a ship.
- **Decision — the weapon's TL is the design's unless stated.** Published ships often carry an older
  spinal: the Azhanti High Lightning is a TL14 hull with a TL12 particle spinal (HG p. 258, as the HG
  FAQ corrects it). `{"Spinal": {"multiple": 2, "tl": 12}}` says so. The TL only sets the tonnage; the
  weapon fires the same at any TL.

#### Rule enforced — spinals struggle against small and close ships
- **Size:** DM-4 against a ship of 10,000 tons or less, DM-8 against 5,000 or less (HG p. 34), in a
  shot's breakdown as "small target".
- **Under 2,000 tons:** a spinal "cannot attack targets of less than 2,000 tons unless they are
  stationary" (HG p. 34). The gunner is told before anything is spent, and the gunner's card does not
  offer the shot. A target that is stationary takes the DM-8.
- **Close range:** the Spinal Weapon Range Adjustments, which grow with the *firing* ship's size (HG
  p. 35): from DM-1 at Short on a ship under 10,000 tons to DM-10 at Adjacent on one over 250,000. The
  table's columns meet at 50,000 and 250,000 tons; a ship of exactly that size takes the smaller column.
- **No facing:** a spinal fires at any bearing, like any other weapon (Companion p. 169). Its
  clumsiness is these DMs.

#### Interpretation — "stationary" means she held still last round
HG never says what stationary is, and in Newtonian space nothing is at rest. A ship under 2,000 tons is
open to a spinal if, in the last round flown, she spent no Thrust and took no evasive action (a pilot
holding Thrust back to dodge). A ship drifting with dead drives, or simply holding still, can be hit. A
ship fresh from a scenario has not yet flown a round, so she has not held still. "Caught in the blast by
accident" is not modelled: there is no splash damage.

#### Interpretation — the close-range table by distance, not by band
Callisto folds Adjacent and Close into Short (see *Point Defence reaches one band, then two*), but
distance is continuous, so the spinal table uses the Core Rulebook's own limits: Adjacent under 1 km,
Close to 10 km, Short beyond. Only spinal fire looks at these.

#### Rule enforced — particle and railgun spinals lose a share to armour
"The damage dealt by a particle spinal mount is reduced by 3% per point of armour ... before applying the
Damage Multiple" (HG p. 36), and a railgun's by 2%. Taken with the ×1,000 in one step, so nothing is
rounded away first. Armour 34 stops a particle spinal entirely. A meson spinal ignores all armour (HG
p. 35); meson screens work against it as normal, after the ×1,000 (see *Screens come after the
Damage Multiple*).
- **Interpretation — a railgun spinal has AP 20 and the 2% rule.** The table gives it AP 20 and the text
  2% a point, without saying how they combine. The AP comes off the armour first, and what is left costs
  2% a point: against armour 20 or less a railgun spinal loses nothing, which is what AP 20 means
  anywhere else.

#### Rule enforced — a spinal hit that does damage is always a critical
"Spinal mount weapons can always (and will!) cause critical hits" (HG p. 27), read literally: every spinal
hit that gets damage through scores a critical, at the usual Effect − 5 but never less than severity 1.
Sustained damage adds its own as for any hit, and a 10⁵-point hit can cross several tenths of a hull at
once.

#### Decision — all the dice are rolled
A spinal can roll 60D or more, and HG suggests rolling a tenth of them and multiplying by ten (HG
p. 35). That is a convenience for a referee with a fistful of dice; the server rolls all of them, which
gives the right spread.

#### Addition — spinal rounds feed the mount from stores
A railgun spinal "come[s] equipped with five rounds" (HG p. 36), and a mass driver's are 50 tons an attack
(HG p. 35). They are counted as `rounds` in the magazine: five for each railgun spinal, plus any the
design states; a mass driver has only what is stated. Each attack spends one, and the spinal feeds from
stores, so it never needs a Reload. Railgun and mass driver rounds share one count, since no design
carries both.

#### Rule enforced — mass drivers are bombardment weapons
Mass drivers carry the Orbital Bombardment trait, in a bay (HG pp. 32-33) or a spinal (HG p. 35), and so
take DM-12 against anything that can manoeuvre (HG p. 30): any ship with Thrust left. There are no ground
targets, so they exist so that published designs can be built.

#### Decision — a spinal draws Power like any other weapon
A spinal's Power is on the engineer's board with the other weapons (see *a power budget, with weapons in
it*). The high-efficiency batteries HG suggests for charging one over several rounds (HG p. 35) are not
modelled, and no rule gives a charge time, so a spinal fires every round it has Power.

#### Omission — weapon readiness
Imperial Navy gives a spinal 4D+12 minutes from weapons safe to ready (IN p. 82). Scenarios open at
action stations, so it does not arise.

#### Omission — detecting missile launch
A launch can be missed and picked up later (HG p. 76: Routine 6+, or Average 8+
if the firing ship is itself undetected, DM+1 per full 10 missiles). Callisto
shows every launch to everyone. The blocker is presentation rather than rules:
ship contacts are already per-observer, but it is not clear how to show *which
missiles are visible to whom* without the view becoming unreadable, least of
all for a referee watching every side at once. The DM's salvo size is now
there to be read, since a salvo carries its count.

---

### Carriers and small craft

High Guard's carrier rules (pp. 57, 61-63, 104) and the HG 2022 FAQ's rates, as Callisto plays them.
The plan (`docs/capital_ships_plan.md` §4) has the reasoning.

#### Addition — craft aboard are not on the plot
A craft in a carrier's berth is not an entity in space. She is stored inside her carrier whole -- hull,
criticals, crew, loads, orders, squadron -- and nothing can detect, target or order her until she is
launched. On the wire the carrier carries her craft in `embarked`, with `flight_ops` for what is going
out or coming in and a `flight_deck` summary (berths, berths free, craft a round through the tubes and
through the decks, and the largest craft they take); each craft knows her `carrier`. A scenario file
lists every ship at the top level and marks a craft that starts aboard with `aboard: "<carrier>"`; a
carrier is any design with a docking space or a full hangar, and a craft aboard anything else is a
broken file.

#### Rule enforced, with a change — launching
The carrier's captain gives a *Launch* order naming the craft. Through a powered launch tube that takes
her size, a tube releases up to ten craft a round (HG 2022 FAQ p. 3); they appear as the round ends, at
the carrier's position and velocity and five kilometres ahead of her, and act from the next round. RAW
lets them thrust and fire in the round they launch, but in a simultaneous game nobody has plotted for a
craft that did not exist when the orders were written. Craft named beyond the tubes' capacity wait for
next round's order -- a carrier with a working tube never falls back to her berths. A carrier with no
working tube releases through the berths: D3 rounds each (1D for a craft of 2,000 tons and up), several
in parallel, the carrier pinned until the last is out (HG p. 61). A full hangar takes the same rounds as
a docking space; HG gives the count under docking space only. Launch is one round's order and does not
replace the captain's leadership check: flight operations are an order, not the captain's own roll.

#### Decision — a pinned ship coasts, and reacts
"Neither ship can expend Thrust or make attack rolls" (HG p. 61): a ship pinned by a berth operation
flies no plan this round (the plan is kept for when the deck is clear), has no dodge, and her Fire
orders are set aside for the round and kept for the next. Reactions stand -- point defence, sand,
screens -- since the deck is busy and the gunners are not. `pinned` is on the wire while it holds.

#### Rule enforced, with assumptions — recovering
The craft's pilot gives a *Land on <carrier>* order, which stands until she is aboard or it is cleared.
It is resolved at the end of the round's movement, in the craft's name order. She must be her side's
ship, within Short range of the carrier (HG p. 104), and closing at no more than her Thrust in G times
a minute of burn (60 seconds) relative to the carrier -- an assumption, since HG's abstract rules have no
relative velocity; a crippled craft keeps a G's worth, so she may still drift into a match. There must
be a berth free that takes her, or she waits.

With a powered recovery deck that takes her: a Routine (6+) Pilot check, DM-1 for every G of Thrust the
craft or the carrier spent that round, DM-1 for every other craft landing on that carrier that round
that is not of her squadron (a craft with no squadron counts every other), and the crew DM. A deck
takes ten craft a round (HG 2022 FAQ p. 3); the rest wait. Success puts her aboard; failure does 1D x
(the craft's G + the carrier's G) to both, ignoring armour, and can wreck either, and she stays
outside to try again. Without a deck: she matches the carrier's velocity and docks through a berth in
D3 rounds (1D if large), both ships pinned, no check, aboard as the round in which the count runs out
ends.

#### Decision — launched craft take the carrier's picture and her air group
A craft comes out with her carrier's contacts (not her sensor locks), the way a hand-off would give
them. She joins the squadron the scenario named for her, or "<carrier> air group", spilling into
"<carrier> air group 2" and so on at the twenty-ship cap, and takes up the squadron's and the fleet's
orders.

#### Decision — one round aboard rearms a craft from the carrier's stores
A craft that has spent one whole round aboard, in any berth, has her launcher and sandcaster mounts
refilled from the carrier's magazine, in order, as far as the stores go, at no action cost to anyone.
A craft that lands at the end of round N is rearmed at the end of round N+1; one ordered out through
a tube in N+1 leaves without it. The carrier's stores run down and run out. Repairs aboard are not
modelled: a damaged craft comes out as she went in.

#### Decision — a launch tube is fed before the drive
A launch tube and a recovery deck draw Power (HG pp. 62-63) and take it ahead of the manoeuvre drive,
like the weapons: they are in the printed Power budget. An unpowered tube launches nothing and the
carrier falls back to her berths; an unpowered deck recovers nothing. Other powered features (a
hologram projector) still come after the drive.

#### Rule enforced — the carrier's loss is her craft's loss
A carrier destroyed takes every craft in her berths with her, each with a line. A carrier that jumps
takes them with her, and they count as departed with her.

#### Decision — what the consoles say before the engine does
The helm's *Land on* shows the engine's gates (own side, pilot's station, Short at the end of the
round, the velocity limit, a berth that takes her) worked out from the committed plans, and the
check's terms and odds, so a landing that will be turned away is marked before it is ordered; the
order can still be given, since it stands from round to round, and only the engine knows how many
craft the decks have taken that round and what the dice say. The Flight Deck card cuts a pick at
the tubes' rate and says how many wait. A wing in a carrier's berths is listed in the roster as
"<wing> (aboard <carrier>)" and nowhere else: she is not a target, a contact or a marker.

#### Decision — the crews run the deck unless told otherwise
An AI side's carriers and craft run their own flight cycle (the user's ruling, 2026-10-29): the first
launch comes from the admiral's orders or from the crews when an enemy is in contact within Very Long
range, and after that the crews handle it. A carrier launches as many ready craft as her tubes take a
round, one wing before the next (a craft is ready when nothing in the stores could still fill a mount
of hers, and she has something to fight with); she launches nothing while pinned or about to jump. A
craft goes home -- closing on her carrier and matching her velocity, then coasting on the approach
(within Short and inside the velocity limit) and giving *Land* only while her carrier is burning no
more than 1 G, since the landing check loses a point for every G either ship spends, and only when
it is her wing's turn (of the craft on the approach, the first squadron by name; the check also loses
a point for every other craft coming in that is not of her squadron) -- when her
launchers are spent and the carrier's stores can refill them (a laser fighter is never spent), when
no enemy is in contact at all, or when her carrier is running Jump Control; after a round aboard she
is rearmed and launches again. A craft back in the fight withdraws any *Land* she had standing. An admiral's `flight` step overrides all of this for a squadron: `launch` (all, a count
to have out, or named wings), `recover` (every craft of the squadron's carriers comes home), or `hold`
(craft aboard stay aboard); a craft told to hold still goes home once spent, since rearming is the
crews' cycle rather than an order. Two limits follow from the physics: a carrier burning away at her
craft's full thrust cannot be caught, so a carrier that wants her craft back must ease off; and
*Launch* is the captain's order, so the ten a round are chosen when the orders are given, not from
what lands that round.

#### Omission — docking clamps, battle riders and tenders
A design may state docking clamps (`DockingClamp`), and they are read and shown, but nothing docks to
one yet: a clamped ship on the plot moving with her carrier is a different model from a berth. Battle
riders are not carried by anyone.

### Critical hits

#### Assumption — what a "bridge station" is
The CRB's Bridge row (p. 170) disables or destroys a "random bridge station"
and never says what the stations are; HG's Fleet Battles table (p. 120) uses
the same words and is no more specific. Callisto rolls 1D on this list:

| 1D | Station | While it is out |
| --- | --- | --- |
| 1 | Comms | Cannot transmit, or send or receive a sensor hand-off |
| 2 | Sensors | No sensor actions: lock, break lock, jam comms, jam missiles |
| 3 | Computer | No acceleration, jump, sensor hand-off or software |
| 4 | Astrogation | No jump |
| 5 | Fire control | No weapons fire, point defence or sand |
| 6 | Pilot | No acceleration, evasion or assisting gunners |

A disabled station is out for the rest of the round it was hit in and all of
the next, then returns by itself; a destroyed one waits for an engineer.
Gunners are not on the list: gunnery control is dispersed through the ship
(HG p. 91), but the fire control that directs it is on the bridge.

Two rows of the CRB table are then read rather than taken literally. Severity 2
("computer reboots, all software unavailable this round and next") is the
Computer station disabled, and severity 5 ("computer destroyed") destroys that
station and takes Bandwidth to 0. The 1D×1D injury to a destroyed station's
occupant is reported but not tracked.

Repairing the Bridge undoes the damage in reverse order: each severity
recovered brings back what that level's hit took, most recent first, lost
Bandwidth included. Injuries are not undone.

#### Rule enforced — big ships shrug off small guns
HG p. 27's thresholds apply to the primary critical, the one an Effect of 6 scores: a target larger than
2,000 tons ignores criticals from turrets and barbettes; larger than 10,000 tons, from everything but
medium and large bays and spinals; larger than 100,000 tons, from everything but large bays and spinals.
A missile is a weapon of the mount that launched it. A point-defence battery, a fixed mount and a small
bay are none of the listed classes and crit a hull up to 10,000 tons. Exactly 2,000 tons is not larger
than 2,000, so a turret still crits the Sakhai and the Ulfhednar; the 3,000-ton Midu Agasham and the
5,000-ton P.F. Sloan now take criticals only from bays and spinals. "Spinal mount weapons can always
(and will!) cause critical hits", at every size. Sustained damage -- a critical for every tenth of the
hull lost -- is suffered "as normal" by every ship.

#### Interpretation — the large-ship critical tables apply to any target over 5,000 tons
HG p. 27 brings in its Cargo, Crew and Weapons tables "when two large ships square off". Callisto keys
them on the target alone, at HG p. 5's own definition of a capital ship, over 5,000 tons: a small
attacker can only crit a big ship through sustained damage or a big gun anyway, and judging "large
attacker" for a destroyer with one medium bay would be arbitrary.

#### Rule enforced — crew criticals on big ships are percentages
Over 5,000 tons the Crew row is the Crew Critical Hit Severity table (HG p. 27): 1%, 5%, 10%, 15%, 20%
or 25% of the complement killed or disabled by severity, rounded up and never nobody, taken off the
ship's `current_crew`. The life-support failures are reported as the CRB's rows report them. The DMs are
read off the crew that is left rather than off which severities have landed, as the table words them:
DM-1 "unless crew restored above 90%" of the complement, DM-2 unless above 70%, DM-4 unless above 25%,
the worst that applies. So a severity-3 hit's 10% leaves the crew at 90% and under the DM-1. The DM is
on every check the crew makes -- attacks, point defence, the pilot's assist, sensor actions, detection,
engineering, repairs -- and is named "crew" in the roll. It is not on the dodge, which is a DM on the
attacker's roll rather than a check of the pilot's.

#### Addition — the frozen watch
A design may give a `frozen_watch`, spare crew in low berths (HG p. 27). The captain revives any number
of them with a Revive order, which is the captain's action for the round in place of the leadership
check: queuing one withdraws the other. They are at their posts as the round ends, "the following turn",
up to what the complement is short, and the crew DM lifts as the thresholds are crossed. A Cargo
critical can kill them in their berths (below).

#### Rule enforced — a cargo critical on a big ship rolls the carrier's D6 table
Over 5,000 tons a Cargo critical rolls the D6 table HG p. 27 gives for a large military carrier: 1-2
fighters, 3 other small craft, 4 the frozen watch, 5-6 cargo, and the Cargo and Weapons Severity table
says how much of the row is lost -- 2D% at severity 1, D3 x 10% at 2, 1D x 10% at 3, all at 4, and all
with a Hull critical at 5 and 6. The fighter row takes that share of the fighters aboard (craft whose
design role is a Fighter), the small-craft row the same share of the other craft aboard, chosen at
random and destroyed in their berths, each with a line of its own. A row with nothing aboard of its
kind is cargo, and so is the frozen-watch row on a ship without one. Cargo itself is not tracked, so a
cargo result is a line in the log, as it is for a small ship.

#### Rule enforced — radiation wears down crews
A ship carries a cumulative DM-1 on every check its crew makes for every 10% of its hull lost to weapons
with the Radiation trait, or every 40% if the design has `radiation_shielding`; meson weapons ignore the
shielding, so their share is counted apart (HG p. 31). Only hull the ship had counts, and the DM is named
"radiation" in the roll beside the crew-loss DM. Every ship is affected: HG frames the rule as a
simplification for large crews, but writes it for all ships, and a size below which it stopped would be
an invented threshold.

#### Rule enforced — hull points follow the size bands
A design keeps its stated `hull`, but a test recomputes HG p. 10's figure (1 per 2.5 tons, per 2 from
25,000 and per 1.5 from 100,000, rounded down) under every legal mix of configuration (HG p. 11) and
reinforced or light (HG p. 12), and fails on a shipped design that matches none of them unless it is
listed as an exception with the reason. A planetoid's +25% is one of the mixes, which is how the Jaellemo's
printed 31,250 for 50,000 tons passes. The one exception is the Laboratory Ship, whose published 160
(HG p. 185) is the figure for 400 tons, not for its 360-ton dispersed hull. The loader logs the same
check for a new design, and the Add Ship tooltip says when a stated hull is not the formula's.

#### Change — a critically failed overload is a critical hit
Failing an overload by 6 or more applies a severity +1 critical hit to that
drive or power plant, with its full effect. We use the ordinary Critical Hit
Effects table for this: a first failure costs the manoeuvre drive 1 Thrust, or
the power plant 10% of its Power.

---

### Engineering and power

#### Change — a power budget, with weapons in it
High Guard prices every system's Power (pp. 16, 23, 28-33) and Callisto now
adds it up as a budget the engineer can see and manage. **Weapons are in it**,
where they used to be ignored entirely, so a ship whose plant is damaged has to
choose between flying and shooting. The jump drive draws only as the ship jumps
(HG p. 16), so it is shown on the board but kept out of the running total.

### The ship's computer

Software is bought with the ship and *runs* within the computer's Processing
score. Owning more than can run at once is normal and is the whole of the
mechanic: HMS Executor's Evade/1, Fire Control/2 and Jump Control/2 come to 30
Bandwidth on a Computer/20, so she fights or she jumps. Computers draw no Power
and take no tonnage (CRB p. 180), so they are absent from the power board, and
changing what runs is free and takes effect at once.

Every package in the two lists (CRB p. 161, HG pp. 73-76) is costed and can be
run, and each behaves as written except where noted below. Four do nothing
here — **Anti-Hijack, Battle System, Conscious Intelligence and Virtual
Crew** — because they turn on boarders, Tactics checks and replacing crew, none
of which a gunnery duel rolls.

#### Decision — a ship arrives with its jump software off
Everything else a design carries comes up running, as far as the computer
will carry it. Jump Control does not: it is the most expensive program most
ships own — on a Computer/5 it is the whole machine — and a ship that comes
up running it has no Bandwidth for the fight it is in, nor the point spare
that a sensor hand-off needs. Jumping is a deliberate act, so the astrogator
switches it on when there is somewhere to go. One tick on the computer card.

#### Change — Point Defence reaches one band, then two
The package covers another ship "within Close range", and /2 "increases this
range to Short" (High Guard p. 75). Callisto has no Close band — Adjacent and
Close both collapse into Short here, as the dogfight rules do — so the labels
cannot be used as written. We keep the step instead of the names: /1 covers a
ship at Short, /2 one band further at Medium. More generous than RAW, and the
only way the second grade means anything at this granularity — and Medium is
the right feel for an escort stationed off a freighter's flank.

#### Assumption — Virtual Gunner only stands in for an absent gunner
"On a military ship, one virtual gunner may replace the gunner crewing a
turret, barbette, bay or screen" (HG p. 76). We read that as filling an empty
seat rather than displacing someone: a mount with a gunner keeps them, and one
with nobody at it fires at the package's score instead of at nothing.

#### Change — a mount nobody is at does not fire
A gun fires because a gunner fires it. A mount with nobody at it now does
nothing, and says so in the results rather than quietly missing — unless the
computer is covering it, with a Fire Control point or a Virtual Gunner.
Point defence works the same way: it is a gunner's reaction.

So that this does not silently disarm ships, a crew nobody wrote out mans
every mount at skill 0: a ship with no stated crew is crewed by people whose
names we do not know, not a derelict. Leaving a position empty is something a
scenario says deliberately, by giving a crew whose gunnery list is shorter
than the ship has mounts.

#### Assumption — Electronics (remote ops) reads like the other engineering skills
The drones work at "an Engineer skill level of 1 or the level the Traveller has
in Electronics (remote ops), whichever is lower" (CRB p. 159), and the books
never say who aboard has that skill. Engineers carry a remote ops rating, and a
crew that says nothing about it is rated 0 at it, exactly as it is at j-drive or
power. Untrained does not arise: the skill has no use on a ship with no drones,
and a ship that has them has someone who can work them. A rating above 1 is
still held to the drones' own rating.

So a drone-run repair usually rolls at DM+0 rather than the drones' rated 1,
since no design states a rating. That is the book's cap read as written — 1 is
the ceiling, not the floor — and a crew that wants the full 1 says so.

#### Interpretation — /bis means Jump Control costs five less
"+5 to the Bandwidth of the jump control software" (CRB p. 180) could mean a
bigger computer or a cheaper program. Callisto reads it as the program costing
five less, so the saving cannot be spent on anything else.

#### House rule — ion damage spills into the computer
An ion hit deducts its damage from the target's Power. A tenth of that
damage, rounded up, comes off Bandwidth too, for as long as the Power loss
lasts — so a hit takes at least one point, which is exactly the point a sensor
hand-off needs, and a heavy hit can put a program out. A hardened (/fib)
computer takes none of it.

Not RAW. It exists because the book's own protection for a hardened computer
is an allocation of Power that a computer never draws (see below), leaving
/fib meaning nothing at all; this gives it a job, and makes ion weapons worth
firing at something other than the drives. The programs that go dark are
remembered and come back with the Power.

#### Decision — /fib computers are recorded and ignored
A hardened computer is written as immune to ion weapons and to Power loss
(HG p. 20), but computers in MgT2 draw no Power, so the clause has nothing to
attach to — a leftover from Classic Traveller, where computers did consume
energy points. The notation is costed and displayed; the protection does
nothing except against the ion house rule above.

#### Change — a screen nobody is on does not angle
Angling a screen is a gunner's reaction, so a screen with no gunner assigned
contributes nothing — unless Screen Optimiser is running, which performs the
reaction automatically at DM+0 (High Guard p. 75). Callisto previously rolled
every screen whether or not anyone was on it.

#### Assumption — Evade software stacks with a pilot's dodge
The two are different things: one is the computer jinking the ship on its own,
costing no Thrust and no crew action, the other a pilot spending Thrust. The
books do not say they combine, and do not say they do not.

#### Addition — magazines
Missiles, torpedoes and sand canisters are finite, and each launcher and
sandcaster fires from its own load. What the mounts cannot hold is in stores.
A mount holds the book's load:

| Mount | Holds | Source |
|---|---|---|
| Turret or fixed mount with missile racks | 12 missiles, however many racks | High Guard p. 28; Core Rulebook p. 168 |
| The same on a Firmpoint (a hull under 100 tons) | 4 missiles | High Guard p. 28 |
| Turret with sandcasters | 20 canisters, however many casters | High Guard p. 29; Core Rulebook p. 168 |
| The same on a Firmpoint | 4 canisters | High Guard p. 29 |
| Missile barbette | 25 missiles (five salvos) | High Guard p. 31 |
| Torpedo barbette | 3 torpedoes | High Guard p. 31 |
| Missile or torpedo bay | 12 full salvos: 144, 288 or 1,440 missiles; 36, 72 or 360 torpedoes | High Guard pp. 33–34 |

A battery holds all its mounts' loads, and a mixed turret each kind's. A salvo
throws what is in its mount, and an empty mount says so. A sandcaster with no
canisters in it cannot react, whatever is in stores. A scenario reset
restocks everything.

**Where the stock comes from.** Twenty-two designs state their stock in their
write-ups, and that is the ship's total: the mounts are filled first, in
order, and the rest goes to stores. A design that states nothing carries
twelve missiles per rack, twenty canisters per sandcaster and three torpedoes
per launcher, as the small ships that do state a load carry. Where the book
gives the mount more (a barbette's 25, a bay's twelve salvos), it carries
that instead. No ship carries less than it did.

**Book storage is beyond the bays** (user, 2026-10-29). A write-up's "Missile
Storage" is extra stores, not the bays' own load. The Dragon states 480 and
the Sloan 240 (High Guard), so their design files state their whole stock:
the Dragon 624 (one bay of 144 plus 480), the Sloan 528 (two bays plus 240).
The capitals the same way: the Hadrian's 23,040 missiles are her 50 medium
bays' 14,400 and 8,640 in storage.

**Rule enforced — the load is per turret, not per rack** (confirmed by the user, 2026-10-29). Both books say "each
turret with one or more missile racks holds 12 missiles" and "each turret
with one or more sandcasters holds 20 sand canisters". So a triple-rack
turret holds twelve, not 36, and throws three a round. A design that states
nothing keeps the 36 it always carried: twelve in the turret and two dozen in
stores.

A ship saved before mounts held loads states only its magazine. That is read
as the whole stock and filled into the mounts the same way.

#### Rule enforced, with an addition — reloading a mount
"It takes one round to reload a missile rack", and a sandcaster the same (Core
Rulebook p. 168). *Reload Turret (Gunner)*: a turret "may be reloaded by any
Traveller with the Gunner skill but it will be unable to make any attacks in
that round" (p. 172).

A gunner chooses **Reload** for a mount the way they choose Point Defence.
It refills the mount from stores, up to its load or what stores have, after
the round's fire and before movement. One Reload refills a whole battery, and
every kind in a mixed turret.

- **Spare gunners reload for free.** The crew's gunners are matched to the
  mounts in order, so gunners beyond the last mount have none of their own.
  Each Reload takes a spare while any are free. The mount keeps its own
  gunner's Fire or Point Defence, so it can fire and reload in one round.
- **With no spare left, the mount's own gunner reloads it.** Spares go to
  Reloads in the order they were chosen. The first Reload that finds none
  free, and every one after it, costs its mount the round's attack, as the
  book says. Its Fire order is kept for the next round.
- **Assumption — and its reaction too.** The busy gunner throws no sand and
  runs no point defence, since a mount gets one use a round (*Mixed turrets*).
- **Nothing reloads by itself.** A Reload is one round's work, chosen again
  each time it is wanted. Withdrawing one gives its spare back.
- **A Reload that would put nothing in the mount is not carried out.** That
  is a full mount, a mount with no launcher or sandcaster, or one whose stores
  are out of what it lacks. It takes no gunner, and the log says why.
- **Leadership** has no roll to boost in a reload.

The log has a line per reload: "Reloads mount 3 (missile triple turret)
with a spare gunner: 12 of 12 missiles (30 left in stores)". When the
mount's own gunner did it, the line also says that the mount made no attack.

An AI crew reloads by the same rules. While spares are free it reloads every
launcher and sandcaster below its load, emptiest first. With none free, a
mount's own gunner reloads only when that gives up nothing worth having:

- an empty launcher;
- a mount with nothing to fire on;
- a sandcaster that is empty when there is a fight;
- a mixed turret, only when its other gun has no target.

#### Addition — ship features
A design carries a list of *features*: whatever the hull is fitted with that is
not a drive, a sensor suite, a gun or software. One that draws Power appears on
the engineer's power board with a switch, arrives off, and sits last in the
power priority — a luxury dims before the ship slows. The Harrier class's
holographic hull, 100 Power, is the example. One that draws nothing is simply
aboard, as repair drones are. Adding another is a line of data in the design
rather than a code change.

#### Change — a system without its Power does not work
The books price every system's Power but say little about running one on less
than it needs. Callisto feeds systems in a fixed order — life support, sensors,
weapons, then the manoeuvre drive — and one that cannot have its full draw does
nothing at all: unpowered sensors find nothing, an unpowered mount cannot fire.
The drive is the exception, taking what is left and flying at the Thrust that
buys.

Most ships cannot jump with everything running, which is the books' own
expectation (HG p. 16); the engineer sheds something first and the jump panel
says how much is missing. Three designs cannot run even their own systems at
full — the Gazulin is 5 Power short, the light fighter and the Hraye scout 1
each — and fly a Thrust lower until the engineer shuts something down.

#### Assumption — Offline System uses the usual engineering target number
"A successful Engineer (power) check will allow the engineer to shut down any
number of systems" (CRB p. 171). The book gives no target number, so Callisto
uses Average (8+), as every other engineering job here does. Shutting one
system down takes the engineer's action for the round rather than "any number"
of them, since an action in Callisto belongs to one person and does one thing.
Bringing a system back takes the round but no check.

Basic ship systems cannot be shut down. They can be run at half, which HG p. 17
allows in an emergency; Callisto does not yet impose the penalties the book
lists for a ship that cannot run even half.

#### Change — an overload lasts the Effect of the check in rounds
RAW gives both overloads a single round: "will increase the ship's Thrust by +1
during the next round" (CRB p. 171). Against a Difficult (10+) check that
carries a cumulative DM-2 and a critical hit on a bad failure, one round is
very little to buy, and the engineer had no reason to take the risk.

In Callisto an overload holds for the Effect of the check in rounds, with an
Effect of 0 still buying one. A scraped success is the book's single round; a
good roll is worth the gamble.

#### Omission — quick repairs never lapse
"These quick repairs will only last for 1D hours" (CRB p. 171). In Callisto a
repair holds for good. A fight is minutes long, so the difference would almost
never come up in play, and tracking it would mean undoing a repair mid-scenario
for reasons no one at the table would see coming.

---

### Not implemented

Recorded so nobody assumes they were considered and rejected.

- **Extended sensor arrays** (+2 to be detected, and longer detail range).
- **Electronic warfare used to degrade a sensop's detection rolls**, as opposed
  to cutting communications. Jamming comms *is* implemented and does break
  hand-offs; what is missing is EW that makes a ship harder to find in the first
  place.
- **Sensor hand-off relaying.** The single-hop rule *is* enforced.

---

## Idle disconnection

Callisto runs on Cloud Run, which scales to zero when nothing is connected. A
single browser tab left open would otherwise hold the service up indefinitely,
so connections that have gone quiet for **30 minutes** are closed.

The check is all-or-nothing: connections are only dropped when *every* one of
them has been quiet that long. One person still playing keeps the whole table
alive, including the tabs of people who have wandered off — the aim is to let an
unused service shut down, not to police individual players.

Keepalives do not count as activity. The client sends one every minute whether
or not anyone is at the keyboard, so counting them would mean nothing was ever
idle. Reconnecting is just a page reload.

## Known gaps being considered for future versions

* **Capital ships**: the designs above 5,000 tons -- see the top of this document for what is already
  in place for them. Carriers launch, recover and rearm their craft; the flight-deck console, the AI's
  flight orders and docking clamps are still to come.
* **Black globe generators**; meson screens and nuclear dampers are in.
* **Ship Design Editor**: new ship designs can be created and saved by users.
* **Scenario editor**: entire scenarios can be created and saved by users.
