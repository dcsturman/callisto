# Replacing the LLM commander with a learned one: the plan

Status: plan, October 2026. Nothing here is built yet.

## 1. What changes

The LLM commander reads a prompt of rules and standing orders and answers with
verbs. It works, it is promptable, and it costs real money every round. The
learned commander keeps the same verbs and the same view of the board, but it
chooses with a small neural network trained by reinforcement learning (RL)
against one fixed objective: destroy the other side, or make them run.

What stays:

- **The verbs.** The network's actions are the tactics verbs (`intercept`,
  `hold_range`, fire, lock, posture, dodge reserve...), turned into engine
  requests by `callisto::tactics::orders` exactly as the LLM's are. The
  verbs are the action space; the engine's own course plotting stays
  underneath them.
- **The view.** The side sees what `observation::observe` gives it: its own
  ships, contacts, blips, odds, exposure. Fog of war is preserved.
- **The harness and the records.** Games are played by `harness::play`;
  outcomes and per-round logs already exist. The LLM commanders and the
  scripted baselines become opponents to measure against.
- **The AI service.** A trained policy is one more `Commander`
  implementation, so Fleet Comms, team orders and the AI gate carry over.

What goes: the prompt, the standing orders, the per-round model call and its
cost. A trained policy answers in under a millisecond, for free.

## 2. The objective, as a reward

RL needs a number per game and, in practice, a little signal per round.

**End of game.** From the trained side's point of view, highest first:

| Outcome | Reward |
|---|---|
| Every enemy ship destroyed | +1.0 |
| Enemy fled: every enemy ship jumped out or out of contact for 3 rounds | +0.6 |
| Draw: round limit with both sides still in it, or both running at once | -1.0, as bad as being destroyed |
| Our side fled | -0.6 |
| Our side destroyed | -1.0 |

Fleeing is worth more than dying, for either side, which is what the
objective asks for.

A draw was first scored on hull alone, up to ±0.3. The first self-play run
showed why that fails: playing copies of itself, the network learned that
keeping its distance after a little long-range damage scored as well as a
risky fight, and drew 40% of its games against the baselines, winning almost
none in Tessaly or Jupiter. A draw then cost both sides 0.2. The second run
showed that was not enough: in an uneven scenario the weaker side, playing a
competent copy of itself, learned to decline battle (a draw at -0.2 beats a
loss at -1), and carried that into games it used to win against `close`. A
draw now counts as a loss for both sides, -1, so a side's only ways out of a
fight are winning it or leaving it (-0.6, which the objective prefers to
dying). This may be refined later; for now it makes them fight. The Elo
ladder leaves draws out of its fit and reports them on their own.
That run also started from scratch; self-play now starts from the network
trained against `close`, which already attacks, and plays half its games
against the baselines at first, tapering to 15% over ten million steps.

**What "fled" means.** Judged by the engine on true positions, not on either
side's fogged view. A side has fled when every one of its surviving ships
has either jumped out, or has been more than 50,000 km (beyond Distant) from
every enemy ship *and* moving away from her nearest enemy, without burning
back toward her, for 3 rounds running, while the other side still has a ship able to fight. If both sides
are running, nobody fled: the game is a draw, scored on hull. Both
conditions matter: a 6 G fighter can pass 50,000 km in a few rounds and come
straight back.

Two refinements found in building it. "Opening the range" cannot be the
test, because range opens for both sides alike: whenever one side ran, the
other was opening too, and every escape scored as a draw. So who is running
is judged by each ship's own burn and motion in the scenario's frame: moving
away from her nearest enemy and not burning toward her. Both, because each
alone misleads. On velocity alone, a fighter that overshoots and brakes to
come round would count as running. On thrust alone, a ship that burns clear
and then coasts away would not, and would take a draw instead of the
penalty for fleeing. A chaser falling behind is heading in either way. And a
side can only flee once it has been inside 50,000 km of an enemy: Marduk and
Thebus-like openings start far apart, and a side that never closed has not
run, it has not arrived.

The incentives then line up in self-play. The chaser prefers destroying
(+1.0) to driving off (+0.6), so it pursues. The loser prefers fleeing
(-0.6) to dying (-1.0). A policy that ran at round 1 would score -0.6
against one that wins by fighting, so that is not learned.

