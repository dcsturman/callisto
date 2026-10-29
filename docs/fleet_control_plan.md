# Fleet control: plan

The epic `epic-fleet-control` covers four pieces of work:
- orders given in words, which the per-side network carries out;
- squadrons;
- a fleet commander's interface that works at 100+ ships;
- High Guard capital ships and carriers.

It ends with large AI-against-AI battles.

## Decisions

- **The network does the fighting; the LLM writes orders.** The LLM is called only when a commander
  gives an order or one of its own wake-up conditions fires, not every round.
- **Everything is a squadron script.**
  - **A ship order** is an order to a squadron of one.
  - **A fleet order** rewrites several squadrons' scripts.
- **A squadron is at most 20 ships.** That is one network pass. Moving the cap means retraining a
  bigger network.
- **Who gives orders:**
  - **Fleet Command** gives orders to the whole team.
  - **Squadron Command**, a new role, gives orders to one squadron. It can be held by a ship's
    captain, like an admiral on a flagship.
- **A squadron is steered mainly by what its network is shown:** the enemies it is assigned, plus
  the nearest ~8 that can reach it.
  - The LLM turns words like "their fighters" into a rule: by design, group or tonnage.
  - Code applies the rule every round.
- **Movement orders:**
  - the network's own choice;
  - a burn: toward or away from a point, at a thrust or up to a speed;
  - an arrival: a point, a range band from a group, by a given round. The flight-path solver works
    out the burns.

  Each step runs until a condition is met, then hands over to the network.
- **The LLM only sees a compact summary:**
  - each design on the plot, described once;
  - enemy groups;
  - one line per squadron;
  - named reference points.

  It never sees coordinates. The estimate is about 4–7k tokens in and 1–3k out per call, roughly
  $0.01 on Gemini 3.8 Flash.

## Strategy, as agreed

- **Real strategies:**
  - focus fire: concentrate in space, in time (to swamp point defence), and pick the target;
  - control the range: the faster side picks the band;
  - hold still and put the thrust into dodging.
- **Dropped as weak or impractical:**
  - pulling back damaged ships;
  - pinning the enemy in a gravity well;
  - high-speed passes;
  - formations and pincers.
- **Stealth is a raider's tool.** A firing Advanced-stealth ship is found about 83% per searching
  ship per round by an equal-TL enemy.
- **The round limit** matters only in scenarios that set one, and the network already sees it.

## Phases

| Phase | Branch | What | Depends on |
|---|---|---|---|
| 0 | epic | **Baseline checks:** does a cruiser already use its bays well (Fighter Screen)? How does the network play when shown only some enemies? | none |
| 1 | `squadrons`, off canary | **Squadron data model:** id, name, team, members; the 20-ship cap; create, rename, assign and dissolve; in scenario files; a minimal editor; the Squadron Command role | none |
| 2 | epic | **Hybrid orders:** code that carries out squadron scripts; the situation report and prompt; when the LLM is called; the orders panel back with a fleet / squadron / ship selector; tests of how well orders are translated, across models | 1 |
| 3 | `fleet-ux`, off canary | **Fleet commander UX at scale:** first, a 200-ship test of server round time, update size and frame rate. Then a squadron roster, one marker per squadron when zoomed out, box-select to make squadrons, adding many ships of a design at once in the builder, and orders from the roster. | 1; runs alongside 2 |
| 4 | `capital-ships`, off canary | **Capital ships and carriers:** a write-up of the High Guard 2024 and Imperial Navy rules; spinal mounts; rules that change with size; carriers launching and recovering craft (a new network action); large designs; retraining | 2 and 3 settled |
| 5 | epic | **Large tests:** squadron behaviour (six squadrons coordinated); a 100+ ship AI-against-AI battle; a fully AI admiral that forms its own squadrons from the scenario goal | 2–4 |

## Phase 0 results (2026-10-27)

**Setup.** Fighter Screen: each side is a Kinunir cruiser plus 15 Light Fighters. The built-in per-side
network plays both sides, 200 games per variant, 40-round limit.
- **The tool:** `ai-ships/examples/squadrons.rs` splits a side into squadrons. Each squadron gets its
  own network pass.
- **`own>shown+N`** shows the squadron only its assigned enemies, plus N threats: enemies that can hit
  it where they are now.
- **`own>>shown+N`** shows everything, but the squadron may fire only on those.

