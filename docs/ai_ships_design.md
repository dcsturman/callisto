# AI-controlled ships

Status: **design, decisions in §7 agreed 2026-10-14.** Nothing in this document is built.
Date: 2026-10-14.
Branch: `epic-ai-ships`. Work branches off it and merges back into it; the
epic goes to `canary` as a whole when it is playable.

Scope: a new service that crews ships, and the handful of backend changes it
needs. Touches `callisto/src/payloads.rs` (new requests and ship fields),
`callisto/src/ship.rs` and the scenario format (who controls a ship, and its
orders), `callisto/src/processor.rs` (service login, round number), a new
`callisto/src/tactics.rs`, a new crate for the service, and later the
frontend (orders panel, admiral view).

---

## 1. What we want

Three ways of playing that we cannot play today:

1. **Opponent.** A referee runs the Thebus ambush and the three pirate
   corsairs fly themselves. The referee watches, adjudicates, and keeps the
   story going instead of filling in three ships' worth of stations every
   round. Or there is no referee at all and a lone player fights the machine.
2. **Admiral.** A player commands a squadron of AI ships with broad orders
   ("close with the yacht, cripple her drive, do not destroy her") and the
   ships work out the rest. The orders can change every round or stand for the
   whole fight.
3. **Mixed crew.** A human takes the stations they enjoy and the AI takes the
   rest: a human pilot and gunner with the AI running engineering and sensors
   on the same ship.

