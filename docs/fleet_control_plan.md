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
