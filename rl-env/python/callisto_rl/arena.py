"""Play saved policies against each other and the baselines, and rate them.

A *player* is a checkpoint (a MaskablePPO ``.zip``) or a baseline name. A
match is one player playing the learner's side against another, alternating
sides, over the scenario pool. Ratings are Elo, fitted to every match at once
(a Bradley-Terry fit), with ``close`` held at 1000 so numbers from different
runs can be compared.
"""

from __future__ import annotations

import collections
import math
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Sequence

import numpy as np
from sb3_contrib import MaskablePPO

from .selfplay import Opponent, SelfPlayVecEnv

BASELINES = ("close", "hold", "idle", "random")
WINS = ("won", "drove off")
ANCHOR, ANCHOR_RATING = "close", 1000.0


def is_baseline(player: str) -> bool:
    return player in BASELINES


def load_policy(path: str) -> Any:
    policy = MaskablePPO.load(path, device="cpu").policy
    policy.set_training_mode(False)
    return policy


def score(outcome: str) -> float:
    if outcome in WINS:
        return 1.0
    return 0.5 if outcome == "draw" else 0.0


@dataclass
class Match:
    player: str
    opponent: str
    results: list[dict[str, Any]]

    @property
    def score(self) -> float:
        """The player's average score: 1 a win, a half a draw."""
        return sum(score(r["outcome"]) for r in self.results) / max(1, len(self.results))


def play(
    player: str,
    opponent: str,
    games: int,
    scenarios: Sequence[str] | None = None,
    batch: int = 32,
    threads: int = 8,
    seed: int = 1_000_000,
    policies: dict[str, Any] | None = None,
) -> Match:
    """``player`` (a checkpoint) against ``opponent`` (a checkpoint or a
    baseline) for at least ``games`` finished games."""
    policies = policies if policies is not None else {}
    for name in (player, opponent):
        if not is_baseline(name) and name not in policies:
            policies[name] = load_policy(name)
    seat = Opponent("baseline", opponent) if is_baseline(opponent) else Opponent("past", 0)
    env = SelfPlayVecEnv(scenarios, games=batch, seed=seed, threads=threads, choose=lambda _game: seat)
    env.set_learner(policies[player])
    if not is_baseline(opponent):
        env.snapshots = [policies[opponent]]
    obs = env.reset()
    learner = policies[player]
    while len(env.results) < games:
        actions, _ = learner.predict(obs, action_masks=env.action_masks(), deterministic=False)
        obs, _, _, _ = env.step(actions)
    return Match(player, opponent, env.results[:games])


def ratings(matches: Sequence[Match], iterations: int = 2_000) -> dict[str, float]:
    """Elo ratings that best explain every match, ``close`` at 1000."""
    players = sorted({m.player for m in matches} | {m.opponent for m in matches})
    index = {p: i for i, p in enumerate(players)}
    games = [(index[m.player], index[m.opponent], score(r["outcome"])) for m in matches for r in m.results]
    strength = np.zeros(len(players))
    scale = 400.0 / math.log(10.0)
    for _ in range(iterations):
        gradient = np.zeros(len(players))
        for a, b, s in games:
            expected = 1.0 / (1.0 + math.exp(strength[b] - strength[a]))
            gradient[a] += s - expected
            gradient[b] -= s - expected
        strength += 4.0 * gradient / max(1, len(games))
        if ANCHOR in index:
            strength -= strength[index[ANCHOR]]
    offset = ANCHOR_RATING if ANCHOR in index else 0.0
    return {p: offset + scale * strength[index[p]] for p in players}


def breakdown(match: Match) -> dict[str, float]:
    """The player's win rate by scenario and side."""
    wins: dict[str, list[bool]] = collections.defaultdict(list)
    for r in match.results:
        wins[f"{r['scenario']} / {r['side']}"].append(r["outcome"] in WINS)
    return {key: sum(v) / len(v) for key, v in sorted(wins.items())}


def short(player: str) -> str:
    if is_baseline(player):
        return player
    path = Path(player)
    return f"{path.parent.name}/{path.stem}"
