# Design: Sensors, Initial Detection and Stealth

Source: High Guard pp. 76–78 (Sensors, Initial Detection, Stealthed Ships,
Sensor Hand-offs), p. 14 (Stealth Types), p. 21 (Sensors packages).

---

## 0. What already exists

Three of the five requested items are already in the codebase. Worth knowing
before we scope the work.

| Item | Status | Where |
|---|---|---|
| 1. `Option<Stealth>` on the ship design | **Done** | `ship.rs:351` `pub stealth: Option<Stealth>`; enum at `ship.rs:1052` |
| 5. Show stealth level on the ship display | **Done** | `Controls.tsx:333-338` — a Stealth row appears whenever the design has stealth or countermeasures |
| Stealth DM applied to sensor lock | **Done** | `entity.rs:1033 sensor_stealth_modifiers`, `rules_tables.rs:167 stealth_mod` |
| Sensor quality + crew skill DM | **Done** | `entity.rs:1047 sensor_quality_modifiers`, `SENSOR_QUALITY_MOD` |
| 2. Active sensors on/off | **New** | — |
| 3. Visibility gating in the UI | **New** | — |
| 4. Escaping range bands | **New** | — |
| Initial detection as a concept | **New** | — |

The existing DM values already match RAW exactly:
`SENSOR_QUALITY_MOD = [-4,-2,0,1,2]` for Basic→Advanced, and
`STEALTH_MOD = [-2,-2,-4,-6]` for Basic/Improved/Enhanced/Advanced.

So the answer to "should we combine the two enums" (your item 1 alternative):
**no — keep `Option<Stealth>` as is.** It is already correct, already
serialized, already round-trips through 79 design files, and `None` already
means "no stealth". Merging `Sensors` and `Stealth` into one enum would
conflate two orthogonal axes (what I can see with vs. how hard I am to see)
that RAW keeps on separate tables with separate DMs.

### One pre-existing bug this work should fix

`sensor_stealth_modifiers` (`entity.rs:1037-1043`) computes
`(stealth_mod + delta_tl).min(0)` and returns `0` for any target without
stealth. That conflates two distinct RAW rules:

- **Stealth Types (p. 14):** "an additional DM-1 for every Tech Level the ship
  is higher than the sensors trying to locate it" — negative only, stealth only.
- **Initial Detection / Stealthed Ships tables (pp. 76–77):** "TL difference
  between ships: **+1 per higher TL** — A TL15 ship receives DM+3 to detect a
  TL12 ship" — a *positive* bonus for the better-teched detector, and it applies
  whether or not the target has stealth.

Today the `.min(0)` clamp discards the legitimate positive TL bonus, and the
`is_some()` guard means a high-TL ship gets no advantage at all detecting a
non-stealthed low-TL ship. These should be two separate terms.

---

## 1. The rules we are implementing

### Initial Detection (table on p. 76, rules text p. 77) — Average (8+) Electronics (sensors)

| Factor | DM | Computable today? |
|---|---|---|
| TL difference between ships | +1 per higher TL | Yes — `design.tl` both sides |
| Target running active sensors | +2 | **Needs new flag** |
| Target running passive sensors only | +0 | Same flag |
| Target operating manoeuvre drive | +1 per Thrust | Yes — from the ship's plan |
| Target operating power plant | +1 | Yes — effectively always true |
| Transponder or radio comms | +6 | **Not modelled — see §3** |
| Extended sensor array deployed | +2 | Not modelled — out of scope |
| Stealth | −2 / −4 / −6 | Yes — `stealth_mod` |

Plus the detector's own sensor-package DM (p. 21) and sensop skill, which
`sensor_quality_modifiers` already supplies.

RAW note that matters, from the tail of the Initial Detection discussion where it
runs over onto p. 77: *"attempting to locate a ship with this level of accuracy
requires the use of active sensors."* This is the hinge that makes your item 2
mechanically meaningful rather than cosmetic.

### Stealthed Ships — reacquisition (p. 77)

Triggered when *"sensor contact with ships that have stealth may be lost if the
range between ships extends by one or more bands during an encounter."*

