"""Compare the network's play with its opponent's, from harness logs.

    .venv/bin/python scripts/analyse_games.py <dir with *-log.jsonl and *-outcomes.jsonl>

For each log, both sides' orders each round are set against the situation
the round began in (positions, hull, contacts, locks) and, for the network,
its own account of each choice. Prints, per series and side, how often each
side fires when it could, how many targets it spreads fire over, how often it
closes or opens the range, how it postures, how long it locks, and how its
games end.
"""

import collections
import glob
import json
import math
import sys
from pathlib import Path


def km(a, b):
    return math.dist(a, b) / 1000.0


def analyse(log_path: Path, outcomes_path: Path):
    rows = [json.loads(line) for line in open(log_path)]
    outcomes = {o["seed"]: o for o in map(json.loads, open(outcomes_path))} if outcomes_path.exists() else {}
    stats = collections.defaultdict(collections.Counter)
    for row in rows:
        side = row["side"]
        # The network explains its choices; a model does not.
        who = "net" if row.get("explain") else "model"
        ships = {s["name"]: s for s in row.get("ships", [])}
        ours = [s for s in ships.values() if s["side"] == side]
        enemies = [s for s in ships.values() if s["side"] != side]
        c = stats[who]
        targets = collections.Counter()
        for name, intent in row["intents"]:
            me = ships.get(name)
            if me is None:
                continue
            c["ship-rounds"] += 1
            held = [e for e in enemies if e["name"] in me["contacts"]]
            nearest = min((km(me["position"], e["position"]) for e in enemies), default=None)
            fire = intent.get("fire") or []
            if held:
                c["could fire"] += 1
                if fire:
                    c["fired"] += 1
            for engage in fire:
                targets[engage["target"]] += 1
            helm = intent["helm"]
            verb = helm if isinstance(helm, str) else next(iter(helm))
            c[f"helm {verb}"] += 1
            posture = intent.get("posture") or ("Active" if me["active_sensors"] else "Silent")
            c[f"posture {posture}"] += 1
            watched = any(name in e["contacts"] for e in enemies)
            if not watched:
                c["unwatched"] += 1
                if posture == "Silent":
                    c["silent while unwatched"] += 1
            if me["locks"]:
                c["holding a lock"] += 1
            if intent.get("evade"):
                c["dodging"] += 1
            engineering = intent.get("engineering", [])
            if any(v == "Jump" for v in engineering):
                c["jump order"] += 1
            if any(isinstance(v, dict) and "Power" in v and not v["Power"].get("online", True) for v in engineering):
                c["powering down to jump"] += 1
            if nearest is not None and nearest < 1250:
                c["at Short"] += 1
        if targets:
            c["firing rounds"] += 1
            c["targets"] += len(targets)
            # Fire on a ship already at a sliver: overkill risk.
            for target in targets:
                t = ships.get(target)
                if t and t["hull"] <= 0.15 * t["design_hull"] and targets[target] > 1:
                    c["piling on a nearly dead ship"] += 1
    ends = collections.Counter()
    for seed, outcome in outcomes.items():
        for side, result in outcome["sides"].items():
            who = "net" if result["commander"].startswith("net") else "model"
            if outcome.get("winner") == side:
                ends[(who, "won")] += 1
            elif side in outcome.get("fled", []):
                ends[(who, "fled")] += 1
            elif result["ships_at_end"] == 0:
                ends[(who, "destroyed")] += 1
            else:
                ends[(who, "other")] += 1
            ends[(who, "hull kept %")] += round(100 * result["hull_at_end"] / max(1, result["hull_at_start"]))
    return stats, ends, len(outcomes)


def pct(c, part, whole):
    return f"{100 * c[part] / c[whole]:.0f}%" if c[whole] else "-"


def main():
    folder = Path(sys.argv[1])
    for log in sorted(folder.glob("*-log.jsonl")):
        series = log.name[: -len("-log.jsonl")]
        stats, ends, games = analyse(log, folder / f"{series}-outcomes.jsonl")
        print(f"\n== {series} ({games} games)")
        for who, c in sorted(stats.items()):
            helm = ", ".join(f"{k[5:]} {pct(c, k, 'ship-rounds')}" for k in sorted(c) if k.startswith("helm "))
            print(
                f"  {who:12s} fires when it could {pct(c, 'fired', 'could fire')}; "
                f"targets per firing round {c['targets'] / max(1, c['firing rounds']):.1f}; "
                f"piling on {c['piling on a nearly dead ship']}; locked {pct(c, 'holding a lock', 'ship-rounds')}; "
                f"silent while unwatched {pct(c, 'silent while unwatched', 'unwatched')}; dodging {pct(c, 'dodging', 'ship-rounds')}; "
                f"jump checks {c['jump order']}, powering down {c['powering down to jump']}"
            )
            print(f"  {'':12s} helm: {helm}")
            e = {k[1]: v for k, v in ends.items() if k[0] == who}
            if e:
                print(
                    f"  {'':12s} ends: won {e.get('won', 0)}, fled {e.get('fled', 0)}, destroyed {e.get('destroyed', 0)}, "
                    f"other {e.get('other', 0)}; hull kept {e.get('hull kept %', 0) / max(1, games):.0f}%"
                )


if __name__ == "__main__":
    main()
