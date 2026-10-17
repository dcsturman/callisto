"""PPO for the side network: one sample per side per round.

As :mod:`callisto_rl.ppo` -- one thread plays while the other learns, and
every batch is re-scored with the current network before learning -- but an
entry is a whole side: its view is the token set, its choice every ship's
orders, its log-probability the sum over ships and heads.

In each game the learner plays one side (alternating between game slots)
and the other is drawn from the pool: the network as it is now, a snapshot
of an earlier one, or a baseline the engine plays.
"""

from __future__ import annotations

import collections
import copy
import queue
import random
import threading
import time
from dataclasses import dataclass, field
from typing import Any, Callable

import numpy as np
import torch

from ._engine import SideEngine
from .ppo import WINS
from .selfplay import Mix, Opponent
from .sidenet import SideNet
from .vec_env import DESIGNS, default_pool, outcome_for

VIEWS = 5  # own, foes, side, pairs, masks


@dataclass
class Settings:
    games: int = 128
    rollout: int = 32
    batch_size: int = 1024
    epochs: int = 4
    learning_rate: float = 3e-4
    gamma: float = 0.99
    gae_lambda: float = 0.95
    clip: float = 0.2
    entropy: float = 0.01
    value: float = 0.5
    max_grad_norm: float = 0.5
    max_rounds: int = 40
    threads: int = 10
    seed: int = 0
    destroy_goals: float = 0.7
    random_goals: float = 0.15
    baselines_start: float = 0.3
    baselines_end: float = 0.15
    taper_steps: int = 10_000_000
    snapshot_every: int = 1_000_000
    max_snapshots: int = 20
    scenarios: list[str] | None = None
    mix: Mix = field(default_factory=Mix)
    #: A fixed, strong opponent: a per-ship network's exported file, played by
    #: the engine in Rust. Logged as "teacher".
    teacher: str = ""
    teacher_share: float = 0.0
    #: Where learning runs. Playing stays on the cpu, in small batches.
    device: str = "mps" if torch.backends.mps.is_available() else "cpu"