**Do cruisers use their weapons well without orders?**
- **Mostly yes.** Particle beams go 92% to the enemy cruiser, where they hit 56% of the time for
  ~23 damage. Against fighters they hit 24% and overkill.
- **The exception is missiles.** About 78% of a cruiser's missiles are fired at the enemy cruiser,
  and only 3% of those get through its point-defence battery. Missiles fired at fighters arrive 77%
  of the time and hit 55%.
- **Why:** the expected damage the per-mount gun sharing uses (`share_fire`, via `BandOdds`) counts
  missiles before point defence.

**Splitting into squadrons, and showing only some enemies.** Wins out of 200:

| Variant | As Blue | As Red |
|---|---|---|
| One pass for the whole side (baseline) | 103 | 89 |
| Two squadrons, every enemy shown | 98 | 84 |
| Fighters shown only enemy fighters | 49 | — |
| Fighters shown enemy fighters plus 8 threats | 77 | 74 |
| The same with the cruiser on the cruiser | 82 | 70 |
| Fighters shown everything, fire only on fighters plus 8 threats | 99 | 87 |
| Two squadrons, each shown every enemy and every ally (allies' orders dropped) | 94 | 101 |

- **Separate passes cost little:** about 5 wins out of 200. The share of hull kept drops from about
  21% to 15%.
- **Hiding enemies costs 10–25 wins, even with threats included.**
  - The fighters fire at much the same targets either way; what changes is how they fly. They intercept
    enemy fighters 24% of the time, against 9%, and close in without seeing the cruiser.
  - Showing every enemy and restricting only fire costs nothing. In this scenario the restriction
    rarely binds, because the cruiser counts as a threat.
- **Showing allies restores the coordination.** Squadrons that also see the rest of the side (as own
  ships whose orders are dropped) win 195 in total, against 192 for one pass and 182 for squadrons
  without allies. The hull kept rises from about 14% back to 18–20%.
- **Conclusion:** show a squadron every enemy (up to 20, assigned ones first), and restrict what it
  may engage with masks. Do not hide enemies from the network.

## Missiles shared out against point defence (2026-10-27)

**The change.** Launchers are no longer shared out with the guns. `ai-ships/src/network/missiles.rs`
decides for the whole side, after the guns:
- **Each target's point-defence pool** comes from its batteries' dice.
- **Missiles are grouped by the round they land,** counting those already in flight.
- **A package is committed** to a defended target only when what it is expected to get through beats
  what those launchers would do against undefended targets.
- **The rest go where they do most,** up to each target's remaining hull, or are held and not fired.
- **`network-legacy`** plays the old way, for comparison.

**Results**, 200 games each:

| Scenario | New against old | Notes |
|---|---|---|
| Fighter Screen | New wins 126–68 as Blue, and 124–65 as Red | The cruiser stops firing into the other cruiser's point defence (6 a round can never beat ~21) and sends every missile at fighters |
| Fighter Swarm | No change (106 and 103 wins for the missile side) | Fighters have no point defence, so both versions already sent missiles at them |
| Missile Wall (an experiment: 3 pocket warships, 36 missiles a round, against 2 Kinunirs and 6 fighters) | Both win all 200 | New ends in 4.4 rounds rather than 4.7, with 31% fewer missiles fired |

## Phase 2a: the squadron order executor (2026-10-27)

`ai-ships/src/fleet.rs` plays a side as squadrons under a `Directive`, with no LLM yet:
- **A script per squadron.** `*` covers every squadron without its own script.
- **Each script is a list of steps.** A step can set:
  - a goal, and a post;
  - who the squadron may `engage`: designs, ships, tonnage;
  - how many `threats` it may also engage;
  - its `movement`: `network`, `burn` toward or away from a place, or `arrive` at a place or a range band of it by a round;
  - `until`: a round, `within` a band of a place, `fired_on`, `losses`, or `any` of these.
- **One network pass per squadron**, through `SideCommander::decide_pass`:
  - the squadron's allies fill its own-ship slots, and their orders are dropped;
  - enemies are shown assigned first, then threats, then nearest;
  - targets outside the assignment are masked, and guns and missiles are shared out only among the allowed ones;
  - movement is done by helm rewrites: `Station` with a thrust cap.
- **Ships in no squadron** are grouped by name, at most 20 per group.
- **In the harness:** `--side Blue=fleet:<directive.json>`.

**Results.** Fighter Screen, Blue in two squadrons (Lance: 15 fighters; Flag: the cruiser), against the plain network, 100 games each:

| Orders | Blue wins | Red wins |
|---|---|---|
| None (squadron passes only) | 48 | 50 |
| Lance engages Light Fighters, plus 8 threats | 51 | 45 |
| Everyone arrives at Medium of the enemy by round 4, then fights | 51 | 45 |
| Everyone holds (Hold goal, starting point) | **61** | 33 |
| Lance engages Light Fighters only, no threats | 35 | 59 |

- **Orders bind:** under the strict assignment the fighters fire no shots at the cruiser.
- **A bad order loses:** the strict assignment costs 25 wins.
- **Holding still and dodging wins this scenario,** as the strategy notes predicted.
- **The timed arrival flies the meeting point** until each squadron is fired on. Medium by round 4 needed about 12 G here, so the ships arrived late.

## Phase 2b: what the fleet commander is told (2026-10-27)

`ai-ships/src/admiral.rs` and `ai-ships/src/prompt/admiral.md`:

**System prompt.** About 1,500 tokens, the same on every call and cached. It covers:
- the commander's job, and how fleet, squadron and ship orders apply;
- everything a script can say;
- how these fights go: the agreed strategy;
- three worked examples, which a test parses as real orders.

**Tool `give_orders`.** Its schema is about 1,500 tokens, also cached. Its arguments are a `Directive`. The format is now flat, plain objects with optional fields:
- places: `planet`, `ship`, `squadron`, `enemies`, `at_km`;
- `until`: ends on any one given;
- `movement`: a `mode` plus its fields.

Scripts can also name a single `ship`, which then fights in its own pass. `call_me_when` triggers are part of the format and take effect in 2c.

**Situation.** Built fresh each call:
- the round;
- the orders by scope, read from the text ships carry, with ships given the same orders on one line;
- each design on the plot, described once: tonnage, thrust, hull, armour, stealth, and every mount with its furthest band;
- our squadrons: make-up, hull, speed, nearest enemy group, script step;
- the enemy, grouped by design and position (E1, E2, …);
- planets, and the current scripts.

It gives no coordinates. Measured sizes:

| Scenario | Situation |
|---|---|
| Thebus | ~420 tokens |
| Fighter Screen | ~320 tokens |
| Aslan Border Clash | ~650 tokens |
| A made-up 100 against 100, five squadrons a side | ~790 tokens |

A call is therefore about 3k cached tokens plus under 1k fresh, which is well under 1¢ on Gemini 3.8 Flash; most of the cost is the reply. `harness --show-admiral <side>` prints the lot for any scenario.

## Phase 2c: the model in the loop (2026-10-27)

**Provider.** `Vertex::call(spec, system, situation, &Tool)` calls any tool and returns its raw arguments and token counts. The old per-round decision is now a thin layer on top. A prompt that Gemini refuses to cache (explicit caching needs 4,096 tokens; ours is ~2,700) is remembered for a day, rather than asked for on every call.

**`admiral:<model>`.** For example `admiral:gemini`, in the harness or as the service's `CALLISTO_AI_MODEL`. It is `ai_ships::admiral::Admiral`: a model writing scripts for the squadron executor. The model is asked only:
- when a living ship's orders text differs from what it last answered (ships lost do not count);
- when one of its own `call_me_when` triggers fires: a squadron's losses since then, a new enemy found, or a round.

With no orders at all it is never asked. If an answer cannot be read, the squadrons keep their scripts and the model is asked again next round. Every call is logged as a JSON line: situation, answer, tokens, cost, seconds.

**First game.** Fighter Screen, Blue `admiral:gemini` (Gemini 3.8 Flash) with the scenario's orders, against the network:
- two calls in the game: the orders in round 1, and the enemy cruiser found in round 2;
- 5–7 seconds a call;
- $0.0074 for the game, about 0.3–0.5¢ a call;
- the model gave the cruiser orders of her own, since hers differed from the fighters', and set triggers for new enemies and for her losses.

## Phase 2d: orders in the game (2026-10-27)

**Orders are stored by layer, on the ships.** Each ship carries her team's `fleet_orders`, her squadron's `squadron_orders`, and her own `orders`. They save with scenarios and reach the AI unchanged.
- `SetOrders` takes a `scope` (`Ship`, `Squadron` or `Fleet`) and fans the text out to the right ships.
- A ship joining a squadron or a team takes up its orders; leaving a squadron drops its orders.
- The admiral reads the layers directly, and asks again when any living ship's layers change.

**Orders panel** (`OrdersPanel.tsx`):
- **Who can use it:** the referee for any team, a fleet commander for their own team, a squadron commander for their squadron and its ships.
- **What it does:** pick Fleet, a squadron or a ship; write; Give orders (or Ctrl/⌘-Enter). The orders in force are listed, each with Withdraw.
- **Other boxes:** the per-ship orders box (Ship Command) and Add Ship's are switched back on. The builder's team box now writes fleet orders.

**Fleet Comms** shows the model's reply as the side's plan. Each ship's report now opens with her squadron's step, e.g. "Lance (step 1 of 2: engage Light Fighter; arrive at Medium of the Light Fighters by round 6; until round 6 or fired on)".

**End to end.** Fighter Screen, with Blue given fleet orders ("Hold your ground and let them come to us.") and Lance squadron orders ("Keep their fighters off the cruiser."), playing as `admiral:gemini` against the network. The model made:
- Lance: hold on Flag's position, engage Light Fighters only;
- Flag: hold;
- call me on new enemies, or 25% losses.

Blue won.

## Phase 2e: how well orders are translated (2026-10-29)

**The test.** `ai-ships/corpus/admiral_orders.json` holds 61 cases (written by `corpus/make_admiral_orders.py`). Each takes a side of a scenario: Fighter Screen, Aslan Border Clash, Fighter Swarm, Picket at the Refuelling Point, Convoy Escape or Thebus.
- The side's ships are put in squadrons, the scenario's orders are swapped for the case's (fleet, squadron and ship), and the model is called once.
- Each case checks the `Directive` that comes back, e.g. "Lance engages Light Fighters", "every group holds", "dodge ≥ 3", "Flag arrives at Long of CS Tarsis by round 4", "CS Hesperus has her own script, ending in escape", "`say` explains what could not be ordered".
- Every case also checks that the answer names only squadrons, ships, designs and planets the situation gave, and that rules name only enemies.

**The runner.** `cargo run --release --example admiral_eval -- --model gemini --model gemini:gemini-3.5-flash-lite --repeat 3 --out runs/x.jsonl`.
- It writes a JSON line per call: answer, checks, tokens, cost and seconds. Then it prints the summary tables.
- `--rescore` checks old answers against the cases as they are now, at no cost.
- It estimates the spend before starting and stops at `--budget`.

**Models.** Gemini 3.8 Flash (the default) and Gemini 3.5 Flash-Lite. Claude Haiku 5.5 could not be tested:
- Vertex answers 429, "quota exceeded for anthropic-claude-haiku", on `global`. Sonnet gets the same answer.
- The regional endpoints answer 404.
- No Anthropic API key is set.

**Results.** Three tries per case with the original prompt, then four with the final one (two of the four were with the last version but one). A case counts only if every check passes. Calls that failed on transport are left out of the accuracy.

| Model | Prompt | Cases right | Checks right | Cost/call | Median s | p90 s |
|---|---|---|---|---|---|---|
| Gemini 3.8 Flash | original | 177/183 (97%) | 99.0% | $0.0031 | 2.9 | 15.8 |
| Gemini 3.8 Flash | final | 235/240 (98%); 61/61 on the very last | 99.1% | $0.0040 | 5.5 | 21.4 |
| Gemini 3.5 Flash-Lite | original | 127/183 (69%) | 85.8% | $0.0015 | 1.2 | 3.2 |
| Gemini 3.5 Flash-Lite | final | 203/244 (83%) | 95.3% | $0.0013 | 1.2 | 3.5 |

- **Size of a call:** about 3.8k tokens in and 170–380 out.
- **Reliability:** in two runs of 6–8 calls at a time, 15 of 3.8 Flash's ~610 calls failed: three 40-second timeouts in a row, or a 429. The slowest answer took about 2 minutes. Flash-Lite never failed.
- **Spend:** about $4.30 for the whole evaluation, with every rerun.

By category (cases right; original prompt, then final):

| Category | 3.8 Flash | Flash-Lite |
|---|---|---|
| plain goals | 9/9, then 11/11 | 9/9, then 12/12 |
| hold, with a post | 15/15, then 20/20 | 10/15, then 14/20 |
| targets (design, tonnage, name) | 27/27, then 36/36 | 18/27, then 32/36 |
| timed arrivals | 15/15, then 17/19 | 13/15, then 17/20 |
| burns | 12/12, then 16/16 | 10/12, then 15/16 |
| dodge | 12/12, then 16/16 | 10/12, then 15/16 |
| ship-level orders | 15/15, then 20/20 | 15/15, then 18/20 |
| layered fleet + squadron + ship | 15/15, then 20/20 | 7/15, then 13/20 |
| ambiguous (least change) | 12/12, then 15/15 | 9/12, then 8/16 |
| cannot be expressed | 9/15, then 19/19 | 4/15, then 17/20 |
| withdrawals | 12/12, then 16/16 | 8/12, then 16/16 |
| tricky phrasings (the user's own among them) | 24/24, then 29/32 | 14/24, then 26/32 |

**What went wrong.**
- **3.8 Flash, original prompt:**
  - it named an enemy not yet found (Thebus: "HMS Executor", still a blip) and burned toward her, which the executor ignores;
  - it carried out "board CS Tarsis" and "take her alive" as attacks without saying it could not do the rest.
- **3.8 Flash, final prompt:**
  - twice it gave an arrival with no `mode`, so the ships did not move. `mode` is now required, and the last run had none of these;
  - "hang back at Long range" became a hold whose post was the enemy with a 25,000 km radius. The prompt now says to `arrive` at a band.
- **Flash-Lite:**
  - **Scripts with no `squadron`:** 32 of its 183 original answers had one. The executor silently drops them.
  - **Layering:** it read squadron orders as replacing the fleet's, which dropped the fleet's hold.
  - **Copying example 1:** with Lance and Flag on the plot, it split the targets "fighters / cruiser" when nobody asked.
  - **Our own ships as targets:** for "Support the escorts" it engaged IMS Steadfast and IMS Valiant.
  - **Wrong field or step:** `toward` where an arrival needs `at`; `until` on the wrong step.
  - **Wrong names:** "Kald0r", "squadron E1", "planet Valiant".
  - **What remains:** mostly layering, needless engage limits on ambiguous orders, and holds without `goal: hold`.

**Changes to the prompt and schema** (`prompt/admiral.md`, `admiral::schema`):
- **Layering:**
  - squadron orders add to the fleet's, and win only where the two conflict, with a one-line example;
  - a ship's own script still carries her fleet's and squadron's orders.
- **What cannot be expressed:** a list (boarding, taking a ship alive, aiming at drives or weapons, formations, ends the scripts cannot test). `say` must name the part not ordered.
- **Names:** use only names the situation lists. An unidentified enemy cannot be named: say so, and ask for `new_enemies`.
- **`until`:** it goes on the step it ends, with an example.
- **Range:** to stand off at a range, `arrive` at a band; a post is a place, not a range.
- **The examples** now use made-up names (Wasps, Line, Arbiter), with a note that they show only the form. This stopped Flash-Lite copying example 1.
- **Schema:**
  - a script's `squadron` is required (for a ship's own script, it is her squadron);
  - movement's `mode` is required;
  - `toward`, `away` and `thrust` are marked burn only, and `at`, `band` and `round` arrive only;
  - rules are marked enemies only;
  - a post works only with `goal: hold`.

The executor (`fleet.rs`) is unchanged. Two orders show a real gap: "until she is destroyed", and "cover the merchants until they are gone". No `until` or `call_me_when` can test whether a ship or squadron is destroyed or has jumped. With the final prompt, 3.8 Flash says so and holds. A `gone` condition (ships or a squadron destroyed or jumped) would let the Convoy Escape escort be ordered properly. It is worth adding before Phase 5.

**Recommendation: keep Gemini 3.8 Flash as the default.**
- It gets 98–100% of cases right, and its mistakes were all closed by the prompt and schema changes.
- Flash-Lite costs a third as much and answers in about a second. But it still gets about one order in six wrong, and its mistakes are silent: a script that does something else. Phase 2a showed that a bad order costs about 25 wins in 100.
- At about 0.4¢ a call and a few calls a game, the saving is not worth it.
- Watch 3.8 Flash's occasional stalls of 40 s or more. The admiral already keeps the old scripts and asks again next round when a call fails.
