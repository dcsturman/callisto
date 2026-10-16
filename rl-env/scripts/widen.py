"""Carry a trained network over to a wider observation or more action heads.

    .venv/bin/python scripts/widen.py checkpoints/selfplay-3/final.zip checkpoints/selfplay-3/widened.zip

When the environment gains inputs or heads, an old checkpoint no longer fits.
This builds a network of the new shape and copies every old weight across:
new inputs get zero weight, so the network behaves exactly as before until
training finds a use for them; new heads get small weights, with a bias that
starts them on their first choice (for the jump head, "do not jump").

Inputs and heads are matched by name, from the layouts below: what the old
build had, and what this build has.
"""

import argparse

import numpy as np
import torch
from sb3_contrib import MaskablePPO

from callisto_rl import HEAD_SIZES, OBS_LEN, ShipVecEnv, default_pool

# Where this build added inputs: (position in the new observation, how many).
# The goal flags sit after the 40 numbers the ship's own block had.
NEW_INPUTS = [(40, 4)]
# Heads added at the end of the action outputs: how many choices each.
NEW_HEADS = [2]
# The bias that starts a new head on its first choice.
FIRST_CHOICE_BIAS = 8.0


def widen(old_path: str, new_path: str) -> None:
    old = MaskablePPO.load(old_path, device="cpu")
    old_obs = old.observation_space.shape[0]
    added = sum(n for _, n in NEW_INPUTS)
    if old_obs + added != OBS_LEN:
        raise SystemExit(f"old network takes {old_obs} inputs; adding {added} gives {old_obs + added}, not {OBS_LEN}")
    env = ShipVecEnv(default_pool(["Thebus.json"]), games=1, threads=1)
    new = MaskablePPO(
        "MlpPolicy",
        env,
        n_steps=old.n_steps,
        batch_size=old.batch_size,
        n_epochs=old.n_epochs,
        learning_rate=old.learning_rate,
        gamma=old.gamma,
        gae_lambda=old.gae_lambda,
        ent_coef=old.ent_coef,
        policy_kwargs={"net_arch": [512, 512, 256]},
        device="cpu",
    )
    old_state = old.policy.state_dict()
    new_state = new.policy.state_dict()
    kept = np.ones(OBS_LEN, dtype=bool)
    for at, n in NEW_INPUTS:
        kept[at : at + n] = False
    old_outputs = old.policy.action_net.out_features
    widened = {}
    for name, new_tensor in new_state.items():
        old_tensor = old_state[name]
        if old_tensor.shape == new_tensor.shape:
            widened[name] = old_tensor.clone()
        elif name.endswith("0.weight") and old_tensor.shape[1] == old_obs:
            # A first layer: old columns into the kept inputs, zeros for the new.
            tensor = torch.zeros_like(new_tensor)
            tensor[:, torch.from_numpy(kept)] = old_tensor
            widened[name] = tensor
        elif name == "action_net.weight":
            tensor = new_tensor.clone() * 0.01
            tensor[:old_outputs] = old_tensor
            widened[name] = tensor
        elif name == "action_net.bias":
            tensor = torch.zeros_like(new_tensor)
            tensor[:old_outputs] = old_tensor
            at = old_outputs
            for size in NEW_HEADS:
                tensor[at] = FIRST_CHOICE_BIAS
                at += size
            widened[name] = tensor
        else:
            raise SystemExit(f"cannot carry {name} from {tuple(old_tensor.shape)} to {tuple(new_tensor.shape)}")
    new.policy.load_state_dict(widened)
    new.save(new_path)
    print(f"{old_path} ({old_obs} inputs, {old_outputs} outputs) -> {new_path} ({OBS_LEN}, {sum(HEAD_SIZES)})")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("old")
    parser.add_argument("new")
    args = parser.parse_args()
    widen(args.old, args.new)


if __name__ == "__main__":
    main()
