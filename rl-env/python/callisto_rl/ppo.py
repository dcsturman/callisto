"""PPO for callisto, with playing and learning overlapped.

One thread plays: it steps the games with a copy of the network, picks the
opponents' moves, and hands each finished batch of experience over. The
other learns from that batch while the next is being played. The playing
copy is refreshed from the learner after every update, so the experience is
at most one update stale -- which PPO tolerates, since every step records
the probability its own copy gave the choice it made.

The algorithm is standard clipped PPO with generalised advantage estimates,
as Stable-Baselines3 runs it (advantages normalised per minibatch, gradients
clipped at 0.5), so a network trained there continues here.
"""

from __future__ import annotations

import collections
import copy
import queue
import threading
import time
from dataclasses import dataclass, field
from typing import Any, Callable

import numpy as np
import torch

from .policy import Policy
from .selfplay import Mix, SelfPlayVecEnv

WINS = ("won", "drove off")


@dataclass
class Settings:
    games: int = 128
    rollout: int = 64
    batch_size: int = 16384
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
    scenarios: list[str] | None = None
    mix: Mix = field(default_factory=Mix)


@dataclass
class Batch:
    obs: torch.Tensor
    masks: torch.Tensor
    actions: torch.Tensor
    log_probs: torch.Tensor
    values: torch.Tensor
    rewards: torch.Tensor
    dones: torch.Tensor
    last_value: torch.Tensor
    last_obs: Any
    last_done: torch.Tensor
    results: list[dict[str, Any]]
    episodes: list[tuple[float, int]]
    steps: int


class Player(threading.Thread):
    """Plays batches of games with its copy of the network."""

    def __init__(self, settings: Settings, learner: Policy, out: queue.Queue):
        super().__init__(daemon=True)
        self.settings = settings
        self.copy = copy.deepcopy(learner)
        self.copy.eval()
        self.lock = threading.Lock()
        self.pending: dict | None = None
        self.out = out
        self.stop = threading.Event()
        self.want_snapshot = threading.Event()
        self.baseline_share = settings.baselines_start
        self.snapshots = 0
        self.error: BaseException | None = None

    def refresh(self, state: dict) -> None:
        """New weights from the learner, taken at the next batch."""
        with self.lock:
            self.pending = state

    def run(self) -> None:
        try:
            self._run()
        except BaseException as error:  # noqa: BLE001 -- handed to the learner
            self.error = error
            self.out.put(None)

    def _run(self) -> None:
        s = self.settings
        env = SelfPlayVecEnv(
            s.scenarios,
            games=s.games,
            seed=s.seed,
            threads=s.threads,
            max_rounds=s.max_rounds,
            mix=s.mix,
            random_goals=s.random_goals,
            destroy_goals=s.destroy_goals,
        )
        env.set_learner(self.copy)
        obs = env.reset()
        n = env.num_envs
        done = np.zeros(n, dtype=bool)
        seen = 0
        while not self.stop.is_set():
            with self.lock:
                if self.pending is not None:
                    self.copy.load_state_dict(self.pending)
                    self.pending = None
            if self.want_snapshot.is_set():
                env.snapshot()
                self.snapshots = len(env.snapshots)
                self.want_snapshot.clear()
            s.mix.baselines = self.baseline_share
            s.mix.current = max(0.0, 1.0 - s.mix.past - self.baseline_share)
            buffers = {k: [] for k in ("obs", "masks", "actions", "log_probs", "values", "rewards", "dones")}
            episodes: list[tuple[float, int]] = []
            for _ in range(s.rollout):
                masks = env.action_masks()
                obs_t, masks_t = torch.as_tensor(obs), torch.as_tensor(masks)
                with torch.no_grad():
                    actions, log_probs, values = self.copy.act(obs_t, masks_t)
                next_obs, rewards, dones, infos = env.step(actions.numpy())
                for name, value in (
                    ("obs", obs_t),
                    ("masks", masks_t),
                    ("actions", actions),
                    ("log_probs", log_probs),
                    ("values", values),
                    ("rewards", torch.as_tensor(rewards)),
                    ("dones", torch.as_tensor(done)),
                ):
                    buffers[name].append(value)
                episodes += [(info["episode"]["r"], info["episode"]["l"]) for info in infos if "episode" in info]
                obs, done = next_obs, dones
            with torch.no_grad():
                _, last_value = self.copy(torch.as_tensor(obs))
            results = env.results[seen:]
            seen = len(env.results)
            batch = Batch(
                **{k: torch.stack(v) for k, v in buffers.items()},
                last_value=last_value,
                last_obs=obs,
                last_done=torch.as_tensor(done),
                results=results,
                episodes=episodes,
                steps=s.rollout * n,
            )
            self.out.put(batch)


