"""Train a policy with MaskablePPO against a baseline, or in self-play.

    .venv/bin/python scripts/train.py --opponent close --steps 2000000

Writes TensorBoard logs to runs/ (``.venv/bin/tensorboard --logdir runs``)
and checkpoints to checkpoints/. Engine and PyTorch threads together are
kept to --threads, so the machine stays usable.
"""

import argparse
import collections
import time
from pathlib import Path

import torch
from sb3_contrib import MaskablePPO
from stable_baselines3.common.callbacks import BaseCallback, CheckpointCallback

from callisto_rl import ShipVecEnv, default_pool

HERE = Path(__file__).resolve().parents[1]


class Outcomes(BaseCallback):
    """Log how the network's sides came out of each game: won, drove off,
    draw, fled, lost, by scenario."""

    def __init__(self, env: ShipVecEnv):
        super().__init__()
        self.env = env
        self.seen = 0
        self.window = collections.deque(maxlen=200)

    def _on_step(self) -> bool:
        for game in self.env.finished[self.seen :]:
            for side in (0, 1):
                if game["agent"][side]:
                    self.window.append((game["scenario"], outcome(game, side)))
        self.seen = len(self.env.finished)
        return True

    def _on_rollout_end(self) -> None:
        if not self.window:
            return
        counts = collections.Counter(result for _, result in self.window)
        for result in ("won", "drove off", "draw", "fled", "lost"):
            self.logger.record(f"outcome/{result.replace(' ', '_')}", counts[result] / len(self.window))
        by_scenario = collections.defaultdict(list)
        for scenario, result in self.window:
            by_scenario[scenario].append(result in ("won", "drove off"))
        for scenario, wins in by_scenario.items():
            self.logger.record(f"win_rate/{scenario}", sum(wins) / len(wins))


def outcome(game, side):
    from callisto_rl.vec_env import outcome_for

    return outcome_for(game, side)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--opponent", default="close", help="a baseline (close, hold, idle, random), or 'self'")
    parser.add_argument("--scenario", action="append", help="scenario file name; repeat; default the pool")
    parser.add_argument("--games", type=int, default=32)
    parser.add_argument("--steps", type=int, default=1_000_000, help="ship decisions to train on")
    parser.add_argument("--rollout", type=int, default=64, help="rounds per entry per update")
    parser.add_argument("--threads", type=int, default=10, help="all cores used, engine and PyTorch together")
    parser.add_argument("--device", default="cpu", help="cpu or mps")
    parser.add_argument("--name", default=None)
    parser.add_argument("--seed", type=int, default=0)
    args = parser.parse_args()

    torch_threads = 2
    torch.set_num_threads(torch_threads)
    env = ShipVecEnv(
        default_pool(args.scenario) if args.scenario else None,
        games=args.games,
        opponent=None if args.opponent == "self" else args.opponent,
        seed=args.seed,
        threads=max(1, args.threads - torch_threads),
    )
    name = args.name or f"{args.opponent}-{time.strftime('%Y%m%d-%H%M%S')}"
    print(f"{name}: {env.num_envs} ships in {args.games} games")
    model = MaskablePPO(
        "MlpPolicy",
        env,
        n_steps=args.rollout,
        batch_size=2048,
        n_epochs=4,
        learning_rate=3e-4,
        gamma=0.99,
        gae_lambda=0.95,
        ent_coef=0.01,
        policy_kwargs={"net_arch": [512, 512, 256]},
        tensorboard_log=str(HERE / "runs"),
        device=args.device,
        seed=args.seed,
        verbose=1,
    )
    checkpoints = CheckpointCallback(
        save_freq=max(1, 200_000 // env.num_envs), save_path=str(HERE / "checkpoints" / name), name_prefix="policy"
    )
    model.learn(total_timesteps=args.steps, callback=[Outcomes(env), checkpoints], tb_log_name=name)
    model.save(HERE / "checkpoints" / name / "final")


if __name__ == "__main__":
    main()