**Goals.** One objective for every side turned out to be too blunt: a side
meant to hold had no way to win by holding, and against a strong defensive
position the best score was to run. So each side now has a goal from a small
menu, set per ship in the scenario beside its orders (a side's goal is the
one its ships share), given to the network as an input, and scored by its
own table:

| Outcome | Destroy | Hold | Raid | Escape |
|---|---|---|---|---|
| Enemy destroyed | +1 | +1 | +1 | +1 |
| Enemy fled | +0.6 | +1 | +0.6 if 25% dealt, else -0.6 | share jumped |
| Draw | -1 | +0.6 if still holding, else -1 | +0.6 if 25% dealt, else -1 | share jumped |
| We fled | -0.9 | -1 | +0.6 if 25% dealt, else -0.6 | share jumped |
| ...with more than half our hull | -1.1 | -1.1 | +0.6 if 25% dealt, else -1.1 | share jumped |
| We were destroyed | -1 | -1 | -1 | share jumped |

Holding means a ship still within 25,000 km (Long) of where the side
started. A raid must destroy a quarter of the enemy's hull however the game
ends. Escape means jumping out of the system and nothing else; "share
jumped" runs from -1 (no ship jumped) to +1 (all did). Running with more than
half the hull left, when leaving is not the goal, is cowardice and scores
worse than being destroyed. A quarter of training games draw each side's
goal at random, so the network learns what the goals mean rather than which
one a scenario has. Ships can now jump: a tenth action head, open only when
the engine would allow the jump, which also starts Jump Control.

**Per round, small.** OpenAI Five found pure win/loss too sparse to learn
from and used shaped rewards; so will we. Each round: 0.1 times (share of
enemy hull destroyed this round minus share of our hull lost this round), and
a flat -0.002 per round so the policy does not stall. Shaping is tuned so a
whole game's shaping cannot outweigh the end-of-game reward.

What is not rewarded: anything a human would call style. No bonus for locks,
formations or dodging. If they win games, the policy will find them.

## 3. Self-play or an adversary

The research answer is clear and we should follow it:

- **Train by self-play**, against the policy's own current and past versions.
  OpenAI Five played 80% of its games against the latest policy and 20%
  against a pool of past ones, to stop the policy forgetting how to beat
  earlier strategies. AlphaStar went further with a league of specialised
  exploiters. We start with the 80/20 pool; a league is a later refinement.
- **Do not train against a fixed opponent.** A policy trained only against
  `close` or against Gemini learns to exploit that opponent's habits, and
  stops improving once it has. Training against an LLM is also impossible on
  cost: at 5 cents a game, a million games is $50,000.
- **Use fixed opponents to measure.** Every N training updates, play the
  policy against `hold`, `close`, its own earliest checkpoint, and a handful
  of games against Gemini Flash and Haiku. Keep an Elo ladder. That is the
  progress chart, and the LLM work this month is what makes it meaningful.
- **Randomise the scenario.** Train on a pool of scenarios and on random
  fleets (designs drawn from the priced templates, sides balanced by MCr
  within a tolerance, random starting geometry). A policy trained on
  Tessaly alone memorises Tessaly.

A first curriculum step is still useful: a few hours against `hold` and
`close` to check the plumbing and the reward signs before self-play starts.

## 4. The network and its inputs

"A 3-layer MLP should be enough" is right for a start, with one proviso: an
MLP needs fixed-size inputs, and fleets vary in size.

**Per-ship policy with shared weights.** One network decides for one ship at
a time, seeing that ship's own state, its side, and the enemies, with the
same weights used for every ship (parameter sharing). This handles any
fleet size, learns once for all ships, and is the standard shape for
multi-agent games. The value estimate for training (the critic) sees the
whole side.

**Observation**, per decision, in the deciding ship's frame, all scaled to
roughly [-1, 1]:

- Own ship: hull share, armour, thrust available, dodge reserve, posture,
  contacts held, best reach band, magazine, criticals, jump readiness.
- Own side: up to 20 ships, each as relative position, relative velocity,
  hull share, thrust, reach; nearest first, padded and masked.
- Enemies: up to 20, contacts and blips in one list, nearest first, each
  as relative position and velocity, a contact flag, and for contacts hull
  share (now visible), thrust, criticals, held/locked/watching flags and the
  odds both ways at every band. A blip carries what was known of her when
  last held. One list, because an enemy's slot is also her index in the
  target heads.