def advantages(batch: Batch, gamma: float, lam: float) -> tuple[torch.Tensor, torch.Tensor]:
    """Generalised advantage estimates and the returns they imply."""
    steps = batch.rewards.shape[0]
    adv = torch.zeros_like(batch.rewards)
    last = torch.zeros_like(batch.last_value)
    for t in reversed(range(steps)):
        if t == steps - 1:
            next_alive = 1.0 - batch.last_done.float()
            next_value = batch.last_value
        else:
            next_alive = 1.0 - batch.dones[t + 1].float()
            next_value = batch.values[t + 1]
        delta = batch.rewards[t] + gamma * next_value * next_alive - batch.values[t]
        last = delta + gamma * lam * next_alive * last
        adv[t] = last
    return adv, adv + batch.values


class Learner:
    """Trains the network on the player's batches."""

    def __init__(self, policy: Policy, settings: Settings, log: Callable[[str, float, int], None]):
        self.policy = policy
        self.settings = settings
        self.log = log
        self.optimizer = torch.optim.Adam(policy.parameters(), lr=settings.learning_rate, eps=1e-5)
        self.steps = 0
        self.recent: collections.deque = collections.deque(maxlen=1200)
        self.episodes: collections.deque = collections.deque(maxlen=200)

    @torch.no_grad()
    def refresh(self, batch: Batch) -> None:
        """Re-score the batch with the current network.

        The batch was played with weights an update or two old. Recomputing
        each choice's probability and each position's value now measures the
        update's clipping from where the network is, not where it was, as
        asynchronous PPO does; otherwise most samples land outside the clip
        and teach little.
        """
        steps, n = batch.rewards.shape
        flat = lambda t: t.reshape(steps * n, *t.shape[2:])  # noqa: E731
        obs, masks, actions = flat(batch.obs), flat(batch.masks), flat(batch.actions)
        log_probs, values = [], []
        for start in range(0, len(obs), self.settings.batch_size):
            part = slice(start, start + self.settings.batch_size)
            lp, _, v = self.policy.evaluate(obs[part], masks[part], actions[part])
            log_probs.append(lp)
            values.append(v)
        batch.log_probs = torch.cat(log_probs).reshape(steps, n)
        batch.values = torch.cat(values).reshape(steps, n)
        _, batch.last_value = self.policy(batch.obs.new_tensor(batch.last_obs))

    def update(self, batch: Batch) -> dict[str, float]:
        s = self.settings
        self.policy.eval()
        self.refresh(batch)
        adv, returns = advantages(batch, s.gamma, s.gae_lambda)
        flat = lambda t: t.reshape(-1, *t.shape[2:])  # noqa: E731
        obs, masks, actions = flat(batch.obs), flat(batch.masks), flat(batch.actions)
        old_log_probs, old_values = flat(batch.log_probs), flat(batch.values)
        adv, returns = flat(adv), flat(returns)
        size = len(obs)
        stats = collections.defaultdict(list)
        self.policy.train()
        for _ in range(s.epochs):
            order = torch.randperm(size)
            for start in range(0, size, s.batch_size):
                idx = order[start : start + s.batch_size]
                log_probs, entropy, values = self.policy.evaluate(obs[idx], masks[idx], actions[idx])
                a = adv[idx]
                a = (a - a.mean()) / (a.std() + 1e-8)
                ratio = torch.exp(log_probs - old_log_probs[idx])
                policy_loss = -torch.min(ratio * a, torch.clamp(ratio, 1 - s.clip, 1 + s.clip) * a).mean()
                value_loss = (returns[idx] - values).pow(2).mean()
                entropy_loss = -entropy.mean()
                loss = policy_loss + s.entropy * entropy_loss + s.value * value_loss
                self.optimizer.zero_grad()
                loss.backward()
                torch.nn.utils.clip_grad_norm_(self.policy.parameters(), s.max_grad_norm)
                self.optimizer.step()
                with torch.no_grad():
                    log_ratio = log_probs - old_log_probs[idx]
                    stats["approx_kl"].append(((torch.exp(log_ratio) - 1) - log_ratio).mean().item())
                    stats["clip_fraction"].append(((ratio - 1).abs() > s.clip).float().mean().item())
                stats["policy_gradient_loss"].append(policy_loss.item())
                stats["value_loss"].append(value_loss.item())
                stats["entropy_loss"].append(entropy_loss.item())
                stats["loss"].append(loss.item())
        variance = returns.var()
        explained = 1 - (returns - old_values).var() / variance if variance > 0 else torch.tensor(0.0)
        self.steps += batch.steps
        out = {f"train/{k}": float(np.mean(v)) for k, v in stats.items()}
        out["train/explained_variance"] = float(explained)
        out["train/learning_rate"] = s.learning_rate
        return out

    def record(self, batch: Batch) -> dict[str, float]:
        """Win rates by opponent, by scenario and side against the baselines,
        and how often each goal was met, over the recent games."""
        self.recent.extend(batch.results)
        self.episodes.extend(batch.episodes)
        out: dict[str, float] = {}
        by_opponent = collections.defaultdict(list)
        by_side = collections.defaultdict(list)
        by_goal = collections.defaultdict(list)
        for r in self.recent:
            by_opponent[r["opponent"]].append(r["outcome"])
            if r["opponent"] not in ("self", "past"):
                by_side[f"{r['scenario']}/{r['side']}"].append(r["outcome"] in WINS)
            by_goal[r.get("goal", "destroy")].append(r["return"] > 0)
        for opponent, outcomes in by_opponent.items():
            counts = collections.Counter(outcomes)
            for outcome in ("won", "drove off", "draw", "fled", "lost"):
                out[f"vs_{opponent}/{outcome.replace(' ', '_')}"] = counts[outcome] / len(outcomes)
            out[f"vs_{opponent}/win_rate"] = sum(counts[w] for w in WINS) / len(outcomes)
        for key, wins in by_side.items():
            out[f"win_rate/{key}"] = sum(wins) / len(wins)
        for goal, met in by_goal.items():
            out[f"goal/{goal}"] = sum(met) / len(met)
        if self.episodes:
            out["rollout/ep_rew_mean"] = float(np.mean([r for r, _ in self.episodes]))
            out["rollout/ep_len_mean"] = float(np.mean([l for _, l in self.episodes]))
        return out