A fourth comes free once the first exists: **scripted doctrine in a scenario
file.** The Thebus lurkers carry standing orders in `Thebus.json` ("stay dark
until the yacht passes the moon, then close and board") and anyone who loads
the scenario gets pirates who behave like pirates.

Every one of these is "something other than a human sits at some stations and
sends the same messages a human would". That is the design in one sentence.

## 2. Where we are

### 2.1 What the backend already offers a client

Everything a human console does goes over one WebSocket as `RequestMsg`, and
the AI can speak the same protocol:

| Need | Already there |
|---|---|
| See the world | `EntityResponse(Entities)` after every change and every round |
| Sit at a station | `SetRole{roles, ship}` -- several roles, one ship per connection |
| Fly | `SetPlan`, `ComputePath` (the gomez solver, server side), `SetPilotActions` (dodge, assist gunners) |
| Fight | `ModifyActions(ShipActionList)` -- fire, called shot, point defence, salvo size, fire-control DM |
| Sensors | `SensorLock`, `BreakSensorLock`, `JamMissiles`, `JamComms`, emissions on/off, hand-off |
| Engineering | `Repair`, `OverloadDrive`, `OverloadPlant`, `SetPower`, software on/off |
| Command | `LeadershipCheck{boosts}`, `CaptainAction` |
| Say "done" | `SetReady(bool)`; the referee sees it in `Users` |
| Read what happened | `Effects(Vec<EffectMsg>)` -- and since #366 every line carries its ship, target and arithmetic as fields, not prose |

The last row matters more than it looks. The structured results were built for
humans, but a machine reading "`%s` fires at `%t`, missed, roll 2d6 ... vs 8"
with the ships as fields gets a faithful account of the last round for free.

### 2.2 What is missing

- **A way in for a service.** `Login{code}` is a Google OAuth code from a
  browser redirect. A service cannot complete that dance. Test mode has
  `MockAuthenticator`, but that is test mode.
- **Fog of war is enforced by the client, not the server.** `EntityResponse`
  is a deep copy of the whole world to everyone. The gunner's dropdown only
  shows `contacts` because the frontend filters. An AI that reads the message
  naively sees every ship, including the ones it has not detected.
- **No round number.** `Entities` carries no "this is round 7". A client
  cannot tell a mid-round `EntityResponse` (someone changed a plan) from the
  one that opens a new round, except by watching for the `Effects` that
  precede it.
- **No notion of who controls a ship.** Ships have a `team`; they do not have
  a controller. Nothing says "Invigilator is the machine's".
- **No orders.** There is nowhere to put "close and board" so that a ship, a
  referee or a saved scenario can read it.
- **One ship per connection.** `SetRole.ship` is an `Option<String>`. A
  service minding a team needs to claim several ships on one connection, and
  one ready flag for all of them.
- **No tactical verbs.** The backend solves "get me to this position with
  this velocity" (`ComputePath`), but not "intercept X and arrive at Medium
  range" or "keep Long range from Y". Every console works that out by hand
  today; so would an AI.

## 3. Shape of the system

```
 browsers ──ws──┐
                ├──  callisto (backend, the rules)  ──ws──  ai-ships (the service)   
 referee ──ws──┘           │                                     │
                           │ tactics.rs (verbs → actions)        │ Commander (llm | baseline | learned)
                           │ computer.rs (the solver)            │ Observation (what this ship may know)
                                                                 │ Orders (standing + this round)
                                                                 └──http──  LLM (Vertex / Anthropic)
```

**The service is Rust.** Everything it needs exists in Rust already: the
wire types, the solver, and the tactics module it links as a library. The
model call is one HTTPS request. The only Python in the plan is the offline
training in the learning phase, which runs over exported logs, never inside the service.

**The service is a client.** It joins a scenario, takes the stations it has
been given, and every round reads the world, decides, and sends the same
messages a human console sends. The backend does not know it is a machine
except for one flag on the ship. This keeps the rules in one place, keeps the
service deployable and restartable on its own, and means a scenario keeps
working when the service is down: the ships just sit there, as they do when a
human steps away.

**One connection per AI team per scenario.** Not one per ship, and the
reason is the backend's fan-out, not the socket count. Every change anyone
makes deep-copies the whole world and broadcasts it to every connection in
the scenario. With a connection per ship that is O(ships²) bytes per change:
a thousand-ship game at a kilobyte a ship sends a gigabyte per change and
there are thousands of changes a round. A connection per team receives one
copy of each broadcast however many ships it minds. It costs one backend
change: a service login may claim a set of ships (`SetRole{roles, ships}`),
and its ready flag covers all of them, so the referee's list reads
"Pirates (AI, 3 ships) -- ready".

A thousand ships would break the backend for humans too, for the same
reason, and the fix is the same seam as server-side fog of war: per-connection
projection and deltas instead of whole-world copies. That is a scaling item
of its own, outside this epic; this design must simply not make it worse,
and with per-team connections it does not.

**Three layers inside the service**, and the boundary between them is the
whole point:

1. **Observation** -- what this ship is allowed to know, in a form a decision
   can be made from. Built from `Entities` and the last round's `Effects`,
   and *only* through this layer, so fog of war has one choke point.
2. **Commander** -- a trait: `fn decide(&Observation, &Orders) -> Intent`.
   One real implementation to begin with, `Llm` (a hosted model choosing
   among verbs), two trivial ones for measuring and testing (`Baseline`,
   `Replay`, §4.6), and later `Learned` (a fine-tuned or RL-trained policy).
   This is the seam that turns the three approaches in §5 into a sequence
   rather than a bet.
3. **Tactics** -- deterministic code that turns an `Intent` ("intercept
   Lyonesse II to Medium range; fire everything at her drive; repair the
   sensors") into `SetPlan` + `ShipActionList` + the pilot and sensor
   messages. Lives in the backend crate as `callisto::tactics` so the service
   links it as a library, and so a later "Intercept" button on the human
   pilot's console can use the same code.

The Commander never emits a number the rules care about. It emits *which
verb, at whom, with what preference*. The numbers come from the solver we
already trust.

**No model implements intercept.** Intercept is a solved numeric problem and
`plot_course` solves it exactly: standoff at the band's inner edge, match
the target's velocity. "Intercept to Medium and hold us there" is that
solver called once a round. Every model in this design -- prompted,
fine-tuned or RL-trained -- sits in the Commander slot and produces the same
thing, a plan and a choice of verbs (§4.2). The one layering that is worth keeping in mind for
the learning phase is an LLM *over* a trained policy: the LLM turns the admiral's
English into constraints and a reward ("do not destroy her" becomes a
penalty on hull below a fifth) and the policy picks verbs under them. That
is still model over policy over solver. The solver is the floor.

## 4. The pieces

### 4.1 Backend changes (prerequisites)

Small, and each is useful on its own.

**`Ship.controller: Controller`** -- `Human` (default) | `Ai`. Set in the
scenario file, or with a new `SetShipController` request from a "Switch to
AI" button that anyone at that ship's console may press (and press again to
take it back). Shown as a chip on the ship. Mixed crews (`ai_stations`) are
not in this epic.

**`Ship.orders: Option<String>`** -- free text, the standing orders. A new
`SetOrders{ship, orders}` request; anyone at that ship's console, the referee,
and later an admiral for their team may set it. Saved in the scenario file so authors can give pirates doctrine. Shown
on the ship's card so a human crew can read their orders too, which is a nice
thing to have regardless. Orders are world state, not a private channel, so
the referee always sees what every machine was told.

**Many ships per connection.** `SetRole` takes `ships: Vec<String>` (the
old `ship` stays as an alias for one). `SetReady` on such a connection marks
every claimed ship ready, and `Users` shows the connection with its ship
count. This is not a service-only privilege. A human who claims every Blue
ship is Blue's commander: the referee's screen with the ship dropdown cut
down to Blue, every Blue station open to them, and the picture limited to
what Blue's ships between them have detected. That is the `Admiral` of §4.4,
and the AI service is the same kind of connection with a model where the
human would sit. One claim, one projection, one ready flag, for both.

**Rounds advance themselves when everyone is ready.** Today the referee
presses Next Round when the ready list fills, and in a game with no referee
anyone may press it. The backend now does what the referee does: in a game
with no referee, when every connection holding ships has said Ready, the
round advances. The AI is one more such connection; nobody waits on it
specially, and nobody can run the round out from under it. If the model does
not answer within its budget, the service says Ready with the ships' last
plan and the results say so. With a referee, nothing changes: they still
press. This is a backend change humans benefit from on their own.

**Round number.** `Entities.round: u32`, counting from 1, incremented by
`update()`, included in `EntityResponse`. On `Entities` beside `filename`
rather than in `MetaData`, because `MetaData` is saved into scenario files and
a saved scenario must open at round 1. The service uses it to know a new round began; the
frontend can show it in the header, which players have wanted anyway.

The results carry it too: `ResponseMsg::Effects{round, effects}` instead of
a bare list, the round stated once per batch. Today the only thing that says
which round a batch of results belongs to is that it arrived just before an
`EntityResponse`. The service reads round N's results to decide round N+1
and must not match them by arrival order; the results window gets "Round 7"
dividers and a filter; the harness logs get the round on every line.

**Service login.** `Login` gets a sibling, `ServiceLogin{token}`. Two kinds
of token, chosen by where the service runs:

- *Google service-account identity token*, on Cloud Run. The service asks
  the metadata server for a fresh one-hour OIDC token on every connection,
  and the backend verifies it against Google's public keys, which it already
  fetches for human logins. Nothing is stored anywhere, so a restart or
  redeploy of either side changes nothing and rotation is Google's problem.
  The configuration is the service account's email in the authorized-users
  file, which already lives in GCS, and `CALLISTO_SERVICE_AUDIENCE` on the
  backend: its own URL, which the token must name.
- *Pre-shared secret*, for local Docker Compose only, where there is no
  metadata server. `CALLISTO_SERVICE_TOKEN` in the backend's environment
  (never on its command line, where a process listing would show it), the
  same string in the service's. Never in an image; if it were ever wanted on Cloud
  Run it would have to come from Secret Manager, mounted on both services
  the way `google_credentials` is today.

The token check is built with the issuer pluggable; the identity token is
the one that ships to canary and prod.

**Sessions do not survive a backend restart**, and that is fine. The session
key the backend issues at login is in memory, and a redeploy forgets it; the
backend answers `PleaseLogin` and the browser logs in again. The service does
the same: `PleaseLogin` or a dropped socket means mint a token, log in,
re-claim its ships. Since the backend also forgot the claims and anything
queued for the round, the service keeps its own record of its claims and its
last `TeamDecision` per team, and after a reconnect it re-claims and re-sends
the current round's messages. It needs that state anyway for the case where
the model does not answer (§4.5). The backend runs at one instance, so there
is no second instance holding different sessions.

**Fog of war.** Two options:

- *A: the service restricts itself.* The Observation builder reads only the
  AI ship, its `contacts`, its team-mates' ships (they share a plot in the
  rules -- the hand-off links), planets, and missiles the ship can see. A
  unit test feeds it a world with an undetected ship and asserts the ship is
  absent from the observation. Cheap; honest as long as the code is.
