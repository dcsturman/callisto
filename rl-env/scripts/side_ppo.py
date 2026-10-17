"""Train the per-side network (attention over every ship) with self-play.

    .venv/bin/python scripts/side_ppo.py --steps 20000000 --name side-1 --wandb

Steps are ship decisions, comparable with scripts/ppo.py's. Checkpoints are
saved as .pt (callisto_rl.sidenet.SideNet).
"""

import argparse
import time
from pathlib import Path

import torch
from rich.progress import BarColumn, Progress, TextColumn, TimeRemainingColumn
from torch.utils.tensorboard import SummaryWriter

from callisto_rl import default_pool
from callisto_rl.side_ppo import Settings, train
from callisto_rl.sidenet import SideNet

HERE = Path(__file__).resolve().parents[1]


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--init-from", default=None)
    p.add_argument("--steps", type=int, default=20_000_000)
    p.add_argument("--name", default=None)
    p.add_argument("--scenario", action="append")
    p.add_argument("--wandb", action="store_true")
    p.add_argument("--wandb-project", default="callisto-rl")
    defaults = Settings()
    for name, value in vars(defaults).items():
        if isinstance(value, (int, float, str)) and not isinstance(value, bool):
            p.add_argument("--" + name.replace("_", "-"), type=type(value), default=value)
    args = p.parse_args()
    s = Settings(
        **{k: getattr(args, k) for k in vars(defaults) if hasattr(args, k)},
        scenarios=default_pool(args.scenario) if args.scenario else None,
    )
    torch.set_num_threads(s.threads)
    torch.manual_seed(s.seed)
    name = args.name or f"side-{time.strftime('%Y%m%d-%H%M%S')}"
    folder = HERE / "checkpoints" / name
    folder.mkdir(parents=True, exist_ok=True)
    net = SideNet.load(args.init_from) if args.init_from else SideNet()
    run = None
    if args.wandb:
        import wandb

        run = wandb.init(project=args.wandb_project, name=name, config=vars(args), sync_tensorboard=True)
    writer = SummaryWriter(str(HERE / "runs" / name))

    def log(stats: dict, step: int) -> None:
        for key, value in stats.items():
            writer.add_scalar(key, value, step)

    def save(net: SideNet, step: int) -> None:
        state = {k: v.detach().cpu() for k, v in net.state_dict().items()}
        torch.save({"kind": "side", "config": net.config, "state": state, "decisions": step}, folder / f"side_{step}_decisions.pt")

    with Progress(TextColumn(name), BarColumn(), TextColumn("{task.completed:,}/{task.total:,}"), TimeRemainingColumn()) as bar:
        task = bar.add_task("train", total=args.steps)
        train(net, s, args.steps, log, save, progress=lambda done: bar.update(task, completed=done))
    net.to("cpu").save(folder / "final.pt", {"decisions": args.steps})
    writer.close()
    print(f"saved {folder / 'final.pt'}")
    if run is not None:
        run.finish()


if __name__ == "__main__":
    main()