| Factor | DM | Computable today? |
|---|---|---|
| TL difference between ships | +1 per higher TL | Yes |
| Stealthed target fires weapons | +2 | Yes — fire actions this round |
| Stealthed target damaged, emits heat | +1 per Severity | **Needs a severity accumulator** |
| Stealthed target uses active sensors | +2 | Needs the new flag |
| Stealthed target operating manoeuvre drive | +1 per Thrust | Yes |
| Stealthed target uses transponder or comms | +6 | **Not modelled — see §3** |

The stealth coating DM itself also applies here — the Stealth Types text says it
applies to checks "to detect **or lock onto**" the ship, so it is not limited to
initial detection.

### Stealth Types (p. 14)

| Type | TL | Sensors DM | Tonnage |
|---|---|---|---|
| Basic | 8 | −2 | 2% of hull |
| Improved | 10 | −2 | — |
| Enhanced | 12 | −4 | — |
| Advanced | 14 | −6 | — |

Already encoded correctly.

---

## 2. Core model: what "seeing" means

Callisto currently has exactly one visibility-ish concept: `sensor_locks:
Vec<String>` on `Ship` — a *targeting* lock worth DM+2 to hit
(`combat.rs:175`). RAW treats detection and lock as different things, and we
need both.

**Proposal: add a second, weaker relation.**

```rust
// ship.rs, alongside sensor_locks
#[derivative(PartialEq = "ignore")]
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub contacts: Vec<String>,   // ships THIS ship currently detects
```

Properties:

- **Directional.** A detects B does not imply B detects A. This is the whole
  point of stealth and falls out naturally from per-ship storage.
- **Sticky.** RAW: *"After initial contact, sensor detection is maintained under
  most circumstances."* Once in `contacts`, it stays until something removes it.
- **Mirrors `sensor_locks`** in shape, serialization, and `PartialEq` handling,
  so scenario round-tripping and the existing FE plumbing extend naturally.

**Invariant: a sensor lock requires a contact.** `sensor_lock()` gains a
precondition, and dropping a contact drops any lock on that target. Today
`sensor_lock` has no detection prerequisite at all.

### Relationship between the three states

```
  undetected  ──initial detection check──▶  contact  ──SensorLock action──▶  locked
       ▲                                       │                               │
       └────── failed reacquisition ◀──────────┴───────────────────────────────┘
              (stealth + range band opened)         (lock drops with contact)
```

---

## 3. Going dark: active sensors

One new boolean on `Ship`, defaulting to **on**:

```rust
#[serde(default = "default_true", skip_serializing_if = "is_true")]
pub active_sensors: bool,
```

`skip_serializing_if` keeps existing scenario JSON byte-identical, the same
discipline the weapon refactor used for the 79 design files.

### What each costs and buys

| | Active sensors ON | Active sensors OFF |
|---|---|---|
| Enemy detecting you | gives them **DM+2** | DM+0 |
| You acquiring new contacts | allowed | **forbidden** (RAW: pinpointing requires active sensors) |
| You maintaining existing contacts | yes | **yes** — sticky contacts survive going dark |
| You using SensorLock | allowed | **forbidden** |
| Your existing sensor locks | kept | **dropped** (see open question D) |

### Transponders are deliberately not modelled

The transponder carries the single biggest DM on either table at +6, and it was
built in the first cut of this phase. It has since been removed.

Nothing sensible goes into combat squawking its transponder, so the control
would be one every player switches off once and never touches again — a
permanent checkbox for a decision nobody actually makes. Detection therefore
assumes it is off and the +6 never applies.

The cost of leaving it out is that an ordinary ship is detected on DM+3 rather
than DM+9: **83% in the first round** instead of 100%. Since acquisition is
retried every round, a pair that misses is almost certain to have found each
other by the second or third, so ordinary engagements still open with mutual
awareness. Contrast the stealthed cases in §8, which stay in the single digits.

Should this ever be wanted back — a customs or piracy scenario where squawking
matters — it is a field, a wire field and a checkbox, and the DM row is already
written down above.

This gives the stealth captain the RAW playbook: *"Savvy stealth ship captains
know how to 'go dark' after distancing themselves from an opponent, shutting
down most or all systems that allow them to be detected."*

### Where the toggles live

**Add Ship** (`AddShip.tsx`) gets a checkbox, defaulting to on, so a scenario
can be built with a ship already running dark.

