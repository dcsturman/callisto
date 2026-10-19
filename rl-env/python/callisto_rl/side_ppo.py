"""PPO for the per-side network: one sample per side per round.

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
import concurrent.futures
import copy
import queue
import random
import threading
import time
from pathlib import Path
from dataclasses import dataclass, field
from typing import Any, Callable

import numpy as np
import torch

from ._engine import SideEngine
from .ppo import WINS
from .selfplay import Mix, Opponent
from .sidenet import SideNet
from .vec_env import DESIGNS, SIDE_POOL, default_pool, outcome_for

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
    # Reward per round for the share of the enemy's ships destroyed, and of
    # its firepower taken out, less the same for ours; off by default.
    kill_bonus: float = 0.0
    #: Reward per round to a holding side while it keeps a ship near where it
    #: started; 0 for none.
    hold_bonus: float = 0.0
    #: The scripted opponents baselines are drawn from, comma-separated: of
    #: close, hold, charger (rushes to its biggest guns' range), kiter (holds
    #: at its best edge, backing off), squadron, spread.
    baselines: str = "close,hold"
    firepower_bonus: float = 0.0
    #: How much each game's opening is varied, 0 for none: at 0.3 the fleets'
    #: distance apart changes by up to 30%, the second fleet shifts sideways,
    #: each formation turns by up to 27 degrees and each fleet drifts.
    random_starts: float = 0.0
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
    #: More teachers, of either kind: comma-separated `share=commander`, as
    #: "0.3=pt~:checkpoints/side-4/final.pt,0.05=net~:...selfplay-9.cpol".
    #: Each is drawn its share of games and logged as "teacher_<name>". A
    #: per-side network given as `pt~:<checkpoint>.pt` is played here, on
    #: the playing device, batched across games; any other name (`side~:`,
    #: `net~:`, a baseline) is played by the engine on the cpu, game by game.
    teachers: str = ""
    #: PPO's ratio, clipped against `clip`: per "ship", each ship's own choice
    #: against her old one, with the side's advantage; or per "side", the
    #: whole side's choice at once. A side's ratio is the product of its
    #: ships', so with twenty ships a small step for each is a huge one for
    #: the side, and learning blows up; per ship, as multi-agent PPO does, it
    #: does not.
    ratio: str = "ship"
    #: Leave a ship's move out of an update when the network learning from it
    #: gives it less than this fraction of the chance the player gave it. The
    #: player is one update behind, so now and then it makes a move the
    #: learner thinks next to impossible; PPO's ratio for such a move swings
    #: wildly, wrecking approx_kl and, for a move that went badly, pushing hard
    #: past the clip. 0 keeps every move.
    stale: float = 1e-3
    #: Let the learning rate fall in a straight line to this fraction of
    #: itself by the end of the run (1.0 keeps it constant).
    final_lr_fraction: float = 1.0
    #: Where learning runs: an NVIDIA GPU, else the Mac's, else the cpu.
    device: str = "cuda" if torch.cuda.is_available() else "mps" if torch.backends.mps.is_available() else "cpu"
    #: The games, split into this many groups of `games / groups`, each its
    #: own engine: while one group's engine steps, the next group's orders are
    #: chosen, so the engine and the network overlap. 1 is the old way.
    groups: int = 1
    #: Where the player chooses orders, for the network, its snapshots and
    #: Python-played teachers. Empty means the learning device on an NVIDIA
    #: GPU, and the cpu on a Mac: the Mac's GPU is slower than its cpu for the
    #: player's small batches while it is also learning (measured: 4,574
    #: decisions a second choosing on the cpu, 3,907 on the GPU).
    play_device: str = ""


def teachers(spec: str) -> list[tuple[float, str, str]]:
    """The `teachers` setting as (share, commander name, short name). The
    short name is the file's, or its folder's for a run's `final`."""
    out = []
    for entry in filter(None, (e.strip() for e in spec.split(","))):
        share, commander = entry.split("=", 1)
        path = Path(commander.split(":", 1)[-1].split("#")[0])
        out.append((float(share), commander, path.parent.name if path.stem == "final" else path.stem))
    if sum(share for share, _, _ in out) > 1.0:
        raise ValueError(f"teachers' shares add up to more than 1: {spec}")
    return out