- Odds: for the deciding ship against each contact, hit chance and hull
  share both ways at each band (the odds table, as numbers).
- Memory for fog of war: rounds since each enemy was last seen and where.
  Partial observability otherwise argues for a recurrent network; memory
  features in the observation get most of the benefit for an MLP.
- Round number and rounds remaining.

1,184 floats as built (40 for the ship, 19 × 16 for her side, 20 × 42 for
the enemy). Hidden layers of 512, 512, 256: under a million parameters. The cap of 20 a side costs little compute; its cost is dilution,
since a 4-ship game leaves 16 slots empty and the network has to learn that
empty means nothing. Nearest-first ordering keeps that learnable. If it ever
hurts, the remedy is an attention-based encoder that takes any number of
ships, which is what OpenAI Five used.

**Actions**, one set per ship, as independent discrete heads with masks for
what is impossible this round (no fire at a ship not in contact, no band a
gun cannot reach):

| Head | Choices |
|---|---|
| Helm verb | hold, intercept, hold_range, open_range, follow (5) |
| Helm target | one of the 20 enemy slots |
| Band | Short, Medium, Long, Very Long |
| Guide | one of the 19 friend slots, for follow |
| Fire target | none, or an enemy in contact, with every gun that reaches (21) |
| Lock target | none, or an enemy in contact (21) |
| Posture | silent, sharing, active |
| Dodge reserve | 0, 1, 2 G |
| Assist gunners | yes, no |

Heads are independent, so a head the verb does not use (the guide, for an
intercept) is simply ignored. A ship that follows joins her guide's
squadron, flown exactly as a model commander's squadrons are: the guide at
the slowest member's thrust, eased off while anyone straggles.

`follow` is in from the start: a side's ships are decided in one step, so
guides' courses are resolved before their followers', and the engine
already refuses a guide who follows. Point defence allocation, software,
engineering and the captain start out automated by rules (the tactics layer
already spends Fire Control and can default point defence), and join the
action space later if the policy plateaus without them.

## 5. The environment: the engine in Python

Training loops are Python; the engine is Rust. The bridge is a Python
module built from a new Rust crate with PyO3 and maturin, the way other Rust
game simulators do it (Zarena, Toybox, rlgym-sim-rs):

- `reset(scenario, seed)` and `step(actions)` for both sides at once, since
  moves are simultaneous. Returns observations, rewards, done flags and
  action masks as NumPy arrays.
- **Vectorised in Rust.** One call steps K games in parallel on all cores
  (rayon). Python sees one batch; the engine does the work. This is the
  single biggest throughput decision.
- Scenario randomisation lives on the Rust side too, drawing from the
  templates and the price list.

**Throughput, and the one risk.** Measured on one core of this machine:

| Game | Rounds per second |
|---|---|
| Tessaly, 8 ships, both sides re-plotting every round | 52 |
| Tessaly, both sides holding course | 3,900 |
| Marduk, 4 ships, plotting | 23,000 |

Round resolution is fast. The flight-path solver, called once per ship per
round when every ship re-plots, takes a Tessaly round from a quarter of a
millisecond to about 20 ms (eight courses at roughly 2.5 ms each) and so
dominates by a factor of 75. On this machine's 12 cores that is about 600
rounds a second: 10 million rounds in some 5 hours, 50 million in a day.
Enough to start. Making the solver cheaper for training is then a tenfold
speedup for scaling, not a blocker: profile it first (its "out of guesses"
fallback brute-forces), cache courses while a target's motion has not
changed, and if needed give training an analytic rendezvous approximation
and keep the exact solver for play. Target: 1,000 rounds a second per core
with all ships plotting.

**As built (phase 1).** Through the Python layer, with random masked
choices across the four-scenario pool, 40 games at once:

| Threads | Game-rounds per second |
|---|---|
| 1 | 157 |
| 10, against `close` | 751 |
| 10, self-play | 1,013 |

A short MaskablePPO run trains at about 1,600 ship decisions a second,
learning included. Ten threads give five times one, not ten, because each
step waits for the slowest game in the batch.

Compiler settings are not the lever. The numbers were always from an
optimised release build, and link-time optimisation with tuning for this CPU
measured the same. A profile shows why: the time is in the solver itself,
a general nonlinear solver working on heap-allocated, dynamically sized
matrices, with a factorisation and allocations every iteration.

