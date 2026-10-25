"""How fast the engine plays: rounds a second, with random masked choices.

    .venv/bin/python scripts/bench.py --games 40 --threads 10 --steps 50
"""

import argparse
import time

import numpy as np

from callisto_rl import HEAD_SIZES, ShipVecEnv, default_pool


def random_actions(masks: np.ndarray, rng: np.random.Generator) -> np.ndarray:
    actions = np.zeros((masks.shape[0], len(HEAD_SIZES)), dtype=np.int64)
    at = 0
    for head, size in enumerate(HEAD_SIZES):
        open_ = masks[:, at : at + size]
        # A random open choice per row: random scores, closed ones pushed down.
        scores = rng.random(open_.shape) + open_
        actions[:, head] = scores.argmax(axis=1)
        at += size
    return actions


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--games", type=int, default=40)
    parser.add_argument("--threads", type=int, default=10)
    parser.add_argument("--steps", type=int, default=50)
    parser.add_argument("--opponent", default="close", help="a baseline, or 'self'")
    parser.add_argument("--scenario", action="append", help="scenario file name; repeat; default the pool")
    args = parser.parse_args()
    env = ShipVecEnv(
        default_pool(args.scenario) if args.scenario else None,
        games=args.games,
        opponent=None if args.opponent == "self" else args.opponent,
        threads=args.threads,
    )
    rng = np.random.default_rng(0)
    env.reset()
    start = time.perf_counter()
    for _ in range(args.steps):
        env.step(random_actions(env.action_masks(), rng))
    elapsed = time.perf_counter() - start
    rounds = args.games * args.steps
    print(
        f"{args.games} games, {env.num_envs} ships, {args.threads} threads: "
        f"{rounds / elapsed:.0f} game-rounds/s, {env.num_envs * args.steps / elapsed:.0f} ship-decisions/s, "
        f"{len(env.finished)} games finished"
    )


if __name__ == "__main__":
    main()