class Group:
    """One engine's games, as the player runs them: whose side the learner
    plays in each, who the opponent is, the views last observed, and this
    batch's records so far."""

    def __init__(self, eng: SideEngine, scenarios: list[str]):
        self.eng = eng
        self.games = eng.num_games
        self.learner_side = np.array([(g // len(scenarios)) % 2 for g in range(self.games)])
        self.learner_rows = 2 * np.arange(self.games) + self.learner_side
        self.opponent_rows = 2 * np.arange(self.games) + (1 - self.learner_side)
        self.opponents: list[Opponent] = [Opponent("self")] * self.games
        self.views: Any = None
        self.done = np.zeros(self.games, dtype=bool)
        self.store: dict[str, list] = {}
        self.stepping: Any = None


class Player(threading.Thread):
    def __init__(self, settings: Settings, learner: SideNet, out: queue.Queue):
        super().__init__(daemon=True)
        self.s = settings
        self.device = settings.play_device or ("cpu" if settings.device == "mps" else settings.device)
        self.copy = copy.deepcopy(learner).to(self.device).eval()
        # Teachers played here rather than by the engine, by commander name.
        self.played_here: dict[str, SideNet] = {
            commander: SideNet.load(commander.split(":", 1)[1], self.device).eval()
            for _, commander, _ in teachers(settings.teachers)
            if commander.startswith("pt~:")
        }
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
        for share, commander, name in teachers(self.s.teachers):
            if roll < share:
                return Opponent("teacher", commander, name)
            roll -= share
        roll = self.rng.random()
        if roll < self.baseline_share:
            return Opponent("baseline", self.rng.choice([n.strip() for n in self.s.baselines.split(",") if n.strip()]))
        if roll < self.baseline_share + mix.past and self.snapshots:
            return Opponent("past", self.rng.randrange(len(self.snapshots)))
        return Opponent("self")

    def _group(self, scenarios: list[str], k: int) -> "Group":
        s = self.s
        eng = SideEngine(
            scenarios, str(DESIGNS), s.games // max(1, s.groups), max_rounds=s.max_rounds, seed=s.seed + 1_000_003 * k,
            threads=s.threads, random_goals=s.random_goals, destroy_goals=s.destroy_goals,
            kill_bonus=s.kill_bonus, firepower_bonus=s.firepower_bonus, random_starts=s.random_starts,
            hold_bonus=s.hold_bonus,
        )
        group = Group(eng, scenarios)
        for g in range(group.games):
            self._seat(group, g)
        group.views = eng.observe()
        return group

    def _seat(self, group: "Group", game: int) -> None:
        opponent = group.opponents[game] = self.draw()
        by_engine = opponent.kind in ("baseline", "teacher") and opponent.which not in self.played_here
        group.eng.stand_in(game, int(1 - group.learner_side[game]), opponent.which if by_engine else None)

    def _choose(self, group: "Group") -> np.ndarray:
        """The orders for every side of the group's games this round, and its
        learner entries recorded."""
        tensors = [torch.as_tensor(v) for v in group.views]
        actions = np.zeros((2 * group.games, group.views[4].shape[1], 15), dtype=np.int64)
        # Every view to the playing device in one transfer.
        on_device = [t.to(self.device) for t in tensors]

        def play(net: SideNet, rows: np.ndarray, per_ship: bool = False):
            index = torch.as_tensor(rows, device=self.device)
            with torch.no_grad():
                return net.act(*[t[index] for t in on_device], per_ship=per_ship)

        opponents, games = group.opponents, group.games
        # The network now: the learner's sides and the "self" opponents in one pass.
        selfish = [group.opponent_rows[g] for g in range(games) if opponents[g].kind == "self"]
        rows = np.concatenate([group.learner_rows, np.array(selfish, dtype=np.int64)])
        a, logp, value = (x.cpu() for x in play(self.copy, rows, per_ship=True))
        actions[rows] = a.numpy()
        for key in sorted({o.which for o in opponents if o.kind == "past"}):
            past_rows = np.array([group.opponent_rows[g] for g in range(games) if opponents[g].kind == "past" and opponents[g].which == key])
            actions[past_rows] = play(self.snapshots[key], past_rows)[0].cpu().numpy()
        for commander, teacher in self.played_here.items():
            taught = np.array([group.opponent_rows[g] for g in range(games) if opponents[g].kind == "teacher" and opponents[g].which == commander], dtype=np.int64)
            if len(taught):
                actions[taught] = play(teacher, taught)[0].cpu().numpy()
        group.store["views"].append([t[group.learner_rows] for t in tensors])
        group.store["actions"].append(a[:games])
        group.store["log_probs"].append(logp[:games])
        group.store["values"].append(value[:games])
        group.store["dones"].append(torch.as_tensor(group.done))
        return actions

    def _finish(self, group: "Group", results: list[dict[str, Any]]) -> None:
        """Take in the result of the group's engine step."""
        views, rewards, dones, finished = group.stepping
        group.stepping = None
        group.views = views
        group.store["rewards"].append(torch.as_tensor(rewards[group.learner_rows]))
        group.done = dones[group.learner_rows]
        for game in finished:
            g = game["game"]
            side = int(group.learner_side[g])
            results.append({
                "scenario": game["scenario"], "side": game["sides"][side], "goal": game["goals"][side],
                "opponent": group.opponents[g].label, "outcome": outcome_for(game, side),
                "return": float(game["returns"][side]), "rounds": int(game["rounds"]),
                "at_post": bool(game["at_post"][side]),
            })
            self._seat(group, g)

    def _run(self) -> None:
        s = self.s
        scenarios = list(s.scenarios or default_pool(SIDE_POOL))
        groups = [self._group(scenarios, k) for k in range(max(1, s.groups))]
        # While one group's engine steps on this thread (it lets go of
        # Python's lock), the next group's orders are chosen on a helper
        # thread, so the engine and the network overlap. The engine has to
        # stay on this thread; the network does not.
        chooser = concurrent.futures.ThreadPoolExecutor(max_workers=1)
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
            for group in groups:
                group.store = {k: [] for k in ("views", "actions", "log_probs", "values", "rewards", "dones")}
            results: list[dict[str, Any]] = []
            # Seconds this batch spent waiting for orders, and stepping engines.
            timing = {"net": 0.0, "engine": 0.0}
            choosing: dict[int, concurrent.futures.Future] = {}
            turns = [(t, k) for t in range(s.rollout) for k in range(len(groups))]
            for at, (_, k) in enumerate(turns):
                group = groups[k]
                clock = time.perf_counter()
                actions = choosing.pop(k).result() if k in choosing else self._choose(group)
                timing["net"] += time.perf_counter() - clock
                # The next group to step, if it has a turn left this batch, has
                # its views already: choose for it while this one steps.
                if at + 1 < len(turns):
                    after = turns[at + 1][1]
                    if after != k and after not in choosing:
                        choosing[after] = chooser.submit(self._choose, groups[after])
                clock = time.perf_counter()
                group.stepping = group.eng.step(actions)
                timing["engine"] += time.perf_counter() - clock
                self._finish(group, results)
            learner_last = [torch.cat([torch.as_tensor(g.views[i])[g.learner_rows] for g in groups]) for i in range(VIEWS)]
            with torch.no_grad():
                last_value = self.copy(*[t.to(self.device) for t in learner_last])[1].cpu()

            def joined(key: str):
                return torch.stack([torch.cat([g.store[key][t] for g in groups]) for t in range(s.rollout)])

            batch = {
                "views": [
                    torch.stack([torch.cat([g.store["views"][t][i] for g in groups]) for t in range(s.rollout)])
                    for i in range(VIEWS)
                ],
                "actions": joined("actions"),
                "log_probs": joined("log_probs"),
                "values": joined("values"),
                "rewards": joined("rewards"),
                "dones": joined("dones"),
                "last_value": last_value,
                "last_views": learner_last,
                "last_done": torch.cat([torch.as_tensor(g.done) for g in groups]),
                "results": results,
                "steps": s.rollout * sum(g.games for g in groups),
                "timing": timing,
            }
            clock = time.perf_counter()
            self.out.put(batch)
            timing["handover"] = time.perf_counter() - clock


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
        """Score the batch with the network as it is now, and mark the moves
        it finds stale: far less likely now than when they were played."""
        steps, n = batch["rewards"].shape
        played = batch["log_probs"]
        flat = [v.reshape(steps * n, *v.shape[2:]) for v in batch["views"]]
        actions = batch["actions"].reshape(steps * n, *batch["actions"].shape[2:])
        lps, vals = [], []
        d = self.device
        for start in range(0, steps * n, self.s.batch_size):
            part = slice(start, start + self.s.batch_size)
            lp, _, v = self.net.evaluate(*[f[part].to(d) for f in flat], actions[part].to(d), per_ship=True)
            lps.append(lp.cpu())
            vals.append(v.cpu())
        batch["log_probs"] = torch.cat(lps).view(steps, n, -1)
        if self.s.stale > 0 and played.dim() == 3:
            batch["fresh"] = (batch["log_probs"] - played) > float(np.log(self.s.stale))
        else:
            batch["fresh"] = torch.ones_like(batch["log_probs"], dtype=torch.bool)
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
        old_lp, old_v = batch["log_probs"].reshape(n, -1).to(d), values.reshape(n).to(d)
        # The ships each entry has, and whose moves are not stale, for
        # averaging over ships.
        present = (flat[0][..., 0] > 0.5).float()
        stale_share = 1.0 - float((batch["fresh"].reshape(n, -1).float().to(d) * present).sum() / present.sum().clamp(min=1.0))
        present = present * batch["fresh"].reshape(n, -1).float().to(d)
        per_ship = s.ratio == "ship"
        if not per_ship:
            old_lp = old_lp.sum(1, keepdim=True)
        adv, returns = adv.reshape(n).to(d), returns.reshape(n).to(d)
        stats = collections.defaultdict(list)
        self.net.train()
        for _ in range(s.epochs):
            order = torch.randperm(n, device=d)
            for start in range(0, n, s.batch_size):
                idx = order[start : start + s.batch_size]
                lp, entropy, value = self.net.evaluate(*[f[idx] for f in flat], actions[idx], per_ship=True)
                a = adv[idx]
                a = ((a - a.mean()) / (a.std() + 1e-8)).unsqueeze(1)
                if per_ship:
                    weight = present[idx]
                else:
                    lp, weight = lp.sum(1, keepdim=True), torch.ones_like(a)
                count = weight.sum().clamp(min=1.0)
                ratio = torch.exp(lp - old_lp[idx])
                pg = -(torch.min(ratio * a, torch.clamp(ratio, 1 - s.clip, 1 + s.clip) * a) * weight).sum() / count
                vl = (returns[idx] - value).pow(2).mean()
                # Per ship too: summed over a side, twenty ships' entropy
                # outweighs everything else the loss says.
                el = -(entropy * weight).sum() / count if per_ship else -entropy.sum(1).mean()
                loss = pg + s.entropy * el + s.value * vl
                self.optimizer.zero_grad()
                loss.backward()
                torch.nn.utils.clip_grad_norm_(self.net.parameters(), s.max_grad_norm)
                self.optimizer.step()
                with torch.no_grad():
                    lr = lp - old_lp[idx]
                    stats["approx_kl"].append((((torch.exp(lr) - 1) - lr) * weight).sum().item() / count.item())
                    stats["clip_fraction"].append(((((ratio - 1).abs() > s.clip).float()) * weight).sum().item() / count.item())
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
        out["train/stale_share"] = stale_share
        return out

    def record(self, batch: dict) -> dict[str, float]:
        self.recent.extend(batch["results"])
        out: dict[str, float] = {}
        by_opponent = collections.defaultdict(list)
        by_side = collections.defaultdict(list)
        by_goal = collections.defaultdict(list)
        # Whether Hold games ended with the side still at its post, by
        # scenario: a Hold side that runs can still show a positive return
        # from the fighting on the way, so goal/hold alone does not say.
        held = collections.defaultdict(list)
        for r in self.recent:
            if r["goal"] == "hold":
                held["hold/at_post"].append(r["at_post"])
                held[f"hold/at_post/{r['scenario']}"].append(r["at_post"])
            by_opponent[r["opponent"]].append(r["outcome"])
            if r["opponent"] not in ("self", "past") and not r["opponent"].startswith("teacher"):
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
        for key, at_post in held.items():
            out[key] = sum(at_post) / len(at_post)
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
            done_share = min(1.0, learner.decisions / max(1, total_steps))
            rate = s.learning_rate * (1.0 - (1.0 - s.final_lr_fraction) * done_share)
            for group in learner.optimizer.param_groups:
                group["lr"] = rate
            clock = time.perf_counter()
            batch = batches.get()
            if batch is None:
                raise RuntimeError("the player stopped") from player.error
            waited = time.perf_counter() - clock
            clock = time.perf_counter()
            stats = learner.update(batch)
            # Where the time goes, in seconds per batch: the player choosing
            # orders and stepping the engine (the engine includes baselines and
            # teachers, which it plays), the learner learning, and the learner
            # waiting for the player. The player's wait to hand a batch over is
            # the previous batch's, as it is measured after the hand-over.
            stats["time/play_net"] = batch["timing"]["net"]
            stats["time/play_engine"] = batch["timing"]["engine"]
            stats["time/learn"] = time.perf_counter() - clock
            stats["time/learn_waiting"] = waited
            player.refresh({k: v.detach().to("cpu").clone() for k, v in net.state_dict().items()})
            stats.update(learner.record(batch))
            done = min(1.0, learner.decisions / max(1, s.taper_steps))
            player.baseline_share = s.baselines_start + (s.baselines_end - s.baselines_start) * done
            stats["pool/baseline_share"] = player.baseline_share
            stats["train/learning_rate"] = rate
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