- *B: the server filters `EntityResponse` per connection.* Right in
  principle, protects human gunners from their own DevTools too, but it is a
  change to the one message every client depends on, and the referee and
  observer still need the full picture, so it is a per-role projection rather
  than a filter.

Decided: A. The service is ours and we trust it not to cheat; one function
strips `Entities` to what the ship knows, and a test keeps it honest. The
choke point in A is exactly the seam B would need, so nothing is thrown away
if B is ever wanted.

### 4.2 `callisto::tactics` -- the verbs

A verb is a function from the world and a few parameters to concrete
messages. The first set, chosen because they cover what a human does at each
station in practice:

| Station | Verb | Turns into |
|---|---|---|
| Pilot | `intercept(target, band)` -- arrive at range band `band` matching velocity | `plot_course` with `standoff_distance` = the band's inner edge, `target_velocity` = theirs |
| Pilot | `hold_range(target, band)` -- stay in `band`, match velocity | same solver, standoff chosen inside the band each round |
| Pilot | `open_range(target)` / `disengage` -- maximum acceleration away, then jump when able | `SetPlan` straight away; `Jump` once `current_jump` allows |
| Pilot | `station(position)` -- go there and stop | `plot_course` to a point |
| Pilot | `evade(thrust)` / `assist_gunners` | `SetPilotActions` |
| Gunner | `engage(target, weapons, called: Option<ShipSystem>, salvo)` | one `FireAction` per listed mount, fire-control DM filled from the software aboard |
| Gunner | `defend(protecting)` | `PointDefenseAction` per sandcaster / PD mount |
| Sensors | `lock(target)`, `search` (an active sweep for new contacts: loud), `share_contacts(on/off)` (the hand-off link, also an emission), `jam_missiles`, `jam_comms(target)` | the sensor actions, `SetShipEmissions`, hand-off |
| Engineer | `repair_priority([systems])`, `overload_drive`, `overload_plant`, `power(system, on)` | one engineer action each; `Repair` picks the worst crit in the priority list |
| Captain | `inspire([stations])` | `LeadershipCheck{boosts}` |

Each verb is pure: it returns the messages; the caller sends them. Each is
unit-tested against the rules in isolation, which is testing we do not have
for the equivalent human clicks today.

The Intent the Commander returns is a struct of one choice per station:

```rust
pub struct Intent {
  pub posture: Posture,           // Silent | Passive | Active: a constraint over everything below
  pub helm: Helm,                 // Intercept{..} | HoldRange{..} | OpenRange{..} | Station{..} | Hold
  pub fire: Vec<Engage>,          // may be empty; may name a called shot
  pub defence: Option<Defend>,
  pub sensors: Vec<SensorVerb>,   // one per operator aboard
  pub engineering: Vec<EngineerVerb>,
  pub command: Option<Inspire>,
  pub rationale: String,          // one sentence, logged, shown to the referee
}

/// One team's answer for the round: a plan that carries over, and an
/// Intent for each ship.
pub struct TeamDecision {
  pub plan: String,               // the strategy, revised each round, fed back next round
  pub ships: Vec<(String, Intent)>,
}
```

**Who formulates the strategy.** The Commander. "Take their fighters first
and stay out of the battleships' reach" is exactly what it is for; the
verbs are the vocabulary it writes that plan in. Two ships get
`engage(fighter_1)`, every helm gets `hold_range(battleship, Long)`, the
captain gets `inspire(gunners)`. If a human admiral has given orders, the
plan serves them; if not, the Commander is the admiral. Three levels, then:

| Level | Who | Form |
|---|---|---|
| Orders | a human admiral or the scenario file; optional | English, standing until replaced |
| Strategy | the Commander, once per team per round | `plan` text plus one `Intent` per ship |
| Execution | `callisto::tactics` and the solver | the messages a console would send |

**The plan persists.** A strategy should not be reinvented every six minutes
of game time. The `plan` from this round goes into next round's prompt, so
the model revises a plan rather than starting from nothing, and the admiral
sees the current plan beside the orders they gave. This is the pattern the
Strategist work (§5.1) gets its results from: strategy in text, revised,
executed by code.

