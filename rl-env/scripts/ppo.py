"""Train with callisto_rl.ppo: playing and learning overlapped.

    .venv/bin/python scripts/ppo.py --init-from checkpoints/selfplay-7/policy_5000000_steps.zip --steps 30000000

Self-play against a pool (the network now, past snapshots, the baselines),
goals mostly destroy (holders hold), as scripts/train.py; checkpoints are
saved as .pt (callisto_rl.policy.Policy), and scripts/export.py and the
arena read them as they read Stable-Baselines3's .zip.
"""

import argparse
import time
from pathlib import Path

import torch
from rich.progress import BarColumn, Progress, TextColumn, TimeRemainingColumn
from torch.utils.tensorboard import SummaryWriter

from callisto_rl import default_pool
from callisto_rl.policy import Policy
from callisto_rl.ppo import Settings, train

HERE = Path(__file__).resolve().parents[1]


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--init-from", default=None, help="a checkpoint (.zip or .pt) to start from")
    p.add_argument("--steps", type=int, default=30_000_000)
    p.add_argument("--name", default=None)
    p.add_argument("--scenario", action="append")
    p.add_argument("--wandb", action="store_true")
    p.add_argument("--wandb-project", default="callisto-rl")
    p.add_argument("--device", default="cpu", help="cpu, mps or cuda: where learning runs; playing stays on the cpu")
    defaults = Settings()
    for name, value in vars(defaults).items():
        if isinstance(value, (int, float)) and not isinstance(value, bool):
            p.add_argument("--" + name.replace("_", "-"), type=type(value), default=value)
    args = p.parse_args()

    settings = Settings(
        **{k: getattr(args, k) for k in vars(defaults) if hasattr(args, k)},
        scenarios=default_pool(args.scenario) if args.scenario else None,
    )
    torch.set_num_threads(settings.threads)
    torch.manual_seed(settings.seed)
    name = args.name or f"ppo-{time.strftime('%Y%m%d-%H%M%S')}"
    folder = HERE / "checkpoints" / name
    folder.mkdir(parents=True, exist_ok=True)
    policy = Policy.from_any(args.init_from) if args.init_from else Policy()
    if args.device != "cpu":
        raise SystemExit("learning on a GPU: the player's copy must stay on the cpu; not wired yet")

    run = None
    if args.wandb:
        import wandb

        run = wandb.init(project=args.wandb_project, name=name, config=vars(args), sync_tensorboard=True)
    writer = SummaryWriter(str(HERE / "runs" / name))

    def log(stats: dict, step: int) -> None:
        for key, value in stats.items():
            writer.add_scalar(key, value, step)

    def save(policy: Policy, step: int) -> None:
        policy.save(folder / f"policy_{step}_steps.pt", {"steps": step})

    with Progress(TextColumn(name), BarColumn(), TextColumn("{task.completed:,}/{task.total:,}"), TimeRemainingColumn()) as bar:
        task = bar.add_task("train", total=args.steps)
        train(policy, settings, args.steps, log, save, progress=lambda done: bar.update(task, completed=done))
    policy.save(folder / "final.pt", {"steps": args.steps})
    writer.close()
    print(f"saved {folder / 'final.pt'}")
    if run is not None:
        run.finish()


if __name__ == "__main__":
    main()
