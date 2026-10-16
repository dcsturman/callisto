"""Self-play: the learner against its current self, its past selves, and the
baselines.

In every game the learner plays one side, the same side for that game slot
throughout, alternating between slots so it learns both. The other side is
played by an opponent drawn afresh for each game:

- most often the learner's current policy, so the opponent keeps pace;
- sometimes a snapshot of an earlier policy, so it does not forget how to
  beat strategies it has moved past;
- sometimes a baseline (``close`` or ``hold``), the fixed yardstick, played
  by the engine in Rust.

Only the learner's ships are entries in this ``VecEnv``: Stable-Baselines3
never sees the opponent's, which are decided here from the opponent's
observations.
"""

from __future__ import annotations

import copy
import random
from dataclasses import dataclass
from typing import Any, Callable, Sequence

import numpy as np
import torch
from gymnasium import spaces
from stable_baselines3.common.vec_env.base_vec_env import VecEnv

from ._engine import HEAD_SIZES, MASK_LEN, OBS_LEN, Engine
from .vec_env import DESIGNS, default_pool, outcome_for


@dataclass(frozen=True)
class Opponent:
    """Who plays the other side of a game."""

    kind: str  # "self", "past" or "baseline"
    #: The snapshot's index in the pool, or the baseline's name.
    which: int | str | None = None

    @property
    def label(self) -> str:
        if self.kind == "self":
            return "self"
        if self.kind == "past":
            return "past"
        return str(self.which)


@dataclass
class Mix:
    """How often each kind of opponent is drawn."""

    current: float = 0.7
    past: float = 0.15
    baselines: float = 0.15
    baseline_names: tuple[str, ...] = ("close", "hold")


