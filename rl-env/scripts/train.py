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
        # How often each goal was met, against every opponent: a win, or for
        # a raid, hold or escape, the outcome that goal counts as success.
        by_goal = collections.defaultdict(list)
        for result in self.recent:
            by_goal[result.get("goal", "destroy")].append(result["return"] > 0)
        for goal, met in by_goal.items():
            self.logger.record(f"goal/{goal}", sum(met) / len(met))


class Taper(BaseCallback):
    """In self-play, start with more games against the baselines and taper to
    the usual share: the baselines punish waiting, which self-play alone can
    drift into."""

    def __init__(self, env, start: float, end: float, steps: int):
        super().__init__()
        self.env, self.start, self.end, self.steps = env, start, end, max(1, steps)

    def _on_rollout_start(self) -> None:
        done = min(1.0, self.num_timesteps / self.steps)
        share = self.start + (self.end - self.start) * done
        mix = self.env.mix
        mix.current = max(0.0, 1.0 - mix.past - share)
        mix.baselines = share
        self.logger.record("pool/baseline_share", share)

    def _on_step(self) -> bool:
        return True


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
    parser.add_argument("--resume", default=None, help="a checkpoint .zip to continue, step count and all")
    parser.add_argument(
        "--init-from", default=None, help="a checkpoint .zip whose network to start from, as a new run"
    )
    parser.add_argument("--baselines-start", type=float, default=0.5, help="self-play: share of baseline games at first")
    parser.add_argument("--baselines-end", type=float, default=0.15, help="self-play: share once tapered")
    parser.add_argument("--taper-steps", type=int, default=10_000_000, help="self-play: steps to taper over")
    parser.add_argument("--name", default=None)
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument("--max-rounds", type=int, default=40)
    parser.add_argument(
        "--destroy-goals",
        type=float,
        default=0.7,
        help="share of games where every side's goal is destroy (a side meant to hold keeps hold)",
    )
    parser.add_argument(
        "--random-goals", type=float, default=0.15, help="share of games where each side's goal is drawn at random"
    )
    parser.add_argument(
        "--wandb", action="store_true", help="also send the run to Weights & Biases (after `.venv/bin/wandb login`)"
    )
    parser.add_argument("--wandb-project", default="callisto-rl")
    args = parser.parse_args()

    torch_threads = 2
    torch.set_num_threads(torch_threads)
    scenarios = default_pool(args.scenario) if args.scenario else None
    engine_threads = max(1, args.threads - torch_threads)
    if args.opponent == "self":
        env = SelfPlayVecEnv(
            scenarios,
            games=args.games,
            seed=args.seed,
            threads=engine_threads,
            max_rounds=args.max_rounds,
            mix=Mix(),
            random_goals=args.random_goals,
            destroy_goals=args.destroy_goals,
        )
    else:
        env = ShipVecEnv(
            scenarios,
            games=args.games,
            opponent=args.opponent,
            seed=args.seed,
            threads=engine_threads,
            max_rounds=args.max_rounds,
            random_goals=args.random_goals,
            destroy_goals=args.destroy_goals,
        )
    name = args.name or f"{args.opponent}-{time.strftime('%Y%m%d-%H%M%S')}"
    folder = HERE / "checkpoints" / name
    run = None
    if args.wandb:
        import wandb

        # Copies the TensorBoard logs as they are written, so both work.
        settings = vars(args) | {"mix": vars(Mix())} if args.opponent == "self" else vars(args)
        run = wandb.init(project=args.wandb_project, name=name, config=settings, sync_tensorboard=True)
    print(f"{name}: {env.num_envs} learner ships in {args.games} games")
    if args.resume or args.init_from:
        model = MaskablePPO.load(
            args.resume or args.init_from, env=env, device=args.device, tensorboard_log=str(HERE / "runs")
        )
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
    callbacks = [Outcomes(env), Snapshots(env, args.snapshot_every, folder)]
    if isinstance(env, SelfPlayVecEnv):
        callbacks.append(Taper(env, args.baselines_start, args.baselines_end, args.taper_steps))
    model.learn(
        total_timesteps=args.steps,
        callback=callbacks,
        tb_log_name=name,
        progress_bar=True,
        reset_num_timesteps=not args.resume,
    )
    model.save(folder / "final")
    print(f"saved {folder / 'final.zip'}")
    if run is not None:
        run.finish()


if __name__ == "__main__":
    main()
