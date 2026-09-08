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
  * No mixed turrets, though you could design a ship with extra turrets to get near the same result.
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

## Known gaps being considered for future versions

* **Capital ships**: hulls above 5,000 tons, which needs spinal mounts and weapon batteries before anything
  else -- see the top of this document.
* **Defensive screens**: meson screens, nuclear dampers and black globe generators.
* **Ship Design Editor**: new ship designs can be created and saved by users.
* **Scenario editor**: entire scenarios can be created and saved by users.