In play it is not a `ShipAction`. This is persistent ship state, not a
once-per-round action, and the sensor action slot is already contended
(SensorLock / BreakSensorLock / JamComms / JamMissiles). A checkbox in the
sensor panel, sent as a new `RequestMsg::SetShipEmissions { ship_name,
active_sensors }`.

---

## 4. When detection is evaluated

Current round order in `player.rs:698-732`:

```
1. leadership effects
2. sensor_actions   (SensorLock, BreakSensorLock, JamComms)   ← pre-fire
3. fire_actions
4. JamMissiles
5. update_all       ← ships MOVE here (movement is after combat)
6. engineer actions (jump)
```

**Add `detection_pass` as the last step of the round** (see question F).

It runs last because the reacquisition trigger is defined by movement, and
movement happens in step 5. Critically, `player.rs:698` already builds
`ship_snapshot: HashMap<String, Ship>` — a deep copy of every ship taken
*before* the round resolves. That gives us start-of-round positions for free,
which is exactly what the range-band comparison needs.

```
for each observer O:
  if !O.active_sensors: skip acquisition (maintenance only)
  for each other ship T:
    band_start = find_range_band(dist(snapshot[O], snapshot[T]))
    band_end   = find_range_band(dist(O, T))

    if T in O.contacts:
        # maintenance
        if band_end > band_start and T has stealth:
            roll Stealthed Ships check; on failure drop contact (+ any lock)
        if band_end == Distant+ and beyond detection envelope:
            drop contact
    else:
        # acquisition — requires active sensors
        if O.active_sensors and band_end <= Distant:
            roll Initial Detection check; on success add contact
```

`find_range_band` (`combat.rs:824`) is already a pure function of distance, and
`RANGE_BANDS = [1_250_000, 10_000_000, 25_000_000, 50_000_000]` metres puts
Distant at 50,000 km — matching the RAW text that beyond 50,000 km objects are
just blips. So "escaping" is not tricky to detect at all: it is a comparison of
two integers derived from positions we already have.

**This is the answer to your item 4.** The part that felt tricky is solved by
the snapshot that already exists for a different reason.

---

## 5. UI and UX

You asked for server-sends-everything, client-dims. Agreed, and worth stating
the consequence explicitly: **this is not fog of war against a hostile player.**
Anyone reading the raw WebSocket traffic sees every ship. For a referee tool
with players around a table that is the right trade — it keeps the GM view
trivial and avoids per-player entity filtering — but we should not later claim
it as a security property.

### Three visual states in the 3D view

| State | Ship body | Label |
|---|---|---|
| Your own ship (single-ship view) | full brightness `[10,10,24]` | green `#3dfc32` |
| Other ship, detected | dimmed | grey |
| Other ship, **not** detected | dimmed further / outline only | dark grey |

In the "all ships" (GM) view every ship renders at full brightness — you see
everything, per your spec.

Files: `Ships.tsx:117` (label colour) and `:95-99` (body material).

### Gating the selectors

`EntitySelector` (`lib/EntitySelector.tsx`) is a single shared component used by
**all three** places that pick a ship:

- `WeaponUse.tsx:632` — firing menu
- `ShipComputer.tsx:337` — navigation computer
- `Controls.tsx:64` — ship picker

So one change to `EntitySelector` covers your item 3 everywhere. Add an optional
`undetectable?: (e: Entity) => boolean` prop; matching options render
`disabled` with a grey style and a "(no contact)" suffix.

**The gating rule is relative to `computerShip`, not to the logged-in player.**
This unifies both views: in single-ship view `computerShip` is your ship; in GM
view it is whichever ship you are currently commanding. Either way the question
"can this ship shoot that one" has the same answer. `computerShip` already
exists in both contexts.

### Sensor panel additions

`ShipComputer.tsx:535-566` already renders the sensor action dropdown and a
"Locks: …" line. Add:

- A checkbox: **Active sensors**
- A "Contacts: …" line beside the existing "Locks: …" line
- Filter the `Sensor Lock: X` options to ships in `contacts` (you cannot lock
  what you have not detected)

### The general rule: no contact, no interaction

A contact is not merely a targeting prerequisite — **an undetected ship is not
there as far as you are concerned.** Every action that names another ship
requires a contact on it:

| Action | Requires contact |
|---|---|
| `FireAction` (incl. called shots) | yes |
| `SensorLock` | yes |
| `JamComms` | yes |
| `BreakSensorLock` | implied — you cannot hold a lock without a contact |
| Nav computer target | yes |
| `BoostTarget::Sensor { ship }` | via the action it boosts |

Point defence is the one deliberate exception (§7.3): it engages *missiles*,
which are physical objects arriving at you, not the ship that launched them.

### The roster box leaks everything today

`ShipSummary.tsx` ("Ships", upper right) currently prints name, hull
`current(max)` and thrust in G **for every ship in the scenario**. Thrust in G
is precisely the manoeuvre-drive detection DM, so the box gives away the exact
signal the sensor game is about.

Change: a row for a ship you have no contact on shows **the greyed name only**,
with hull and thrust redacted to `—`. Presence stays visible (consistent with
dimming rather than hiding in the 3D view); details do not. In the GM
"all ships" view everything renders fully, as everywhere else.

### Server-side enforcement

The FE greys out illegal targets, but the server should also reject a
`FireAction` at a ship not in the attacker's `contacts`, and reject `SensorLock`
on a non-contact. Cheap, and it keeps a hand-crafted WS frame from bypassing the
rule.

---

## 6. Wire protocol changes

| Direction | Change |
|---|---|
| → client | `Ship.contacts: string[]` (omitted when empty) |
| → client | `Ship.active_sensors: bool` (omitted when true) |
| ← client | `RequestMsg::SetShipEmissions { ship_name, active_sensors }` |
| → client | `EffectMsg::SensorContact { observer, target, acquired: bool }` |

`EffectMsg` (`payloads.rs`) currently has ShipImpact, ExhaustedMissile,
ShipDestroyed, BeamHit, Message, EngineerAction, LeadershipAction. A new
`SensorContact` variant lets the log say "Marduk lost contact with Threshing
Oar" rather than burying it in a generic `Message`.

All additive; existing clients keep parsing.

---

## 7. Things not on your list

You said you were sure you were missing things. These are the ones I found.

1. **Missiles already in flight when contact is lost.** Proposal: they keep
   flying. Callisto already treats all missiles as smart
   (`entity.rs:837` "For now assume all missiles are smart missiles"), so they
   have their own seekers and should not care about the launcher's contact
   state. Needs an explicit ruling, though, or it will come up mid-game.

2. **Detection must gate sensor lock** — covered above, but it is a behaviour
   change to existing code, not just an addition.

3. **Point defence vs. undetected attackers.** PD intercepts *missiles*, which
   are physical objects arriving at you; it should keep working regardless of
   whether you have contact on the launching ship. No change, but worth a test
   so nobody "fixes" it later.

4. **Sensor damage already degrades detection for free.** `combat.rs:498-509`
   already downgrades `current_sensors` on a critical, and detection reads
   `current_sensors`. So shooting out a ship's sensors will now also blind it.
   That is a nice emergent result and needs a test to lock it in.

5. **The "+1 per Severity" heat DM has no backing data.** Criticals are applied
   immediately (`do_critical`, `combat.rs:422`) but no cumulative severity is
   stored on the ship. Implementing that row of the table means adding an
   accumulator. See open question C.

6. **New ships added mid-scenario** (GM uses Add Ship). They need a defined
   initial contact state — proposal: they enter undetected and get resolved by
   the next detection pass, except under the compatibility default in
   question A.

7. **Planets are always visible.** Detection applies to ships only. No change,
   but the `EntitySelector` gating must not disable planets in the nav computer.

8. **Sensor hand-offs (p. 77) — deferred, blocked on TEAMS.** Squadrons sharing
   contacts over comms, costing 1 Bandwidth from host and recipient. This needs
   a concept of *sides* that Callisto does not yet have: there is no way to say
   which ships would share with each other. Add teams first, then hand-offs.

9. **`JamComms` interaction — deferred, deliberately.** There is no RAW for
   jamming affecting sensor detection, and `JamComms` is a *comms* action, not a
   sensors one. Revisit later rather than inventing a rule.

---

## 7b. Detecting missile launch (found; deferred)

Not in High Guard — it is in the Core Rulebook under **Detecting Missile
Launch**, and it interlocks with this design more tightly than expected:

