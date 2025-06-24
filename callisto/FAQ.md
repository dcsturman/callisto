# Callisto FAQ

This FAQ gives additional detail on the state of the Callisto project.

## Game Mechanics and differences from Mongoose Traveller (MT)

* The entire "dogfight" system is ignored in Callisto.  There's no special dogfight rules or _adjacent_ range band.
* Skills are still a work-in-progress and in some cases do not always have impact:
  * Moves are simultaneous for all ships.  Therefore there is no initiative between sides, and no impact from _Tactics_ or _Leadership_ skill. 
  * _Engineering_ skills, while supported during ship construction, have not yet been implemented in play with the exception of _engineering (jump)_.
* Missiles have a different implementation than in Mongoose Traveller.  
  * They are not launched in salvos.  Instead each ship can fire a number of missiles equal to the number of turrets it has.  So a ship with 3 turrets can fire 3 missiles per turn.  Each missile then guides towards its target, adjusting its own course each turn, and has its own chances to hit as a singleton.
  * Missiles are effective at close range.
  * Missiles have a burn limit of 10 turns and acceleration of 10G.
  * Missile launchers are currently assumed to have infinite ammo and never have to reload.
  * Missile launch is detected by all ships.
* _Boarding actions_ are outside the scope of Callisto.
* Planets currently do not support gravity.  Currently we found the movement of ships near planets was just difficult hard to get right and there'd often be collisions.  This may be addressed in a future release.
* Weapons:
  * No mixed turrets, though you could design a ship with extra turrets to get near the same result.
  * Weapons larger than large bays are not yet supported.
  * Only lasers, pulses, missiles, sand, and particle beams are supported.
  * Fixed mounts are not supported (there is no facing for ships).
* _Astrogation_ checks for jump are considered to automatically succeed.  _Engineering (Jump)_ skill is used for the check to see if the jump is successful.  


## Known gaps being considered for future versions

* **Ship Design Editor**: so new ship designs can be created by users.
* **Scenario editor**: so that entire scenarios can be created by users.