def train(
    policy: Policy,
    settings: Settings,
    total_steps: int,
    log: Callable[[dict[str, float], int], None],
    save: Callable[[Policy, int], None],
    progress: Callable[[int], None] | None = None,
) -> Policy:
    """Train `policy` for `total_steps` learner decisions."""
    batches: queue.Queue = queue.Queue(maxsize=1)
    player = Player(settings, policy, batches)
    learner = Learner(policy, settings, lambda *_: None)
    player.start()
    next_snapshot = settings.snapshot_every
    started = time.perf_counter()
    try:
        while learner.steps < total_steps:
            batch = batches.get()
            if batch is None:
                raise RuntimeError("the player stopped") from player.error
            stats = learner.update(batch)
            player.refresh({k: v.detach().clone() for k, v in policy.state_dict().items()})
            stats.update(learner.record(batch))
            done = min(1.0, learner.steps / max(1, settings.taper_steps))
            player.baseline_share = settings.baselines_start + (settings.baselines_end - settings.baselines_start) * done
            stats["pool/baseline_share"] = player.baseline_share
            stats["pool/snapshots"] = player.snapshots
            stats["time/fps"] = learner.steps / (time.perf_counter() - started)
            if learner.steps >= next_snapshot:
                next_snapshot += settings.snapshot_every
                save(policy, learner.steps)
                player.want_snapshot.set()
            log(stats, learner.steps)
            if progress:
                progress(learner.steps)
    finally:
        player.stop.set()
        # Let the player finish its batch and exit.
        try:
            batches.get(timeout=30)
        except queue.Empty:
            pass
    return policy
