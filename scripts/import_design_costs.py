#!/usr/bin/env python3
"""Copy prices from docs/design_costs.csv into the ship design files.

Fill in the cost_mcr column of the CSV, then run this from the repository root:

    python3 scripts/import_design_costs.py

Each design whose row has a price gets a `cost_mcr` field in its JSON file
under callisto/ship_templates/, right after `displacement`; a row with no
price leaves the design as it is. It prints what it changed, and anything in
the CSV that matches no design. Run with --check to see the changes without
writing them.

The ship_templates directory is bind-mounted into the local backend, so the
prices reach a running game within a few seconds. The canary and prod design
buckets are not touched: they are uploaded separately.
"""

import collections
import csv
import glob
import json
import sys

CSV = "docs/design_costs.csv"
DESIGNS = "callisto/ship_templates/*.json"


def main() -> int:
    check = "--check" in sys.argv
    prices = {}
    with open(CSV, newline="") as handle:
        for row in csv.DictReader(handle):
            text = (row.get("cost_mcr") or "").strip()
            if text:
                prices[row["design"]] = float(text)

    seen = set()
    changed = 0
    for path in sorted(glob.glob(DESIGNS)):
        with open(path) as handle:
            raw = handle.read()
        design = json.loads(raw, object_pairs_hook=collections.OrderedDict)
        if not isinstance(design, dict) or design.get("name") not in prices:
            continue
        name = design["name"]
        seen.add(name)
        price = prices[name]
        if design.get("cost_mcr") == price:
            continue
        out = collections.OrderedDict()
        for key, value in design.items():
            if key == "cost_mcr":
                continue
            out[key] = value
            if key == "displacement":
                out["cost_mcr"] = price
        print(f"{name}: {design.get('cost_mcr')} -> {price}  ({path})")
        changed += 1
        if not check:
            indent = 1 if '\n "' in raw else 2
            with open(path, "w") as handle:
                handle.write(json.dumps(out, indent=indent) + ("\n" if raw.endswith("\n") else ""))

    for name in sorted(set(prices) - seen):
        print(f"no design named {name!r}: check the spelling in {CSV}")
    print(f"{changed} design(s) {'would change' if check else 'updated'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
