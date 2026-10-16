"""Self-play: only the learner's ships are entries, opponents are seated,
and the arena rates players."""

import numpy as np

from callisto_rl import HEAD_SIZES, Opponent, SelfPlayVecEnv, default_pool
from callisto_rl.arena import Match, ratings


class FirstChoice:
    """A stand-in policy: always the first open choice of every head."""

    def predict(self, obs, action_masks=None, deterministic=False):
        return np.zeros((len(obs), len(HEAD_SIZES)), dtype=np.int64), None


def test_learner_entries_and_opponents():
    env = SelfPlayVecEnv(default_pool(["Thebus.json"]), games=4, threads=2, max_rounds=8)
    env.set_learner(FirstChoice())
    obs = env.reset()
    # Thebus is four ships against one; the learner alternates sides.
    assert len(obs) == env.num_envs < env.engine.num_slots
    for _ in range(10):
        obs, rewards, dones, infos = env.step(np.zeros((env.num_envs, len(HEAD_SIZES)), dtype=np.int64))
    assert env.results, "games finish within the round limit"
    assert {r["opponent"] for r in env.results} <= {"self", "past", "close", "hold"}


def test_baseline_seat_is_played_by_the_engine():
    seat = Opponent("baseline", "close")
    env = SelfPlayVecEnv(default_pool(["Thebus.json"]), games=2, threads=2, max_rounds=8, choose=lambda _g: seat)
    env.set_learner(FirstChoice())
    env.reset()
    for _ in range(10):
        env.step(np.zeros((env.num_envs, len(HEAD_SIZES)), dtype=np.int64))
    assert env.results and all(r["opponent"] == "close" for r in env.results)


def test_ratings_put_the_stronger_player_higher():
    win = {"outcome": "won"}
    loss = {"outcome": "lost"}
    matches = [Match("a", "close", [win] * 30 + [loss] * 10), Match("b", "close", [win] * 10 + [loss] * 30)]
    rated = ratings(matches)
    assert rated["close"] == 1000.0
    assert rated["a"] > 1000.0 > rated["b"]
