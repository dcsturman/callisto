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
                ├──  callisto (backend, the rules)  ──ws──  callisto-ai (the service)
 referee ──ws──┘           │                                     │
                           │ tactics.rs (verbs → actions)        │ Commander (scripted | llm | learned)
                           │ computer.rs (the solver)            │ Observation (what this ship may know)
                                                                 │ Orders (standing + this round)
                                                                 └──http──  LLM (Vertex / Anthropic)
```

**The service is Rust.** Everything it needs exists in Rust already: the
wire types, the solver, and the tactics module it links as a library. The
model call is one HTTPS request. The only Python in the plan is the offline
training in phase 4, which runs over exported logs, never inside the service.

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
   Three implementations over time: `Scripted` (rules of thumb, no model),
   `Llm` (a hosted model choosing among verbs), `Learned` (a fine-tuned or
   RL-trained policy). This is the seam that turns the three approaches in
   §5 into a sequence rather than a bet.
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
phase 4 is an LLM *over* a trained policy: the LLM turns the admiral's
English into constraints and a reward ("do not destroy her" becomes a
penalty on hull below a fifth) and the policy picks verbs under them. That
is still model over policy over solver. The solver is the floor.

## 4. The pieces

### 4.1 Backend changes (prerequisites)

Small, and each is useful on its own.

**`Ship.controller: Controller`** -- `Human` (default) | `Ai`. Settable with a
new `SetShipController` request from the referee or scenario builder, saved in
the scenario file, shown as a chip on the ship. Later, for mixed crews, this
becomes per station (`ai_stations: Vec<Role>`), but v1 is whole-ship.

**`Ship.orders: Option<String>`** -- free text, the standing orders. A new
`SetOrders{ship, orders}` request; the referee and the ship's own captain may
set it. Saved in the scenario file so authors can give pirates doctrine. Shown
on the ship's card so a human crew can read their orders too, which is a nice
thing to have regardless. Orders are world state, not a private channel, so
the referee always sees what every machine was told.

**Many ships per connection.** `SetRole` takes `ships: Vec<String>` (the
old `ship` stays as an alias for one). `SetReady` on such a connection marks
every claimed ship ready, and `Users` shows the connection with its ship
count. Only a service login may claim more than one ship; a human console
keeps one.

**Round number.** `MetaData.round: u32`, incremented by `update()`, included
in `EntityResponse`. The service uses it to know a new round began; the
frontend can show it in the header, which players have wanted anyway.

**Service login.** `Login` gets a sibling, `ServiceLogin{token}`. Two ways to
mint the token:

- *Pre-shared secret*: `--service-token` on the backend, same string in the
  service's environment. Trivial, and fine for local and canary.
- *Google service-account identity token*: Cloud Run mints an OIDC token for
  the service's account on request; the backend already validates Google JWTs
  for human logins, so verifying one more issuer is a few lines, and the
  allowed service-account emails go in the existing authorized-users file.
  This is what should run in prod.

Recommend building the token check with the issuer pluggable and shipping the
pre-shared secret first.

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
| Sensors | `lock(target)`, `jam_missiles`, `jam_comms(target)`, `go_dark`, `go_loud` | the sensor actions and `SetShipEmissions` |
| Engineer | `repair_priority([systems])`, `overload_drive`, `overload_plant`, `power(system, on)` | one engineer action each; `Repair` picks the worst crit in the priority list |
| Captain | `inspire([stations])` | `LeadershipCheck{boosts}` |

Each verb is pure: it returns the messages; the caller sends them. Each is
unit-tested against the rules in isolation, which is testing we do not have
for the equivalent human clicks today.

The Intent the Commander returns is a struct of one choice per station:

```rust
pub struct Intent {
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

`rationale` is not decoration. It is the ship reporting back up the chain:
the squadron and fleet controllers see each ship's line in the orders panel
beside the orders it was given ("Invigilator: closing to board before the
yacht can run"), the referee sees every ship's, and it is the label on every
logged decision when we come to train on them (§5).

### 4.3 Observation -- what a ship may know

A compact, stable, text-and-numbers view built once per round per AI ship:

- **Own ship**: hull, armour, crits by system, power available and spare,
  weapons and whether they can bear at each contact's range, ammunition,
  software running, crew skills, drives and jump readiness, the standing
  orders.
- **Contacts** (only ships in `contacts`, plus team-mates): name, design if
  known, range and range band, closing speed, bearing relative to our
  velocity, whether we hold a sensor lock, what we last saw it do.
- **Missiles** in flight at us or ours.
- **Planets** nearby, because the solver cares and because "use the moon for
  cover" is an order someone will give.
- **Last round**, from the structured `Effects` filtered to lines whose ship,
  target or contact is us or a contact: what we hit, what hit us, which crits
  landed.
- **The clock**: round number, rounds the orders have stood, time since last
  contact.

The same struct feeds all three Commanders. For the LLM it is rendered to
text; for a learned policy it is flattened to features; for the scripted one
it is read directly.

### 4.4 Orders and the chain of command

Orders are strings. The Commander is what gives them meaning, and the LLM is
the only Commander that can read arbitrary English, which is the strongest
reason to start there (§5).

Three levels, all stored as `orders` on ships:

- **Scenario doctrine**: written in the JSON, stands until replaced.
- **Admiral's orders**: a human with the (new) `Admiral` role sends
  `SetOrders` to any ship on their team. The frontend needs an orders panel:
  a list of the team's ships, each with its orders and its last `rationale`.
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

**Interface, not prose.** The model is given the Observation and the orders
and asked for one tool call, `plan_round`, whose schema is the `Intent` above.
It never writes an acceleration vector, a to-hit number, or a position. It
picks verbs. This is the lesson from the game-playing literature (§5.1): a
model asked for intent is reliable; a model asked for numbers is not.

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

**Determinism for tests.** A `Replay` Commander plays back recorded
decisions, and the scripted Commander is deterministic by construction, so
CI never calls a model. The integration suite drives a scenario with scripted
pirates and asserts they close and fire.

### 4.6 The scripted Commander

Not a placeholder. Rules of thumb that a referee would write on an index
card:

- Close to the best range band for our heaviest weapon; stay out of theirs
  if we out-range them.
- Fire everything at the nearest contact we hold a lock on; call the shot on
  the M-drive if the orders say cripple, the weapons if they say survive.
- Repair the worst crit on M-drive, then power, then weapons.
- Break off at a third of hull unless ordered to fight to the end.

It is the opponent in phase 1 (§6), the baseline every smarter Commander is
measured against, and the fallback when the model is down or slow.

### 4.7 The harness

A headless runner, in the backend crate's tests or a small binary: load a
scenario, assign a Commander to each team, run N rounds through
`Entities::update` directly (no WebSocket), stop on a victory condition
(all of a team destroyed or jumped out; a named ship boarded or escaped;
round limit), and record every `(Observation, Intent, outcome)`.

This exists for three reasons, in order of when they matter:

1. **Evaluation.** "Is the LLM better than the index card?" is a win rate
   over a hundred Thebus runs with the seed varied. Without it we are
   guessing. The same run with two providers on the two sides, swapped each
   game so the side advantage cancels, is how the model choice (§4.5) gets
   made.
2. **Tuning.** Prompt changes, doctrine changes and verb changes are measured
   the same way.
3. **Data.** The log is the training set that approach 2 lacks and the
   environment that approach 3 needs. See §5.

### 4.8 Joining a game

Nobody prompts the service. It is a client: it watches the round number in
`EntityResponse`, and when it ticks the service builds each team's
observation with a plain function, makes one model call per team, and sends
the messages. The backend pushing a "your move" message instead would work
too, but needs a new message and a backend that knows where the service
is; watching costs nothing and survives a backend restart on its own.

The service keeps one lobby connection and already receives the running
scenario list. When a running scenario contains ships with
`controller: Ai` that the service is not yet crewing, it opens a connection
per ship, joins, takes all roles, and sends `SetReady(false)`. When the ship
dies or the scenario ends, the connection closes. No new request, no button:
mark a ship `Ai` and it wakes up.

The service's own HTTP API is small and operational: `/healthz`, `/status`
(which ships it is crewing, last decision time, model errors), and
`/decide/{ship}` to force a re-decision, for debugging. Everything a player
does goes through the backend.

### 4.9 Where it runs

A second Cloud Run service, `callisto-ai`, deployed by a workflow alongside
`canary-be-merge.yml`, with the backend's WebSocket URL and the model
endpoint as config. Locally, a third `compose.yaml` service. The service is a
new crate in the workspace (`callisto-ai/`) that depends on the `callisto`
library for the wire types, the solver and the tactics. Rust rather than
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
- The scripted Commander gives us an opponent and a baseline before the
  model is even wired in, so the LLM is measured from its first round.
- When the harness exists, 3 becomes a weekend experiment rather than a
  programme: a policy over `Intent` trained against the scripted Commander,
  with hull-difference as reward. Whether it is worth running is a question
  we can answer with numbers then.

What we give up: 1 will sometimes do something odd, and it is not free to
run. Both are acceptable for a tool that today needs a human at every station.

## 6. Phases

Each phase ends in something playable or measurable, and merges into
`epic-ai-ships` on its own.

**Phase 0 -- prerequisites (backend only).** `controller` and `orders` on
ships and in scenario files; `SetShipController` and `SetOrders`; round
number in `MetaData`; `ServiceLogin` with the pre-shared token; many ships
per service connection. Frontend shows
the chip, the orders, and the round. Nothing decides anything yet.

**Phase 1 -- verbs, index card, harness.** `callisto::tactics` with tests;
the scripted Commander; the headless runner with victory conditions; the
service skeleton that joins, observes (with the fog-of-war test), decides
with the scripted Commander, and says ready. Playable: Thebus with pirates
who close and shoot. Measurable: scripted vs scripted win rates by scenario.

**Phase 2 -- the model.** The LLM Commander behind the provider trait; the
prompt; per-team decisions; `rationale` in the results window; `Replay` for
CI. Measurable: model vs index card. Playable: the admiral use case through
`SetOrders`, even before it has a panel.

**Phase 3 -- command.** The `Admiral` role and the orders panel; squadron
multi-select; mixed crews (`ai_stations`). Server-side fog of war (§4.1 B)
if the opponent is now good enough to be worth cheating against.

**Phase 4 -- learning.** Export the harness logs; fine-tune a small model on
the LLM's better games (filtered by outcome); or RL over `Intent` in the
harness. Decided by the numbers from phases 1-2.

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
6. **Whole-ship control in v1.** Stations come in phase 3.
7. **The verbs will be offered to humans later** ("Intercept to Medium and
   hold us there" on the pilot's console). Not in this epic, but
   `callisto::tactics` is built knowing it.

Still open, none blocking phase 0:

- **Naming.** `callisto-ai` for the crate and service; `Controller::Ai` on
  the ship; `Admiral` for the role. Open to better.
- **Victory conditions in the harness.** Destroyed / jumped out / boarded /
  round limit are the obvious four; scenarios may need their own (Thebus:
  "the yacht reaches 100 diameters").
- **What the referee can override.** Force a re-decision, replace an
  `Intent` by hand, or take a station back mid-fight. Probably all three,
  eventually.