class Player(threading.Thread):
    def __init__(self, settings: Settings, learner: SideNet, out: queue.Queue):
        super().__init__(daemon=True)
        self.s = settings
        self.copy = copy.deepcopy(learner).to("cpu").eval()
        self.lock = threading.Lock()
        self.pending: dict | None = None
        self.out = out
        self.stop = threading.Event()
        self.want_snapshot = threading.Event()
        self.baseline_share = settings.baselines_start
        self.snapshots: list[SideNet] = []
        self.error: BaseException | None = None
        self.rng = random.Random(settings.seed)

    def refresh(self, state: dict) -> None:
        with self.lock:
            self.pending = state

    def run(self) -> None:
        try:
            self._run()
        except BaseException as error:  # noqa: BLE001 -- handed to the learner
            self.error = error
            self.out.put(None)

    def draw(self) -> Opponent:
        mix = self.s.mix
        if self.s.teacher and self.rng.random() < self.s.teacher_share:
            return Opponent("teacher", f"net~:{self.s.teacher}")
        roll = self.rng.random()
        if roll < self.baseline_share:
            return Opponent("baseline", self.rng.choice(mix.baseline_names))
        if roll < self.baseline_share + mix.past and self.snapshots:
            return Opponent("past", self.rng.randrange(len(self.snapshots)))
        return Opponent("self")

    def _run(self) -> None:
        s = self.s
        scenarios = list(s.scenarios or default_pool())
        eng = SideEngine(
            scenarios, str(DESIGNS), s.games, max_rounds=s.max_rounds, seed=s.seed, threads=s.threads,
            random_goals=s.random_goals, destroy_goals=s.destroy_goals,
        )
        games = eng.num_games
        learner_side = np.array([(g // len(scenarios)) % 2 for g in range(games)])
        learner_rows = 2 * np.arange(games) + learner_side
        opponent_rows = 2 * np.arange(games) + (1 - learner_side)
        opponents: list[Opponent] = [Opponent("self")] * games

        def seat(game: int) -> None:
            opponents[game] = self.draw()
            played_by_engine = opponents[game].kind in ("baseline", "teacher")
            eng.stand_in(game, int(1 - learner_side[game]), opponents[game].which if played_by_engine else None)

        for g in range(games):
            seat(g)
        views = eng.observe()
        done = np.zeros(games, dtype=bool)
        while not self.stop.is_set():
            with self.lock:
                if self.pending is not None:
                    self.copy.load_state_dict(self.pending)
                    self.pending = None
            if self.want_snapshot.is_set():
                frozen = copy.deepcopy(self.copy).eval()
                self.snapshots.append(frozen)
                if len(self.snapshots) > s.max_snapshots:
                    del self.snapshots[1]
                self.want_snapshot.clear()
            store = {k: [] for k in ("views", "actions", "log_probs", "values", "rewards", "dones")}
            results: list[dict[str, Any]] = []
            for _ in range(s.rollout):
                tensors = [torch.as_tensor(v) for v in views]
                actions = np.zeros((2 * games, views[4].shape[1], 15), dtype=np.int64)
                # The network now: the learner's sides and the "self" opponents in one pass.
                selfish = [opponent_rows[g] for g in range(games) if opponents[g].kind == "self"]
                rows = np.concatenate([learner_rows, np.array(selfish, dtype=np.int64)])
                with torch.no_grad():
                    a, logp, value = self.copy.act(*[t[rows] for t in tensors])
                actions[rows] = a.numpy()
                n = games
                for key in sorted({o.which for o in opponents if o.kind == "past"}):
                    past_rows = np.array([opponent_rows[g] for g in range(games) if opponents[g].kind == "past" and opponents[g].which == key])
                    with torch.no_grad():
                        pa, _, _ = self.snapshots[key].act(*[t[past_rows] for t in tensors])
                    actions[past_rows] = pa.numpy()
                store["views"].append([t[learner_rows] for t in tensors])
                store["actions"].append(a[:n])
                store["log_probs"].append(logp[:n])
                store["values"].append(value[:n])
                store["dones"].append(torch.as_tensor(done))
                views, rewards, dones, finished = eng.step(actions)
                store["rewards"].append(torch.as_tensor(rewards[learner_rows]))
                done = dones[learner_rows]
                for game in finished:
                    g = game["game"]
                    side = int(learner_side[g])
                    results.append({
                        "scenario": game["scenario"], "side": game["sides"][side], "goal": game["goals"][side],
                        "opponent": opponents[g].label, "outcome": outcome_for(game, side),
                        "return": float(game["returns"][side]), "rounds": int(game["rounds"]),
                    })
                    seat(g)
            with torch.no_grad():
                last = [torch.as_tensor(v)[learner_rows] for v in views]
                last_value = self.copy(*last)[1]
            batch = {
                "views": [torch.stack([step[i] for step in store["views"]]) for i in range(VIEWS)],
                "actions": torch.stack(store["actions"]),
                "log_probs": torch.stack(store["log_probs"]),
                "values": torch.stack(store["values"]),
                "rewards": torch.stack(store["rewards"]),
                "dones": torch.stack(store["dones"]),
                "last_value": last_value,
                "last_views": last,
                "last_done": torch.as_tensor(done),
                "results": results,
                "steps": s.rollout * games,
            }
            self.out.put(batch)


class Learner:
    def __init__(self, net: SideNet, s: Settings):
        self.net, self.s = net.to(s.device), s
        self.device = s.device
        self.optimizer = torch.optim.Adam(net.parameters(), lr=s.learning_rate, eps=1e-5)
        self.steps = 0
        self.decisions = 0
        self.recent: collections.deque = collections.deque(maxlen=1200)

    @torch.no_grad()
    def rescore(self, batch: dict) -> None:
        steps, n = batch["rewards"].shape
        flat = [v.reshape(steps * n, *v.shape[2:]) for v in batch["views"]]
        actions = batch["actions"].reshape(steps * n, *batch["actions"].shape[2:])
        lps, vals = [], []
        d = self.device
        for start in range(0, steps * n, self.s.batch_size):
            part = slice(start, start + self.s.batch_size)
            lp, _, v = self.net.evaluate(*[f[part].to(d) for f in flat], actions[part].to(d))
            lps.append(lp.cpu())
            vals.append(v.cpu())
        batch["log_probs"] = torch.cat(lps).view(steps, n)
        batch["values"] = torch.cat(vals).view(steps, n)
        batch["last_value"] = self.net(*[v.to(d) for v in batch["last_views"]])[1].cpu()

    def update(self, batch: dict) -> dict[str, float]:
        s = self.s
        self.net.eval()
        self.rescore(batch)
        rewards, values, dones = batch["rewards"], batch["values"], batch["dones"]
        steps = rewards.shape[0]
        adv = torch.zeros_like(rewards)
        last = torch.zeros_like(batch["last_value"])
        for t in reversed(range(steps)):
            if t == steps - 1:
                alive, next_value = 1.0 - batch["last_done"].float(), batch["last_value"]
            else:
                alive, next_value = 1.0 - dones[t + 1].float(), values[t + 1]
            delta = rewards[t] + s.gamma * next_value * alive - values[t]
            last = delta + s.gamma * s.gae_lambda * alive * last
            adv[t] = last
        returns = adv + values
        n = steps * rewards.shape[1]
        d = self.device
        flat = [v.reshape(n, *v.shape[2:]).to(d) for v in batch["views"]]
        actions = batch["actions"].reshape(n, *batch["actions"].shape[2:]).to(d)
        old_lp, old_v = batch["log_probs"].reshape(n).to(d), values.reshape(n).to(d)
        adv, returns = adv.reshape(n).to(d), returns.reshape(n).to(d)
        stats = collections.defaultdict(list)
        self.net.train()
        for _ in range(s.epochs):
            order = torch.randperm(n, device=d)
            for start in range(0, n, s.batch_size):
                idx = order[start : start + s.batch_size]
                lp, entropy, value = self.net.evaluate(*[f[idx] for f in flat], actions[idx])
                a = adv[idx]
                a = (a - a.mean()) / (a.std() + 1e-8)
                ratio = torch.exp(lp - old_lp[idx])
                pg = -torch.min(ratio * a, torch.clamp(ratio, 1 - s.clip, 1 + s.clip) * a).mean()
                vl = (returns[idx] - value).pow(2).mean()
                el = -entropy.mean()
                loss = pg + s.entropy * el + s.value * vl
                self.optimizer.zero_grad()
                loss.backward()
                torch.nn.utils.clip_grad_norm_(self.net.parameters(), s.max_grad_norm)
                self.optimizer.step()
                with torch.no_grad():
                    lr = lp - old_lp[idx]
                    stats["approx_kl"].append(((torch.exp(lr) - 1) - lr).mean().item())
                    stats["clip_fraction"].append(((ratio - 1).abs() > s.clip).float().mean().item())
                stats["policy_gradient_loss"].append(pg.item())
                stats["value_loss"].append(vl.item())
                stats["entropy_loss"].append(el.item())
        variance = returns.var()
        explained = 1 - (returns - old_v).var() / variance if variance > 0 else torch.tensor(0.0)
        self.steps += batch["steps"]
        # Ship decisions, comparable with the per-ship network's steps.
        self.decisions += int((batch["views"][0][..., 0] > 0.5).sum())
        out = {f"train/{k}": float(np.mean(v)) for k, v in stats.items()}
        out["train/explained_variance"] = float(explained)
        return out

    def record(self, batch: dict) -> dict[str, float]:
        self.recent.extend(batch["results"])
        out: dict[str, float] = {}
        by_opponent = collections.defaultdict(list)
        by_side = collections.defaultdict(list)
        by_goal = collections.defaultdict(list)
        for r in self.recent:
            by_opponent[r["opponent"]].append(r["outcome"])
            if r["opponent"] not in ("self", "past", "teacher"):
                by_side[f"{r['scenario']}/{r['side']}"].append(r["outcome"] in WINS)
            by_goal[r["goal"]].append(r["return"] > 0)
        for opponent, outcomes in by_opponent.items():
            counts = collections.Counter(outcomes)
            for outcome in ("won", "drove off", "draw", "fled", "lost"):
                out[f"vs_{opponent}/{outcome.replace(' ', '_')}"] = counts[outcome] / len(outcomes)
            out[f"vs_{opponent}/win_rate"] = sum(counts[w] for w in WINS) / len(outcomes)
        for key, wins in by_side.items():
            out[f"win_rate/{key}"] = sum(wins) / len(wins)
        for goal, met in by_goal.items():
            out[f"goal/{goal}"] = sum(met) / len(met)
        return out


def train(net: SideNet, s: Settings, total_steps: int, log: Callable, save: Callable, progress: Callable | None = None):
    batches: queue.Queue = queue.Queue(maxsize=1)
    player = Player(s, net, batches)
    learner = Learner(net, s)
    player.start()
    next_snapshot = s.snapshot_every
    started = time.perf_counter()
    try:
        while learner.decisions < total_steps:
            batch = batches.get()
            if batch is None:
                raise RuntimeError("the player stopped") from player.error
            stats = learner.update(batch)
            player.refresh({k: v.detach().to("cpu").clone() for k, v in net.state_dict().items()})
            stats.update(learner.record(batch))
            done = min(1.0, learner.decisions / max(1, s.taper_steps))
            player.baseline_share = s.baselines_start + (s.baselines_end - s.baselines_start) * done
            stats["pool/baseline_share"] = player.baseline_share
            stats["pool/snapshots"] = len(player.snapshots)
            stats["time/fps"] = learner.decisions / (time.perf_counter() - started)
            if learner.decisions >= next_snapshot:
                next_snapshot += s.snapshot_every
                save(net, learner.decisions)
                player.want_snapshot.set()
            log(stats, learner.decisions)
            if progress:
                progress(learner.decisions)
    finally:
        player.stop.set()
        try:
            batches.get(timeout=60)
        except queue.Empty:
            pass
    return net