**Phase 2, done: a new rendezvous solver.** Writing each burn as the velocity
it adds makes the rendezvous equations linear; for a given flight time the
first burn's length comes from a quadratic, which leaves a single equation in
the flight time. The flight computer now scans that one number for its first
root and refines it by bisection, instead of solving for eight unknowns from
up to a hundred random starts. On 2,157 courses recorded from games of the
pool, it found every rendezvous the old solver found, plus ten, always the
earliest; 66 of the old solver's "rendezvous" did not arrive when flown. It
takes about 35 µs a course against 1.5 ms, and the old solver and its
sampling library are gone. Caching was dropped from the plan: 11% of courses
repeat, not worth it at this cost.

| Through the Python layer | Before | After |
|---|---|---|
| Game-rounds a second, 1 thread | 157 | 3,633 |
| Game-rounds a second, 10 threads | 751 | 17,868 |
| Ship decisions a second, self-play | 6,835 | 120,197 |

Training is now limited by the learner, not the game. The pursuit fallback
(one burn through the target's path) and missile guidance still use the
general solver; they reduce the same way, to one equation in time, if they
ever matter.

## 6. Toolkit

Recommendation: **PyTorch with Stable-Baselines3 and its `MaskablePPO`** to
start, graduating to a CleanRL-style single-file PPO when self-play needs
more control.

- **PPO** is the algorithm. It is what OpenAI Five used, it is robust to
  tuning, and every library has it.
- **Stable-Baselines3** is the easiest on-ramp for someone new to RL: a
  trained policy in a dozen lines, good defaults, logging to TensorBoard.
  `MaskablePPO` (sb3-contrib) gives the action masks the heads above need.
- **Self-play in SB3** is done the usual way: the environment holds a frozen
  copy of the opponent policy and plays it for the other side, swapped from
  a checkpoint pool every few updates. Simple and sufficient for the 80/20
  pool.
- **CleanRL** later, if we want full control of the loop (a league,
  recurrence, custom batching). Its PPO is a few hundred readable lines.
- **Not RLlib.** Powerful and distributed, and far more machinery than a
  single-machine project needs.
- **Gymnasium / PettingZoo** interfaces are what SB3 expects; the Rust
  module presents a Gymnasium vector environment.
- Experiment tracking: TensorBoard locally, Weights & Biases if we want it
  hosted; both are free at this scale.

## 7. Compute and cost

The surprise: this is a CPU problem, not a GPU problem. The network is tiny;
a forward pass for 8 ships is microseconds. The time goes to simulating
games, which is Rust on CPU cores. A GPU speeds up the training updates and
is worth having, but a cheap one.

- **Phases 1 to 4 run on this Mac.** It has 12 cores for simulation, and
  PyTorch drives Apple's GPU through MPS for the training updates, which SB3
  supports with `device="mps"`.
- **Scale on a rented box with many cores and one modest GPU.** An RTX 4090
  pod on RunPod is about $0.34 an hour; Vast.ai spot instances about
  $0.29 to $0.50 and interruptible; Lambda more. RunPod bills by the second
  and is the simple choice; Vast is cheaper if a run can resume from
  checkpoints, which ours will. Pick pods by CPU count as much as by GPU.
- **Budget.** 100 pod-hours is $35 to $70. A first useful policy should
  come in well under that. Evaluation games against the LLMs are the other
  cost, at about 5 cents a game: 200 games per evaluation point is $10.
- Google Cloud is possible since the project is there, but its GPUs cost
  several times more for this purpose.

## 8. Phases

1. **Environment.** The PyO3 crate; fixed-size observation encoding; the
   action heads with masks; reward; vectorised stepping; scenario
   randomisation. Test: a random policy plays legal games; `close` encoded
   through the same path plays as it does today.
2. **Solver speed.** Done: a one-variable rendezvous search, exact, 40 times
   faster per course and 23 times faster per game round. Test was 1,000
   rounds a second per core; it does 3,600.
3. **First training, locally.** PPO against `hold`, then `close`. Test: the
   policy beats `close` in most games within a few hours. This is where the
   reward design gets debugged.
4. **Self-play.** Checkpoint pool, 80/20. Evaluation ladder against `hold`,
   `close`, early checkpoints, and a few games each against Flash and
   Haiku. Test: Elo rises and the policy beats Flash in Tessaly.
5. **Scale.** Rented pod; scenario randomisation across the priced designs;
   longer runs; add action heads (squadrons, point defence) if the policy
   plateaus; try a recurrent policy for fog of war.
6. **Deploy.** Export the policy as ONNX, a standard file format for a
   trained network's layers and weights that many runtimes can execute, and
   run it inside the AI service as a `Commander` through an ONNX runtime
   crate in Rust, so there is no Python in production. Fleet Comms gets a
   rationale made from the chosen verbs, no model involved: "Kestrel:
   intercept Red Morrow to Medium, firing on her, locked, 1 G in reserve."
 The AI gate applies unchanged; the cost per round
   becomes zero, which changes who it can be offered to.

## 9. Setting up

For the person doing this for the first time, in order:

- Python tooling: `uv` for environments; `maturin` to build the Rust module;
  PyTorch; `stable-baselines3` and `sb3-contrib`; `gymnasium`; TensorBoard.
  Done: see `rl-env/README.md`. Training uses 10 of this Mac's 12 cores, the
  engine's threads and PyTorch's together, to leave two for everything else.
- A RunPod account with a small credit, for phase 5. Nothing is rented
  before then.
- Reading, one hour each: OpenAI's Dota 2 paper for the self-play and
  reward-shaping choices; SB3's PPO and MaskablePPO docs; CleanRL's PPO
  file, to see the whole algorithm in one place.

## 10. Decisions and open questions

Decided:

- One network with shared weights, run once per ship. The alternative, a
  joint network with a separate output block per ship slot, needs a fixed
  fleet and cannot carry what one slot learned to another.
- The training pool starts with the scenarios played so far: Tessaly
  Crossing, Fire over Jupiter, Marduk Encounter and Thebus. Fire over
  Jupiter is unbalanced (the Imperials won every LLM game) and gets more
  Aslan fighters or a modest extra ship, balanced first by the free
  baseline harness (100 games of `close` against `close`, as Tessaly was)
  and confirmed with a few LLM games. Random fleets come in phase 5.
- `follow` is in the first action space.
- Fleet Comms gets a programmatic rationale from the verbs. No LLM is
  called anywhere in this path: the policy is a complete alternative to the
  LLM commander.

Open:

- How much random-fleet play, and how to balance random fleets by MCr.
- Whether a recurrent policy is needed for fog of war, or memory features
  suffice.
- How the trained policy and the LLM coexist in play: the LLM for
  promptable, storyline games; the policy for cheap, strong, objective-only
  games.

Later, once this works: an LLM that turns a human's orders into an objective
function for the policy, as some robotics work does, so promptable play
comes back without an LLM in the loop every round. Not before.

## Sources

- [Dota 2 with Large Scale Deep Reinforcement Learning (OpenAI Five)](https://arxiv.org/pdf/1912.06680)
- [A Survey on Self-play Methods in Reinforcement Learning](https://arxiv.org/pdf/2408.01072)
- [Superhuman AI for Generals.io Using Self-Play Reinforcement Learning](https://arxiv.org/html/2606.23348v1)
- [Gym-μRTS: affordable full-game RTS research with deep RL](https://arxiv.org/pdf/2105.13807)
- [PettingZoo and multi-agent RL platforms](https://medium.com/@gwrx2005/top-10-github-repositories-for-multi-agent-reinforcement-learning-marl-platforms-05cc8d21a6c1)
- [6 best reinforcement learning tools in 2026](https://dev.to/ethan_5383afd058ff/6-best-reinforcement-learning-rl-tools-in-2026-21dg)
- [TorchRL: a data-driven decision-making library for PyTorch](https://arxiv.org/pdf/2306.00577)
- [RunPod vs Lambda vs Vast.ai: GPU pricing 2026](https://tech-insider.org/runpod-vs-lambda-vs-vast-ai-2026/)
- [7 cheapest cloud GPU providers in 2026](https://northflank.com/blog/cheapest-cloud-gpu-providers)
- [Zarena: Rust games with RL gym environments via PyO3](https://pypi.org/project/zarena/)
- [rlgym-sim-rs: a Rust simulator with PyO3 bindings](https://docs.rs/rlgym_sim_rs)
