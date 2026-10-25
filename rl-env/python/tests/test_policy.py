"""The one-block heads give exactly what Stable-Baselines3's per-head
distributions give, so training can move over without changing anything."""

import numpy as np
import torch
from sb3_contrib import MaskablePPO

from callisto_rl import HEAD_SIZES, MASK_LEN, OBS_LEN, ShipVecEnv, default_pool
from callisto_rl.policy import Policy


def test_matches_stable_baselines(tmp_path):
    env = ShipVecEnv(default_pool(["Tessaly Crossing.json"]), games=4, threads=2)
    sb3 = MaskablePPO("MlpPolicy", env, policy_kwargs={"net_arch": [512, 512, 256]}, device="cpu")
    sb3.save(tmp_path / "m.zip")
    ours = Policy.from_sb3(tmp_path / "m.zip")
    obs = torch.tensor(env.reset())
    masks = env.action_masks()
    rng = np.random.default_rng(0)
    actions = np.zeros((len(obs), len(HEAD_SIZES)), dtype=np.int64)
    at = 0
    for head, size in enumerate(HEAD_SIZES):
        open_ = masks[:, at : at + size]
        actions[:, head] = (rng.random(open_.shape) + open_).argmax(1)
        at += size
    with torch.no_grad():
        values_sb3, logp_sb3, ent_sb3 = sb3.policy.evaluate_actions(obs, torch.tensor(actions), action_masks=masks)
        logp, ent, values = ours.evaluate(obs, torch.tensor(masks), torch.tensor(actions))
    assert torch.allclose(logp, logp_sb3, atol=1e-4)
    assert torch.allclose(ent, ent_sb3, atol=1e-4)
    assert torch.allclose(values, values_sb3.squeeze(-1), atol=1e-5)


def test_samples_only_open_choices():
    policy = Policy()
    obs = torch.randn(64, OBS_LEN)
    masks = torch.rand(64, MASK_LEN) < 0.5
    at = 0
    for size in HEAD_SIZES:  # every head has something open
        masks[:, at] = True
        at += size
    actions, _, _ = policy.act(obs, masks)
    at = 0
    for head, size in enumerate(HEAD_SIZES):
        chosen = masks[torch.arange(64), at + actions[:, head]]
        assert chosen.all(), f"head {head} chose a closed option"
        at += size
