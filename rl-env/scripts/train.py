"""Train a policy with MaskablePPO: against a baseline, or in self-play.

    .venv/bin/python scripts/train.py --opponent self --steps 20000000
    .venv/bin/python scripts/train.py --opponent close --steps 2000000

Self-play draws each game's opponent from a pool: the learner's current
policy, snapshots of earlier ones, and the baselines (see
``callisto_rl.selfplay``). Training stops after ``--steps`` ship decisions.

Writes TensorBoard logs to runs/ (``.venv/bin/tensorboard --logdir runs``)
and checkpoints to checkpoints/<name>/. Engine and PyTorch threads together
are kept to --threads, so the machine stays usable.
"""

import argparse
import collections
import time
from pathlib import Path

import torch
from sb3_contrib import MaskablePPO
from stable_baselines3.common.callbacks import BaseCallback

from callisto_rl import Mix, SelfPlayVecEnv, ShipVecEnv, default_pool

HERE = Path(__file__).resolve().parents[1]
WINS = ("won", "drove off")


class Outcomes(BaseCallback):
    """Log how the learner's games came out: by opponent, and against the
    baselines by scenario and side."""

    def __init__(self, env, window: int = 300):
        super().__init__()
        self.env = env
        self.seen = 0
        self.recent: collections.deque = collections.deque(maxlen=window * 4)

    def _on_step(self) -> bool:
        self.recent.extend(self.env.results[self.seen :])
        self.seen = len(self.env.results)
        return True

    def _on_rollout_end(self) -> None:
        if not self.recent:
            return
        by_opponent = collections.defaultdict(list)
        by_scenario_side = collections.defaultdict(list)
        for result in self.recent:
            by_opponent[result["opponent"]].append(result["outcome"])
            if result["opponent"] not in ("self", "past"):
                by_scenario_side[f"{result['scenario']}/{result['side']}"].append(result["outcome"] in WINS)
        for opponent, outcomes in by_opponent.items():
            counts = collections.Counter(outcomes)
            for outcome in ("won", "drove off", "draw", "fled", "lost"):
                self.logger.record(f"vs_{opponent}/{outcome.replace(' ', '_')}", counts[outcome] / len(outcomes))
            self.logger.record(f"vs_{opponent}/win_rate", sum(counts[w] for w in WINS) / len(outcomes))
        for key, wins in by_scenario_side.items():
            self.logger.record(f"win_rate/{key}", sum(wins) / len(wins))


class Snapshots(BaseCallback):
    """Save a checkpoint every so often, and in self-play add it to the pool."""

    def __init__(self, env, every: int, folder: Path):
        super().__init__()
        self.env = env
        self.every = every
        self.folder = folder
        self.next = every

    def _on_step(self) -> bool:
        if self.num_timesteps >= self.next:
            self.next += self.every
            self.model.save(self.folder / f"policy_{self.num_timesteps}_steps")
            if isinstance(self.env, SelfPlayVecEnv):
                self.env.snapshot()
                self.logger.record("pool/snapshots", len(self.env.snapshots))
        return True


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--opponent", default="self", help="'self' for self-play, or a baseline (close, hold, idle, random)")
    parser.add_argument("--scenario", action="append", help="scenario file name; repeat; default the pool")
    parser.add_argument("--games", type=int, default=64)
    parser.add_argument(
        "--steps", type=int, default=1_000_000, help="learner ship decisions to train on; training stops there"
    )
    parser.add_argument("--rollout", type=int, default=64, help="rounds per entry per update")
    parser.add_argument("--threads", type=int, default=10, help="all cores used, engine and PyTorch together")
    parser.add_argument("--device", default="cpu", help="cpu or mps")
    parser.add_argument("--snapshot-every", type=int, default=500_000, help="steps between checkpoints / pool snapshots")
    parser.add_argument("--resume", default=None, help="a checkpoint .zip to continue from")
    parser.add_argument("--name", default=None)
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument("--max-rounds", type=int, default=40)
    args = parser.parse_args()

    torch_threads = 2
    torch.set_num_threads(torch_threads)
    scenarios = default_pool(args.scenario) if args.scenario else None
    engine_threads = max(1, args.threads - torch_threads)
    if args.opponent == "self":
        env = SelfPlayVecEnv(
            scenarios, games=args.games, seed=args.seed, threads=engine_threads, max_rounds=args.max_rounds, mix=Mix()
        )
    else:
        env = ShipVecEnv(
            scenarios,
            games=args.games,
            opponent=args.opponent,
            seed=args.seed,
            threads=engine_threads,
            max_rounds=args.max_rounds,
        )
    name = args.name or f"{args.opponent}-{time.strftime('%Y%m%d-%H%M%S')}"
    folder = HERE / "checkpoints" / name
    print(f"{name}: {env.num_envs} learner ships in {args.games} games")
    if args.resume:
        model = MaskablePPO.load(args.resume, env=env, device=args.device)
    else:
        model = MaskablePPO(
            "MlpPolicy",
            env,
            n_steps=args.rollout,
            batch_size=4096,
            n_epochs=4,
            learning_rate=3e-4,
            gamma=0.99,
            gae_lambda=0.95,
            ent_coef=0.01,
            policy_kwargs={"net_arch": [512, 512, 256]},
            tensorboard_log=str(HERE / "runs"),
            device=args.device,
            seed=args.seed,
            verbose=0,
        )
    if isinstance(env, SelfPlayVecEnv):
        env.set_learner(model.policy)
    model.learn(
        total_timesteps=args.steps,
        callback=[Outcomes(env), Snapshots(env, args.snapshot_every, folder)],
        tb_log_name=name,
        progress_bar=True,
        reset_num_timesteps=not args.resume,
    )
    model.save(folder / "final")
    print(f"saved {folder / 'final.zip'}")


if __name__ == "__main__":
    main()
