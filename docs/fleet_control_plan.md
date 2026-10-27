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