class SelfPlayVecEnv(VecEnv):
    """Many games; the learner's ships are the entries."""

    def __init__(
        self,
        scenarios: Sequence[str] | None = None,
        games: int = 32,
        max_rounds: int = 40,
        seed: int = 0,
        threads: int = 8,
        designs: str | None = None,
        mix: Mix | None = None,
        max_snapshots: int = 20,
        choose: Callable[[int], Opponent] | None = None,
        random_goals: float = 0.0,
        destroy_goals: float = 0.0,
    ):
        scenarios = list(scenarios or default_pool())
        self.engine = Engine(
            scenarios,
            str(designs or DESIGNS),
            games,
            max_rounds=max_rounds,
            opponent=None,
            seed=seed,
            threads=threads,
            random_goals=random_goals,
            destroy_goals=destroy_goals,
        )
        self.mix = mix or Mix()
        self.max_snapshots = max_snapshots
        self.rng = random.Random(seed)
        self._choose = choose
        self.slot_info = self.engine.slot_info()
        games_count = self.engine.num_games
        #: The side the learner plays in each game.
        self.learner_side = [(game // len(scenarios)) % 2 for game in range(games_count)]
        self.learner_rows = np.array(
            [row for row, (game, side, _) in enumerate(self.slot_info) if side == self.learner_side[game]]
        )
        self.opponent_rows_of_game: dict[int, np.ndarray] = {
            game: np.array(
                [row for row, (g, side, _) in enumerate(self.slot_info) if g == game and side != self.learner_side[game]]
            )
            for game in range(games_count)
        }
        observation_space = spaces.Box(low=-1.5, high=1.5, shape=(OBS_LEN,), dtype=np.float32)
        action_space = spaces.MultiDiscrete(HEAD_SIZES)
        super().__init__(len(self.learner_rows), observation_space, action_space)

        #: The learner's live policy, set by the trainer: the "self" opponent.
        self.learner: Any = None
        #: Frozen copies of earlier policies.
        self.snapshots: list[Any] = []
        self.opponents: list[Opponent] = [Opponent("self")] * games_count
        self._obs = np.zeros((self.engine.num_slots, OBS_LEN), dtype=np.float32)
        self._masks = np.ones((self.engine.num_slots, MASK_LEN), dtype=bool)
        self._actions: np.ndarray | None = None
        #: Every finished game, from the learner's side: scenario, side name,
        #: opponent, outcome, return, rounds.
        self.results: list[dict[str, Any]] = []

    # -- the pool -------------------------------------------------------------

    def set_learner(self, policy: Any) -> None:
        self.learner = policy

    def snapshot(self) -> None:
        """Freeze a copy of the learner's policy into the pool."""
        frozen = copy.deepcopy(self.learner)
        frozen.set_training_mode(False)
        self.snapshots.append(frozen)
        if len(self.snapshots) > self.max_snapshots:
            # Keep the first, as the oldest reference, and drop the next.
            del self.snapshots[1]

    def draw(self, game: int) -> Opponent:
        if self._choose is not None:
            return self._choose(game)
        roll = self.rng.random()
        if roll < self.mix.baselines:
            return Opponent("baseline", self.rng.choice(self.mix.baseline_names))
        if roll < self.mix.baselines + self.mix.past and self.snapshots:
            return Opponent("past", self.rng.randrange(len(self.snapshots)))
        return Opponent("self")

    def _seat(self, game: int, opponent: Opponent) -> None:
        self.opponents[game] = opponent
        side = 1 - self.learner_side[game]
        self.engine.stand_in(game, side, opponent.which if opponent.kind == "baseline" else None)

    # -- VecEnv -------------------------------------------------------------

    def reset(self) -> np.ndarray:
        for game in range(self.engine.num_games):
            self._seat(game, self.draw(game))
        self._obs, self._masks = self.engine.observe()
        return self._obs[self.learner_rows]

    def step_async(self, actions: np.ndarray) -> None:
        self._actions = np.asarray(actions, dtype=np.int64).reshape(self.num_envs, len(HEAD_SIZES))

    def _opponent_actions(self, full: np.ndarray) -> None:
        """Fill in the opponents' choices, one policy at a time."""
        groups: dict[int, list[int]] = {}
        for game, opponent in enumerate(self.opponents):
            if opponent.kind == "baseline":
                continue  # the engine plays it
            key = -1 if opponent.kind == "self" else int(opponent.which)
            groups.setdefault(key, []).extend(self.opponent_rows_of_game[game].tolist())
        for key, rows in groups.items():
            if not rows:
                continue
            policy = self.learner if key == -1 else self.snapshots[key]
            rows_array = np.array(rows)
            with torch.no_grad():
                chosen, _ = policy.predict(
                    self._obs[rows_array], action_masks=self._masks[rows_array], deterministic=False
                )
            full[rows_array] = chosen

    def step_wait(self):
        assert self._actions is not None, "step_async first"
        full = np.zeros((self.engine.num_slots, len(HEAD_SIZES)), dtype=np.int64)
        full[self.learner_rows] = self._actions
        self._opponent_actions(full)
        self._obs, self._masks, rewards, dones, terminal, finished = self.engine.step(full)
        self._actions = None

        by_game = {game["game"]: game for game in finished}
        for game_index, game in by_game.items():
            side = self.learner_side[game_index]
            opponent = self.opponents[game_index]
            self.results.append(
                {
                    "scenario": game["scenario"],
                    "side": game["sides"][side],
                    "goal": game["goals"][side],
                    "opponent": opponent.label,
                    "outcome": outcome_for(game, side),
                    "return": float(game["returns"][side]),
                    "rounds": int(game["rounds"]),
                }
            )
            # The engine has already started the next game: seat its opponent.
            self._seat(game_index, self.draw(game_index))

        rows = self.learner_rows
        infos: list[dict[str, Any]] = [{} for _ in range(self.num_envs)]
        for entry in np.flatnonzero(dones[rows]):
            row = rows[entry]
            game_index, side, _ = self.slot_info[row]
            info = infos[entry]
            info["terminal_observation"] = terminal[row]
            game = by_game.get(game_index)
            if game is not None:
                info["episode"] = {"r": float(game["returns"][side]), "l": int(game["rounds"])}
        return self._obs[rows], rewards[rows], dones[rows], infos

    def close(self) -> None:
        pass

    def action_masks(self) -> np.ndarray:
        return self._masks[self.learner_rows]

    def has_attr(self, attr_name: str) -> bool:
        return attr_name in ("action_masks", "render_mode")

    def env_method(self, method_name: str, *args, indices=None, **kwargs) -> list[Any]:
        if method_name == "action_masks":
            return list(self.action_masks()[self._indices(indices)])
        raise NotImplementedError(method_name)

    def get_attr(self, attr_name: str, indices=None) -> list[Any]:
        if attr_name == "render_mode":
            return [None for _ in self._indices(indices)]
        raise AttributeError(attr_name)

    def set_attr(self, attr_name: str, value: Any, indices=None) -> None:
        raise NotImplementedError(attr_name)

    def env_is_wrapped(self, wrapper_class, indices=None) -> list[bool]:
        return [False for _ in self._indices(indices)]

    def _indices(self, indices) -> list[int]:
        if indices is None:
            return list(range(self.num_envs))
        if isinstance(indices, int):
            return [indices]
        return list(indices)