**Posture is a constraint, not a verb.** Stealth in this game is a handful of
switches -- transmitting, active sensors, the hand-off link, and firing -- over
a `Stealth` rating on the hull. "Try hard not to lose your stealth" is a
standing rule over all of them, so `Intent` states it once and tactics sets
the switches from it: `Silent` turns everything off, `Passive` listens and
shares with the team, `Active` sweeps and locks. A `Silent` posture with a
fire order is not a contradiction, it is the ambush: tactics lets the fire go
and the results say that silence was broken, because the detection pass
treats a ship that fired as having broken it.

**Boarding** is not in the rules yet. Until it is, "board" means
`intercept(target, Adjacent)` and matched velocity, and the referee narrates
the rest.

`rationale` is not decoration. It is the ship reporting back up the chain:
the squadron and fleet controllers see each ship's line in the orders panel
beside the orders it was given ("Invigilator: closing to board before the
yacht can run"), the referee sees every ship's, and it is the label on every
logged decision when we come to train on them (§5).

### 4.3 Observation -- what a ship may know

A compact, stable, text-and-numbers view built once per round per AI ship:

- **Own ship**: hull, armour, crits by system, power available and spare,
  weapons and whether they can bear at each contact's range, ammunition,
  software running, crew skills, drives and jump readiness, stealth rating
  and current emissions, whether anyone is believed to hold us as a contact,
  the standing orders.
- **Contacts** (only ships in `contacts`, plus team-mates), and **only what
  the human sensor station shows for them**: name, class and tonnage,
  current thrust against rated, stealth and countermeasures, weapons, crit
  levels, range and band, position and velocity (the 3D view shows them).
  From those, derived facts a human works out by hand: closing speed, which
  of *their* weapons reach us at this range and which of ours reach them,
  and the sensor grade their class is built with. "Stay at Very Long because
  they cannot hit you there" and "any ship with better than Basic sensors is
  the priority" are decidable from what a human sees, so they are decidable
  here. Nothing the screen does not show -- hull points, power, orders,
  what they are about to do -- is in the Observation.
- **Missiles** in flight at us or ours.
- **Planets** nearby, because the solver cares and because "use the moon for
  cover" is an order someone will give.
- **Last round**, from the structured `Effects` filtered to lines whose ship,
  target or contact is us or a contact: what we hit, what hit us, which crits
  landed.
- **The clock**: round number, rounds the orders have stood, time since last
  contact.

The same struct feeds every Commander. For the LLM it is rendered to text;
for a learned policy it is flattened to features.

**The mechanics are computed, not described.** A commander has to know the
game: which weapons reach which band, what the to-hit modifiers are, what
armour does to a light gun, how likely a silent ship is to be found. Writing
all that into a prompt would be long, would drift from the code, and would
leave the model doing the arithmetic it is worst at. Instead the Observation
carries what the rules *mean* for this fight, computed by the rules' own code:

- **Engagement tables** (built). For each of our ships against each contact,
  by range band: the best single mount's chance to hit and the hull damage
  expected from everything that bears, both ways. Damage is after armour,
  penetration, the turret bonus and the mount's multiple, worked over every
  way the dice can fall, so armour clipping a weak gun to nothing shows as a
  small number. Missiles count their whole salvo, before point defence. The
  to-hit sum is `combat::attack_terms`, the same function the real attack
  rolls against, so the odds cannot drift from the dice. An enemy's crew is
  not visible, so their side assumes gunnery 1; our side uses our gunners.
- **Exposure tables** (built). For each of our ships against each contact,
  by posture and whether we fire: the chance that contact finds us this
  round, and for a stealthy hull the chance of slipping a contact by opening
  the range a band. Range does not enter the detection roll -- only Distant
  ends contact -- so posture, burn, firing and damage are what move it. The
  sum is the detection pass's own (`entity::detection_terms`). We cannot see
  who holds us, so the table answers "if they look"; the enemy is assumed to
  be sweeping actively, since it can switch on at will. "Creep in silent and
  stay at Very Long" becomes a choice made from numbers.

What is left for the prompt is the short list of rules that are facts rather
than odds: firing breaks silence, a ship with no drive cannot run, jump needs
100 diameters and fuel, launchers ignore range.

The baseline commander shows the tables at work. It scores each band as the
share of the enemy's hull it expects to take in a round, less the share of
its own it expects to lose. The Executor against a Buccaneer deals 50 and
takes 19 at Short but 24 and nothing at Very Long; with half the Buccaneer's
hull, Very Long wins, which is what a referee would tell the player.

### 4.4 Orders and the chain of command

Orders are strings. The Commander is what gives them meaning, and the LLM is
the only Commander that can read arbitrary English, which is the strongest
reason to start there (§5).

Three levels, all stored as `orders` on ships:

- **Scenario doctrine**: written in the JSON, stands until replaced.
- **Admiral's orders**: a human with the (new) `Admiral` role, which is a
  connection that has claimed a team's ships (§4.1), sends `SetOrders` to
  any of them. The admiral can also drop into any station on any of those
  ships, as the referee can today, and sees the team's combined contacts and
  nothing else. The frontend needs an orders panel: a list of the team's
  ships, each with its orders, its current `plan` and its last `rationale`.
- **Captain's orders**: a human captain on a mixed crew can set their own
  ship's orders, which the AI stations follow.

A squadron is just several ships given the same orders at once; the panel
multi-selects. We do not need a squadron object in the rules for v1.

An order can be anything, but the system prompt (§4.5) teaches the model a
vocabulary it can rely on being understood: *engage / cripple / destroy /
board / shadow / screen X / escort X / hold here / stay dark / break off /
run for jump*. Orders in that vocabulary resolve predictably; orders outside
it resolve as well as the model manages, and the rationale tells the admiral
what it understood.

### 4.5 The LLM Commander

**Interface, not prose.** The English stops at the model. It is given the
Observation and the orders and must answer with one tool call, `plan_round`,
whose schema is `TeamDecision` above. Both providers constrain the output to
the schema, so the answer is valid JSON of the right shape or it is rejected
and asked again; nothing downstream parses a sentence. The model never writes
an acceleration vector, a to-hit number, or a position. It picks verbs with
targets and parameters. This is the lesson from the game-playing literature
(§5.1): a model asked for intent is reliable; a model asked for numbers is
not.

Worked through for one pirate in round 4 of Thebus:

*In* (the rendered observation, abridged):

```
Orders: Stay dark until the yacht passes the moon, then close and board.
Round 4. Lyonesse II: range 38,000 km (Long), closing 2,100 km/round, no
lock. HMS Executor: not in contact. Our pulse lasers reach Long; her
turret reaches Long. Hull 100%, no crits, 4 power spare.
```

*Out* (the tool call, exactly as the model emits it):

```json
{"plan": "Yacht is past the moon. Close to Short before the escort finds us; cripple her drive so she cannot run.",
 "ships": [{"ship": "Invigilator",
   "intent": {"helm": {"Intercept": {"target": "Lyonesse II", "band": "Short"}},
              "fire": [{"mount": 0, "target": "Lyonesse II", "called": "Maneuver"}],
              "sensors": [{"Lock": {"target": "Lyonesse II"}}],
              "engineering": [],
              "rationale": "Closing to board; shooting her drive so she cannot run."}}]}
```

*Then tactics does arithmetic and nothing else.* `Intercept{Lyonesse II,
Short}` becomes a `ComputePath` with the yacht's position as the end point,
her velocity as the end velocity and a standoff of 1,250 km, the inner edge
of Short; the solver returns the burn and the service sends `SetPlan`.
`Lock` becomes a `SensorLock` action. The fire entry becomes a `FireAction`
with the called-shot system set and the fire-control DM filled from the
software the ship is running. That is the code path a human console runs
when the pilot clicks a target and types a standoff. It is scripted in the
sense that a calculator is scripted.

So the split is: the model does all the *understanding* (English orders and
a picture of the fight, into verbs with targets and parameters), and code
does all the *geometry and rules* (verbs into vectors, modifiers and
messages). The verbs are the contract between them, and they are
deliberately coarse: an admiral says "close to Short and shoot her drive",
not "burn 2.3 g on bearing 047 for forty seconds". We ask the model for what
a human would say.

A harder order, from the Marduk encounter, to show the seams holding:

> While trying hard not to lose your stealth, creep into range of the
> raiders. Stay at Very Long range as they cannot hit you there. Then shoot
> them with your missiles and particle accelerator from there. If any ship
> with better than Basic sensors comes in range, taking that out is your
> priority.

| Clause | What it is | Where it lands |
|---|---|---|
| not lose your stealth | a constraint over everything | `posture: Silent` until the round we fire |
| creep into range | helm | `Intercept{raiders, VeryLong}` |
| stay at Very Long, they cannot hit you there | helm, and a fact | `HoldRange{raiders, VeryLong}`; the Observation states their reach per band |
| shoot with missiles and the PA | gunner | `engage` on those mounts; the rules digest says firing reveals us |
| better-than-Basic sensors is the priority | targeting by capability | the Observation carries each contact's sensor grade; the model picks |

Nothing here needed a new layer. It needed `posture` in the Intent and two
more facts in the Observation, which is the kind of gap a playthrough finds
and a line in a struct closes.

**Prompt shape**, cached where the provider allows:

1. *Rules digest* (fixed): the two pages of Traveller space combat that
   decide tactics -- range bands and which weapons reach which, what a crit
   does, that a ship with no M-drive is a target, that jump needs 100
   diameters and fuel. Drawn from `FAQ.md` so the model is taught our
   deviations, not the book's.
2. *Doctrine* (fixed per team or scenario): how pirates fight versus how a
   navy frigate fights. Optional.
3. *Orders* (standing + this round).
4. *Observation* (this round).
5. *The verbs* (the tool schema), with one line each on when a human would
   choose it.

**One call per team.** Per ship, three pirates decide independently and
will not coordinate. Per game, one context holds both sides' knowledge and
the model "plays" the pirates knowing exactly what the Executor is about to
do, because it just decided that too. Per team keeps the model an honest
player: its context contains only what that team's ships know, which is what
the Observation layer exists for. In the usual game (humans against AI
pirates) there is one AI team, so per team is per game anyway; the two only
differ when AI fights AI in the harness, where the separation is what makes
a win rate mean anything. The call returns one `Intent` per ship, so the
model can split fire and screen. Past a dozen or so ships one call cannot
carry every Intent; then the team splits into squadrons and a fleet call
issues orders to squadron calls, which is the chain of command (§4.4)
applied recursively. The seam allows it; this epic does not build it.

**Cost and latency.** Roughly 5-8k input tokens (mostly cached) and
300-600 output per call. One call per team per round. A round of tabletop
play takes minutes of human time; a model answering in 5-15 seconds is not
on the critical path, and the referee sees "AI ships deciding..." then
"ready". At current Sonnet-class prices this is cents per fight. Cheap enough
that cost is not a design input.

**Provider.** Behind a trait with two implementations, both reached with
the GCP project's own service-account credentials and billed to it: Gemini
on Vertex AI, and Claude on Vertex AI (Anthropic's models are a first-party
publisher in Vertex Model Garden, same auth, same billing, regional
endpoints). They differ in request format (`generateContent` versus the
Anthropic Messages API) and in how often their tool calls need correcting,
which only a week of harness runs will tell. Both cache the fixed prompt
prefix, which is what makes the rules digest cheap. Gemini is the default;
the trait makes it a config line.

**The model is chosen per team, not per service.** Each AI team names its
provider and model in the scenario file or at switch time, so Claude pirates
can fight a Gemini navy: a fair comparison in the harness, a varied opponent
in play, and a training partner that is not a copy of oneself. Within a
provider the tiers are a cost knob: the fast, cheap tier for the harness's
hundreds of runs, the strong tier for a game people are playing. The exact
versions are a week-one choice and will change under us; the trait and the
per-team field are what stay.

**Determinism for tests.** A `Replay` Commander plays back recorded
decisions, so CI never calls a model. The integration suite drives a scenario
with replayed pirates and asserts they close and fire.

**When the model is down or slow.** No scripted brain steps in. The ships
keep their last plan, the connection reports ready, and a line in the results
says the AI did not answer this round. That is what a human crew that stepped
away looks like, and the referee can see it.

### 4.6 Baseline and Replay

A scripted "index card" Commander was considered and dropped: making one
play well is real work, it would still play badly, and nobody would use it
once the model is wired in. What the harness needs from an opponent is not
that it be good but that it be *fixed*, and that is an afternoon:

- `Baseline::Hold` -- keep the current plan, fire every weapon that bears at
  the nearest contact we hold a lock on, repair the worst crit.
- `Baseline::Random` -- any legal verb per station, seeded.

Enough to tell "the model is doing something" from "the model is doing
nothing". Real improvement is measured model against model: Claude against
Gemini, or this week's prompt against last week's, sides swapped each game.

`Replay` plays back a recorded `TeamDecision` per round, for CI.

### 4.7 The harness

**Built** as `ai-ships/src/harness.rs` and the `harness` binary:

```
cd ai-ships
cargo run --release --bin harness -- --scenario ../callisto/scenarios/Thebus.json \
  --side Blue=close --side Red=close --side "ship:Lyonesse II=idle" --games 20 --rounds 40 \
  --log rounds.jsonl --outcomes outcomes.jsonl
```

A game loads the scenario into an in-process backend server, and each round
every side's commander decides from its own observation, its intents become
orders through `callisto::tactics`, the orders are applied as the processor
would apply them, and the round resolves. Dice come from the game's seed and
the round number, so a seed replays exactly and no two seeds roll alike. A
hundred and fifty rounds take well under a second. `--log` writes every
side's intents per round; `--outcomes` writes each game's result.

First results, Thebus, 20 games each (pirates on Blue, the Executor on Red):

| Pirates | Executor | Pirates won | Executor won | Draws |
|---|---|---|---|---|
| close | close | 5 | 15 | 0 |
| hold | close | 5 | 14 | 1 |
| close | random | 10 | 0 | 10 |

Two things the first runs taught. A commander with no enemy in contact has to
sweep for one, or a stealthy enemy is never found; `Close` now goes active
when it has nobody. And Marduk starts every ship at about 55,000 km, which is
Distant, where nothing can be detected: a baseline that does not know where
to look never finds anyone. Getting there is what a scenario's briefing tells
a human, so it is the model's orders, not the baseline's rules, that will
have to say it.

The design as first written follows.

A headless runner, in the backend crate's tests or a small binary: load a
scenario, assign a Commander to each team, run N rounds through
`Entities::update` directly (no WebSocket), stop when one side is gone or the
round limit is reached, and record every `(Observation, Intent, outcome)`.
Victory, for now, is which side is still alive at the end; scenario-specific
conditions ("the yacht reaches 100 diameters") come later. For training, hull
points -- remaining on our side, taken from theirs -- are the distance to
victory, dense and available every round.

This exists for three reasons, in order of when they matter:

1. **Evaluation.** "Is this prompt better than last week's?" is a win rate
   over a hundred Thebus runs with the seed varied. Without it we are
   guessing. The same run with two providers on the two sides, swapped each
   game so the side advantage cancels, is how the model choice (§4.5) gets
   made.
2. **Tuning.** Prompt changes, doctrine changes and verb changes are measured
   the same way.
3. **Data.** The log is the training set that approach 2 lacks and the
   environment that approach 3 needs. See §5.

### 4.8 Joining a game

**The backend engages the service.** When a ship becomes `controller: Ai`
-- loaded from a scenario file, or because someone at its console pressed
"Switch to AI" -- the backend calls the service: `POST /crew {scenario_id,
side, team, ships}`. A side is every AI ship on one team (id: the team's
name), or one unaligned AI ship on its own (id: `ship:` and its name), and
each call states the side's whole ship list, so a missed or repeated call is
healed by the next. The backend works this out after every request by
comparing what is true with what it last said, and calls only for sides that
changed. The service then does what any player does: opens a WebSocket
to the backend, logs in with its identity token, joins the scenario, claims
those ships, and sends `SetReady(false)`. When the ships die, the scenario
ends, or someone switches a ship back, the backend calls `POST /release
{scenario_id, side}` and the connection closes. The backend knows the
service's address from `CALLISTO_AI_SERVICE` and authenticates its calls as
a bearer token: the shared secret locally, otherwise its own identity token
from the metadata server naming the service as audience, the ordinary Cloud
Run service-to-service arrangement. Calls never hold up the game. If the service is
down the backend logs it and the ship sits there, as a ship with no crew
does.

Once crewing, nobody prompts the service round by round. It watches the round
number in `EntityResponse`, and when it ticks it builds each team's
observation with a plain function, makes one model call per team, sends the
messages, and says ready.

The service's HTTP API is therefore: `/crew` and `/release` for the backend;
`/healthz` and `/status` (what it is crewing, last decision time, model
errors) for operations; `/decide/{team}` to force a re-decision, for
debugging. Everything a player does goes through the backend.

### 4.9 Where it runs

A second Cloud Run service, `ai-ships`, deployed by a workflow alongside
`canary-be-merge.yml`, with the backend's WebSocket URL and the model
endpoint as config. Locally, a third `compose.yaml` service. The service is a
new crate, `ai-ships/`, beside `callisto/` and depending on the `callisto`
library by path for the wire types, the solver and the tactics. Not a Cargo
workspace: one at the repo root would move `callisto`'s build output out of
`callisto/target`, which its scripts and tests expect. Rust rather than
Python because everything it needs is already in Rust and the model call is
one HTTPS request.

## 5. The three approaches

### 5.1 What the literature says

The useful results are consistent:

- Pretrained models reason about strategy well in few-shot prompts without
  game-specific training, and that generalises across scenarios and even
  games -- the Stanford work on
  [strategic reasoning with language models](https://arxiv.org/pdf/2305.19165).
- The best results pair a model with search or scripted execution rather
  than asking the model to act directly:
  [Strategist](https://proceedings.iclr.cc/paper_files/paper/2025/hash/439dbffb7a044ee4b636babff57d4994-Abstract-Conference.html)
  (ICLR 2025) has the model write and revise a strategy in text while tree
  search plays it out, with no training data at all. That is our Commander /
  tactics split.
- Self-play RL on top of a small open model works and transfers to held-out
  games ([MARSHAL](https://arxiv.org/abs/2510.15414v1), 2025, trained from
  Qwen3-4B), but it needs an environment with clear rewards and many fast
  episodes. That is our harness.
- The same group's
  [self-play case study](https://arxiv.org/pdf/2403.05632) and
  [Nemobot](https://www.alphaxiv.org/abs/2604.21896) both land on
  "the model chooses, code executes" as the interface that makes game agents
  reliable.

### 5.2 Compared

| | 1. Hosted model, prompted | 2. Fine-tuned model | 3. Reinforcement learning |
|---|---|---|---|
| Training data needed | none | thousands of good rounds | none, but needs a reward and a fast environment |
| Reads English orders | yes, the whole point | yes, if trained on them | no, unless built on a language model |
| Time to a playable opponent | weeks | after 1 has produced data | months |
| Per-round cost | cents | fractions of a cent | fractions of a cent, after an expensive run |
| Ceiling | a competent referee's opponent | 1, but consistent and cheap | possibly better than us at the tactics; worse at the orders |
| Explains itself | yes (`rationale`) | yes, if trained to | no |
| Risk | prompt drift, odd choices | needs the data first | reward hacking, cost, and tactics no human would give |

### 5.3 Recommendation

**Build 1, on the Commander / tactics seam, with the harness from the start.
Treat 2 and 3 as later phases on the same seam, not as alternatives.**

The reasons:

- Two of the three ways of playing (§1) are *about orders in English*. Only
  a language model reads "cripple her but do not destroy her" on day one.
- We have no data. Approach 1 is the only one that produces a playable
  opponent before data exists, and every round it plays is a labelled
  example for 2.
- The verbs do the numerics. The part of the game an LLM is bad at -- vectors,
  to-hit arithmetic, range geometry -- is the part we already have code for.
  The part it is good at -- "they out-range us, close fast and call the shot
  on their weapons" -- is the only part we ask of it.
- The harness and its trivial baselines exist from the first round, so the
  LLM is measured from its first round, and model against model after that.
- When the harness exists, 3 becomes a weekend experiment rather than a
  programme: a policy over `Intent` trained against the LLM Commander, with
  hull-difference as reward. Whether it is worth running is a question
  we can answer with numbers then.

What we give up: 1 will sometimes do something odd, and it is not free to
run. Both are acceptable for a tool that today needs a human at every station.

### 4.10 Game records

Every game is recorded for learning later, humans and AI alike. With
`CALLISTO_GAME_LOG_DIR` set (Compose sets it to `./game_logs`), the backend
writes one JSON-lines file per game: the world at the start, every order with
the ships it was for and whether a human crew, the AI or the GM gave it, and at
each round's end the results and the world after. Nobody is named. A side's
view of a round is not stored, because the same function the AI uses rebuilds
it from the world. The AI service writes a file per side beside it: each
model call's situation, reply, tokens and estimated cost.
`cargo run --release --bin game_report -- ../game_logs` (in `ai-ships/`)
summarises what was played and what the calls cost.

Recorded today: every game on a backend with the directory set, locally.
Still to do before canary: upload to a bucket, and a converter from records
to training examples.

## 6. Phases

Each phase ends in something playable or measurable, and merges into
`epic-ai-ships` on its own.

**Phase 0 -- prerequisites (backend only).** `controller` and `orders` on
ships and in scenario files; `SetShipController` and `SetOrders`; round
number on `Entities` and on `Effects`; `ServiceLogin` with the pre-shared
token; many ships per connection. Frontend shows
the chip, the orders, and the round. Nothing decides anything yet.

**Phase 1 -- verbs, harness, model.** `callisto::tactics` with tests; the
headless runner with victory conditions and the two `Baseline` Commanders;
the service that joins, observes (with the fog-of-war test), decides with
the LLM Commander behind the provider trait, and says ready; `rationale` and
`plan` in the results window; `Replay` for CI. Playable: Thebus with real
pirates. Measurable: model against baseline, and Claude against Gemini with
sides swapped. The admiral use case works through `SetOrders` before it has
a panel.

Phase 1 also leaves the harness ready for learning, whether or not that
comes next: it runs headless through `Entities::update` with no model in the
loop at thousands of rounds a minute, dice seeded (test mode already does
this), the Observation flattened to a feature vector, the Intent space
enumerated as a discrete action set, and a reward recorded per round
(victory, plus hull differential). Built that way, learning is an experiment
on the harness rather than a rebuild of it.

The two phases after this are independent of each other. Which comes first
is decided with phase 1's numbers in hand, and the leaning as of this
writing is learning before command.

**Phase 2 -- learning.** Three shapes, from cheapest to dearest, and the
numbers choose:

- *A policy over `Intent`*, no language model: trained in the harness against
  the LLM Commander and against itself. Plays the opponent role and may play
  it better than the prompted model. Cannot read orders in English, so the
  LLM stays wherever orders come in.
- *LLM over policy*: the model turns the admiral's English into constraints
  and a reward ("do not destroy her" becomes a penalty on hull below a
  fifth); the policy plays under them. Keeps the English, keeps the
  policy's play.
- *RL fine-tuning of a small language model* on the harness (the MARSHAL
  shape, §5.1): one model that reads orders and plays well. Needs GPUs and
  the most engineering; the one to do only if the first two leave something
  on the table.

Alternatively, or as well: fine-tune on the LLM's better games from the
harness logs, filtered by outcome, for a cheaper and more consistent
Commander.

**Phase 3 -- command.** The `Admiral` role and the orders panel; squadron
multi-select; mixed crews (`ai_stations`). Server-side fog of war (§4.1 B)
if the opponent is now good enough to be worth cheating against.

## 7. Decisions and what is still open

Agreed 2026-10-14:

1. **A separate service**, in Rust, speaking the player protocol, with
   **one connection per AI team per scenario** (not per ship: the backend
   broadcasts the whole world to every connection, so per-ship connections
   are O(ships²) at scale). The model call is slow and external and does not
   belong inside the processor's single task, and "the AI went down" must
   not mean "the game went down".
2. **One model call per team** (§4.5), returning an `Intent` per ship.
3. **Fog of war is the service's job** (§4.1 A): one strip function with a
   test. Server-side projection only if ever needed.
4. **Gemini on Vertex by default**, Claude on Vertex behind the same trait;
   a week of harness runs picks (§4.5).
5. **Rationale reports up the chain**: each ship's line is shown to its
   squadron and fleet controllers and to the referee; never to the other
   side.
6. **Whole-ship control in v1.** Stations come with the command phase.
8. **No scripted Commander.** A fixed trivial baseline and a replay for CI
   are all the harness needs (§4.6); the first opponent anyone plays is the
   model.
9. **Many ships per connection is for humans too.** Claiming a team's ships
   is what makes someone its commander (§4.1, §4.4); the service is one more
   such connection, not a special case.
7. **The verbs will be offered to humans later** ("Intercept to Medium and
   hold us there" on the pilot's console). Not in this epic, but
   `callisto::tactics` is built knowing it.

Also agreed, later the same day:

10. **Rounds advance themselves** when every ship-holding connection is ready
    and there is no referee (§4.1). The AI is one more connection.
11. **The Observation is the human screen** (§4.3): what the sensor station
    shows for a contact, and nothing it does not.
12. **The backend engages the service** when a ship becomes AI-controlled,
    from a scenario file or a "Switch to AI" button at the ship's console
    (§4.8).
13. **Victory is who is alive at the end**; hull points are the training
    reward (§4.7). Scenario-specific conditions later.
14. **Anyone at a ship's console** may switch it to AI and set its orders;
    so may the scenario file (§4.1).
15. **Model per team** (§4.5), so models can play each other.
16. **Names**: the service is a sibling crate, `ai-ships`, depending on
    `callisto` by path (not a workspace, §4.9), so its Docker build takes
    the repo root as context, unlike the backend's. The ship field is
    `controller: Human | AI` on the wire and in scenario files; the Rust
    variant is `Ai` with a serde rename, because clippy's
    `upper_case_acronyms` lint fails CI on `AI`. The team-commander role is
    `Admiral`.
17. **Not now**: referee overrides of an AI's decisions, mixed crews, and
    the backend's whole-world broadcast (§3). All after there is a working
    AI.

Agreed 2026-10-19, for the first release:

18. **The AI plays with the trained network** (`docs/per_side_network.md`),
    built into the service; a model is a setting (`CALLISTO_AI_MODEL`), not
    the default. The network reads a side's goal from the scenario and
    nothing else: orders in English wait for a later step, where a model
    would turn them into settings the network follows.
19. **The new-game screen chooses who crews each side**: Human, AI, or
    Mixed, which lists the side's ships to tick. Every game offers AI
    crews, and anyone at a ship's console may still switch it to AI or take
    it back mid-game.
20. **No permission for the network.** It costs next to nothing, so
    anyone may use it. The users file's `ai` flag stays, sent to the client
    as `ai_allowed`, for features that call a model.

Still open:

- **Verbs the model is told about but cannot use yet:** point defence on
  behalf of another ship, and choosing a smaller missile salvo than the
  launcher's. The
  prompt says so for each, so the model does not try. Fire Control points
  are spent for it, spread across the round's fire orders.
- **The harness calls a game off too readily for production.** After three
  rounds with no ship holding an enemy it stops, even when one side is
  chasing the other's blip and would catch it. Cheap, and fine for prompt
  work; before results are trusted, keep playing while anyone is pursuing,
  or count only a jump as an escape.
- **The scenario builder must learn AI ships before the epic lands.** A
  scenario can already say `controller: AI` and give `orders` per ship in its
  JSON, but the builder has no way to set either: mark a ship, or a whole
  team, as AI-crewed, and write its standing orders.
- **Called shots and missile damage** now follow the book on the server (see `FAQ.md`, Weapons).

- **The verb set.** §4.2 is a first pass checked against two real orders
  (Thebus and Marduk). It needs a read against a playthrough: anything done
  every round at a station that is not a verb is something the AI cannot do.
- **Which Gemini and which Claude** to start with, and the prompt-caching
  details. A week-one choice.