> When a ship launches missiles, sensor operators on board other ships may make
> an immediate **Routine (6+)** Electronics (sensors) check in order to detect
> them. **If the firing ship has not been detected itself, this becomes an
> Average (8+) check.** DM+1 is applied for every full 10 missiles in the salvo,
> up to a maximum of DM+6. Undetected missiles may be picked up by the sensor
> operator at the start of every combat round with an Average (8+) check.

| Rule | Value |
|---|---|
| Detect an incoming salvo on launch | Routine (6+) |
| …if the *firing ship* is not a contact | Average (8+) |
| Salvo size | DM+1 per full 10 missiles, max DM+6 |
| Re-check for missed salvos | Average (8+) at the start of each round |

Two things worth noting now, even though we are deferring:

1. **It reuses the ship-contact state we are building.** "If the firing ship has
   not been detected itself" is a direct read of `contacts`. Shooting from
   stealth genuinely makes your missiles harder to see — a second-order payoff
   that falls out of the model for free.
2. **It implies a missile-contact relation** parallel to ship contacts
   (which observer knows about which salvo), plus salvo grouping. Callisto
   currently tracks individual missiles, not salvos, so this is not a small
   addition. It also gates the CRB Electronic Warfare countermeasure, which
   is only meaningful against missiles you have detected.

Deferred; revisit after the ship-level model is in.

---

## 8. Decisions (resolved)

**A. Contact state — RESOLVED: no special-casing, no compatibility flag.**
Every round, before any player action, each ship with **active sensors on**
attempts to acquire a contact on every ship it does not already have one for.
Ships with active sensors off acquire nothing.

The DM maths makes this safe for all 8 existing scenarios without an opt-in
flag, which is why the earlier A1/A3 proposal is dropped:

| Situation | DM | Detected |
|---|---|---|
| Ordinary ship (plant +1, active sensors +2), Military sensors | +3 | **83%** |
| Same, detector has Basic sensors (−4) | −1 | 28% |
| Stealth Basic/Improved, gone dark, coasting | −1 | 28% |
| Stealth Enhanced, gone dark, coasting | −3 | 8% |
| Stealth Advanced, gone dark, coasting | −5 | 0% |
| Stealth Enhanced, gone dark but thrusting 3G | +0 | 42% |
| Stealth Advanced, gone dark but thrusting 3G | −2 | 17% |

Ordinary ships are found quickly rather than automatically, and since
acquisition is retried every round a pair that misses will almost certainly
have found each other by the second or third. Stealth works because going dark
removes the +2 and the coating subtracts up to another 6. And the manoeuvre-drive
DM creates the central tension: running away is exactly what makes you visible.

**B. Thrust DM — RESOLVED:** `floor(|acceleration| / G)` using the existing
`entity.rs:37 G = 9.807`. Ships store acceleration in m/s² in
`AccelPair(Vec3, u64)`.
*Sub-case:* a `FlightPlan` may carry two segments with separate durations. For
the single-segment case (the overwhelming majority) this is exactly as
specified. For two segments, proposal: **duration-weighted mean magnitude over
the round**, taken from the start-of-round plan in `ship_snapshot`. Alternative
is max-magnitude ("the loudest moment is what the sensop notices").

**C. Heat DM — RESOLVED:** sum `crit_level` across all systems on the target.
`Ship` already carries `crit_level: [u8; 11]` (`ship.rs:261`), so this is
`ship.crit_level.iter().map(i16::from).sum()` with no new state. Cumulative for
the encounter, cleared by `crit_level = [0; 11]` on repair/reset
(`ship.rs:1138`).

**D. Going dark — RESOLVED:** drops **locks**, keeps **contacts**. Matches RAW
"sensor detection is maintained under most circumstances" while making the
choice cost something real. House rule; goes in `FAQ.md`.

**E. Beyond Distant — RESOLVED:** no acquisition past Distant (50,000 km), and
existing contacts drop. This is what makes running away actually work.

---

## 8b. Decisions (resolved, round two)

**F. RESOLVED — the pass runs at the end of the round**, plus an initial pass at
scenario load/reset and whenever a ship is added mid-game. Positionally
identical to "first thing each round" (nothing moves in between) but the player
sees new contacts before queueing, so there is no round of lag. It also has to
be here for the range-band comparison, which needs start- and end-of-round
positions.

**One pass, at most one roll per ordered pair per round:**

