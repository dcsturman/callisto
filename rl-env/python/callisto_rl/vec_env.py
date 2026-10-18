"""The engine as a Stable-Baselines3 ``VecEnv``: one entry per ship.

Every ship the network plays is an entry, all sharing one policy, so a batch
of 32 Tessaly games against a baseline is 128 entries. A ship's reward is her
side's. When a game ends, every one of its ships is done and the game starts
again at once; a ship destroyed earlier stays an entry, seeing zeros, with
only the do-nothing choice open, until her game ends.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any, Sequence

import numpy as np
from gymnasium import spaces
from stable_baselines3.common.vec_env.base_vec_env import VecEnv

from ._engine import HEAD_SIZES, MASK_LEN, OBS_LEN, Engine

REPO = Path(__file__).resolve().parents[3]
SCENARIOS = REPO / "callisto" / "scenarios"
DESIGNS = REPO / "callisto" / "ship_templates"

#: The training pool: the scenarios the LLM commanders were measured on.
POOL = ["Tessaly Crossing.json", "jupiter.json", "Marduk Encounter.json", "Thebus.json"]
#: The per-side network's pool: the four above and five of 10 to 24 ships a
#: side, which only it can play.
SIDE_POOL = POOL + [
    "Gazelle Duel.json", "Fighter Screen.json", "Aslan Border Clash.json", "Stealth Strike.json", "Fighter Swarm.json",
    # A convoy that must jump clear against corsairs that must stop it: the
    # one scenario where a side's goal is Escape.
    "Convoy Escape.json",
    # Short-reach heavy guns against long-reach ones: the brawlers must close,
    # the kiters must keep their distance.
    "Brawlers and Kiters.json",
]


def default_pool(names: Sequence[str] = POOL) -> list[str]:
    """The pool's scenario files, as paths."""
    return [str(SCENARIOS / name) for name in names]


class ShipVecEnv(VecEnv):
    """Many callisto games, one entry per ship the network plays."""

    def __init__(
        self,
        scenarios: Sequence[str] | None = None,
        games: int = 32,
        opponent: str | None = "close",
        max_rounds: int = 40,
        seed: int = 0,
        threads: int = 10,
        designs: str | None = None,
        random_goals: float = 0.0,
        destroy_goals: float = 0.0,
    ):
        self.engine = Engine(
            list(scenarios or default_pool()),
            str(designs or DESIGNS),
            games,
            max_rounds=max_rounds,
            opponent=opponent,
            seed=seed,
            threads=threads,
            random_goals=random_goals,
            destroy_goals=destroy_goals,
        )
        self.slot_info = self.engine.slot_info()
        observation_space = spaces.Box(low=-1.5, high=1.5, shape=(OBS_LEN,), dtype=np.float32)
        action_space = spaces.MultiDiscrete(HEAD_SIZES)
        super().__init__(self.engine.num_slots, observation_space, action_space)
        self._masks = np.ones((self.num_envs, MASK_LEN), dtype=bool)
        self._actions: np.ndarray | None = None
        self._rounds = np.zeros(self.num_envs, dtype=np.int64)
        self.opponent = opponent
        #: Every game finished so far, as the engine reported it.
        self.finished: list[dict[str, Any]] = []
        #: The same, one entry per side the network played: scenario, side
        #: name, opponent, outcome, return, rounds.
        self.results: list[dict[str, Any]] = []

    # -- VecEnv -------------------------------------------------------------

    def reset(self) -> np.ndarray:
        obs, self._masks = self.engine.observe()
        return obs

    def step_async(self, actions: np.ndarray) -> None:
        self._actions = np.asarray(actions, dtype=np.int64).reshape(self.num_envs, len(HEAD_SIZES))

    def step_wait(self):
        assert self._actions is not None, "step_async first"
        obs, self._masks, rewards, dones, terminal, finished = self.engine.step(self._actions)
        self._actions = None
        self._rounds += 1
        infos: list[dict[str, Any]] = [{} for _ in range(self.num_envs)]
        by_game = {game["game"]: game for game in finished}
        self.finished.extend(finished)
        for game in finished:
            for side in (0, 1):
                if game["agent"][side]:
                    self.results.append(
                        {
                            "scenario": game["scenario"],
                            "side": game["sides"][side],
                            "goal": game["goals"][side],
                            "opponent": self.opponent or "self",
                            "outcome": outcome_for(game, side),
                            "return": float(game["returns"][side]),
                            "rounds": int(game["rounds"]),
                        }
                    )
        for entry in np.flatnonzero(dones):
            game_index, side, _ship = self.slot_info[entry]
            game = by_game.get(game_index)
            info = infos[entry]
            info["terminal_observation"] = terminal[entry]
            if game is not None:
                info["episode"] = {"r": float(game["returns"][side]), "l": int(game["rounds"])}
                info["outcome"] = outcome_for(game, side)
            self._rounds[entry] = 0
        return obs, rewards, dones, infos

    def close(self) -> None:
        pass

    def action_masks(self) -> np.ndarray:
        """Every entry's mask, for MaskablePPO."""
        return self._masks

    def env_method(self, method_name: str, *args, indices=None, **kwargs) -> list[Any]:
        if method_name == "action_masks":
            return list(self._masks[self._indices(indices)])
        raise NotImplementedError(method_name)

    def has_attr(self, attr_name: str) -> bool:
        return attr_name in ("action_masks", "render_mode")

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


def outcome_for(game: dict[str, Any], side: int) -> str:
    """How a finished game went for one side: won, drove off, draw, fled, lost."""
    end, loser = game["end"], game["loser"]
    if end == "draw":
        return "draw"
    ours = loser == side
    if end == "destroyed":
        return "lost" if ours else "won"
    return "fled" if ours else "drove off"
