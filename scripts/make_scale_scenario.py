#!/usr/bin/env python3
"""Write a large fleet-against-fleet scenario for scale testing.

Each side has 100 ships in six squadrons of mixed designs from
callisto/ship_templates. The two fleets start 100,000 km apart at rest. Run
from the repo root:

    ./scripts/make_scale_scenario.py > "callisto/scenarios/Scale Test - 200 Ships.json"

Pass a number to change the ships per side. Squadrons are scaled with it, so
above 100 a side they pass the 20-ship limit: such a file is for timing only.
"""
import json
import sys

# (squadron, design, count). 100 ships a side; no squadron above 20.
SQUADRONS = [
  ("Line", "Colonial Cruiser - Kinunir", 2),
  ("Line", "Destroyer Escort - Fer-de-Lance", 4),
  ("Line", "Patrol Corvette - Type T", 4),
  ("Escort", "Gazelle", 6),
  ("Escort", "System Defense Boat", 6),
  ("Escort", "Torpedo Boat", 4),
  ("Lance", "Light Fighter", 20),
  ("Spear", "Light Fighter", 20),
  ("Arrow", "Light Fighter", 20),
  ("Hammer", "Heavy Fighter", 14),
]

KM = 1000.0
SIDE_GAP_KM = 100_000
SQUADRON_GAP_KM = 10_000
SHIP_GAP_KM = 1_000
ROW = 5


def side(team, sign, scale):
  ships = []
  # Per squadron: its place in the line, and how many ships it has so far.
  squadrons = {}
  for squadron, design, count in SQUADRONS:
    count = max(1, round(count * scale))
    place = squadrons.setdefault(squadron, [len(squadrons), 0])
    for _ in range(count):
      slot = place[1]
      place[1] += 1
      x = sign * (SIDE_GAP_KM / 2 + (slot // ROW) * SHIP_GAP_KM)
      y = (place[0] - 2.5) * SQUADRON_GAP_KM + (slot % ROW - ROW // 2) * SHIP_GAP_KM
      ships.append({
        "name": f"{team} {squadron} {slot + 1:02d}",
        "position": [x * KM, y * KM, 0.0],
        "velocity": [0, 0, 0],
        "plan": [[[0, 0, 0], 50000]],
        "design": design,
        "team": team,
        "squadron": squadron,
        "goal": "destroy",
      })
  return ships


def main():
  per_side = int(sys.argv[1]) if len(sys.argv) > 1 else 100
  scale = per_side / sum(count for _, _, count in SQUADRONS)
  scenario = {
    "metadata": {
      "name": f"Scale Test - {2 * per_side} Ships",
      "owner": "dcsturman@gmail.com",
      "description": (
        f"Two fleets of {per_side} ships in six squadrons each, 100,000 km apart at rest: cruisers, "
        "destroyer escorts, corvettes, boats and three wings of fighters. For testing how the game and its "
        "fleet tools hold up with hundreds of ships."
      ),
    },
    "planets": [],
    "ships": side("Blue", -1, scale) + side("Red", 1, scale),
  }
  json.dump(scenario, sys.stdout, indent=2)
  print()


if __name__ == "__main__":
  main()