```
for observer O, target T:
    if T in O.contacts:
        if band(O,T) > Distant:                  drop contact
        elif band_end > band_start and T stealthed:
                                                 Stealthed Ships check
                                                 on failure: drop contact + lock
    else if O.active_sensors and band <= Distant:
                                                 Initial Detection check
                                                 on success: add contact
```

A pair that failed reacquisition does not also get an acquisition roll that
round.

**G. RESOLVED — missiles in flight are unaffected by contact loss.** Every
missile is smart (`entity.rs:837`) and guides itself. Revisit if dumb missiles
are ever added; noted in `FAQ.md` so the assumption is written down rather than
implied.

**H. RESOLVED — two-segment flight plans use max magnitude.** Take
`max(|seg0|, |seg1|)` from the start-of-round plan in `ship_snapshot`, then
`floor(mag / G)`. The loudest burn in the round is what the sensop notices.

---

## 8c. Refinements made during implementation

Two things the design did not pin down, settled while building phase 4.

**The opening contact state is seeded, not rolled.** Decision F called for "an
initial pass at scenario load", but `Server::new` has no `test_mode` and so no
seeded RNG, and rolling dice inside a scenario load would make the opening of
every scenario non-reproducible. Instead the load seeds deterministically:
**every ship gets a contact on every non-stealthed ship.** Ordinary hulls would
be found on DM+3 within a round or two anyway, and opening a fight with a
coin-flip over whether the two sides can see each other is worse than opening it
resolved. A stealthed hull is the case actually worth playing out, so it starts
undetected and has to be acquired by the pass.

**Seeding checks range.** Contacts are only seeded within Distant, so a scenario
that opens with ships more than 50,000 km apart opens with them unaware of each
other, and they acquire normally once they close.

An earlier cut seeded regardless of range and let the first detection pass drop
the far pairs, on the theory that a range-aware seed would leave such ships
permanently unengageable. That was wrong twice over: it is not permanent — they
acquire as soon as they are inside Distant — and handing out a contact only to
retract it a round later is worse than never granting it. The one test that
relied on firing at a target 10 million km away (missile burn-out) now launches
at a target inside Distant and moves it out of reach afterwards, which
incidentally covers decision G: missiles in flight are unaffected by their
launcher losing contact.

---

## 9. Implementation phases

Each phase is independently shippable and testable, in canary→main order.

| Phase | Content | Risk |
|---|---|---|
| **1** | ✅ Split the TL bonus from the stealth TL penalty; fix the `.min(0)` clamp bug. Pure rules fix with tests. | Low, but it *does* change existing to-hit maths |
| **2** | ✅ `contacts` on `Ship`; wire serialization; no-contact-no-interaction invariant across all ship-targeting actions; server-side enforcement. No UI yet. | Medium — touches sensor_lock, fire, JamComms |
| **3** | ✅ `active_sensors` flag, `SetShipEmissions` request, sensor-panel and Add Ship checkboxes. | Low, additive |
| **4** | ✅ `detection_pass` as the last round step: acquisition + reacquisition + range-band escape. | Highest — the real new mechanic |
| **5** | ✅ FE visibility: `EntitySelector` gating, 3D dimming, contacts readout, `ShipSummary` redaction. | Low, additive |
| **6** | ✅ `FAQ.md` entries for every house rule chosen above. | Low |

Phase 1 is worth doing first and alone, because it changes numbers in existing
combat and should not be tangled up with the new mechanic when we are reading
test diffs.

---

## 10. Test plan

- One unit test per row of both DM tables, asserting the modifier in isolation.
- Round-trip test that a scenario with no sensor fields serializes byte-identical
  (the `shipped_designs_round_trip_unchanged` pattern).
- Range-band escape: two ships at Short, one with stealth, thrust apart to
  Medium, assert a reacquisition roll happens and that failure drops the contact.
- Range-band *closing*: assert **no** check is made.
- Non-stealth ship opening range: assert no check and contact retained.
- Going dark: assert no new acquisitions, existing contacts retained, locks
  dropped (per D).
- Sensor critical → degraded `current_sensors` → measurably worse detection.
- PD still intercepts missiles from an undetected launcher.
- Fire action at a non-contact is rejected server-side.
- `SensorLock` and `JamComms` at a non-contact are rejected server-side.
- `ShipSummary` redacts hull and thrust for non-contacts but keeps the name.
