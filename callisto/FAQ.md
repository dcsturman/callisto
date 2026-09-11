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
* **Transponders are not modelled.** Nothing sensible goes into combat squawking
  one, so it would be a control every player switches off once and never touches
  again. Detection assumes it is off, and the DM+6 for it never applies.
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

## Known gaps being considered for future versions

* **Capital ships**: hulls above 5,000 tons, which needs spinal mounts and weapon batteries before anything
  else -- see the top of this document.
* **Defensive screens**: meson screens, nuclear dampers and black globe generators.
* **Ship Design Editor**: new ship designs can be created and saved by users.
* **Scenario editor**: entire scenarios can be created and saved by users.
