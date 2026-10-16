"""Rate checkpoints against each other and the baselines.

    .venv/bin/python scripts/evaluate.py checkpoints/self-*/policy_*.zip --games 200

Every checkpoint plays every baseline given (default close and hold) and, with
--round-robin, every other checkpoint. Prints each match's score, each
checkpoint's win rate against close by scenario and side, and Elo ratings
with close at 1000.
"""

import argparse

from callisto_rl import arena


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("checkpoints", nargs="+")
    parser.add_argument("--baseline", action="append", help="baselines to play; default close and hold")
    parser.add_argument("--games", type=int, default=200, help="games per match")
    parser.add_argument("--round-robin", action="store_true", help="also play the checkpoints against each other")
    parser.add_argument("--threads", type=int, default=8)
    args = parser.parse_args()

    baselines = args.baseline or ["close", "hold"]
    policies: dict = {}
    matches = []
    for i, player in enumerate(args.checkpoints):
        opponents = list(baselines)
        if args.round_robin:
            opponents += args.checkpoints[i + 1 :]
        for opponent in opponents:
            match = arena.play(player, opponent, args.games, threads=args.threads, policies=policies)
            matches.append(match)
            print(f"{arena.short(player):45s} vs {arena.short(opponent):45s} score {match.score:.2f}", flush=True)
            if opponent == "close":
                for key, rate in arena.breakdown(match).items():
                    print(f"    {key:40s} {rate:.2f}")
    print()
    for player, rating in sorted(arena.ratings(matches).items(), key=lambda item: -item[1]):
        print(f"{rating:7.0f}  {arena.short(player)}")


if __name__ == "__main__":
    main()
