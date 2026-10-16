"""The Python layer: shapes, masks, and a game played to its end."""

import numpy as np

from callisto_rl import HEAD_SIZES, MASK_LEN, OBS_LEN, ShipVecEnv, default_pool


def test_shapes_and_masks():
    env = ShipVecEnv(default_pool(["Tessaly Crossing.json"]), games=2, threads=2)
    obs = env.reset()
    assert obs.shape == (env.num_envs, OBS_LEN)
    assert env.action_masks().shape == (env.num_envs, MASK_LEN)
    # Every head always has a choice.
    at = 0
    for size in HEAD_SIZES:
        assert env.action_masks()[:, at : at + size].any(axis=1).all()
        at += size


def test_games_end_and_restart():
    env = ShipVecEnv(default_pool(["Thebus.json"]), games=2, threads=2, opponent="close", max_rounds=10)
    env.reset()
    ended = []
    for _ in range(12):
        obs, rewards, dones, infos = env.step(np.zeros((env.num_envs, len(HEAD_SIZES)), dtype=np.int64))
        ended += [info for info in infos if "episode" in info]
        assert np.isfinite(obs).all() and np.isfinite(rewards).all()
    assert ended, "every game ends by the round limit"
    assert all(info["outcome"] in ("won", "drove off", "draw", "fled", "lost") for info in ended)
    assert all(info["terminal_observation"].shape == (OBS_LEN,) for info in ended)
