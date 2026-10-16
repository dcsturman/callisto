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

# Where each earlier layout's network is missing inputs, by its input count:
# (position in this build's observation, how many). The goal flags came at
# 40; the withdrawal, hand-off, captain and jamming inputs at 44.
NEW_INPUTS = {1184: [(40, 13)], 1188: [(44, 9)]}
# The heads each earlier layout lacks, appended after its own, by its output
# count: (choices, the bias that starts it on its first choice). The bias is
# strong for the jump (a withdrawal tried at random is costly) and mild for
# the rest, so training still explores them.
NEW_HEADS = {
    98: [(2, 5.0), (3, 2.0), (3, 2.0), (21, 0.0), (6, 0.0), (2, 2.0)],
    100: [(3, 2.0), (3, 2.0), (21, 0.0), (6, 0.0), (2, 2.0)],
}


def widen(old_path: str, new_path: str) -> None:
    old = MaskablePPO.load(old_path, device="cpu")
    old_obs = old.observation_space.shape[0]
    old_outputs = old.policy.action_net.out_features
    if old_obs not in NEW_INPUTS or old_outputs not in NEW_HEADS:
        raise SystemExit(f"no record of a layout with {old_obs} inputs and {old_outputs} outputs")
    inputs, heads = NEW_INPUTS[old_obs], NEW_HEADS[old_outputs]
    if old_obs + sum(n for _, n in inputs) != OBS_LEN or old_outputs + sum(n for n, _ in heads) != sum(HEAD_SIZES):
        raise SystemExit("this build's layout does not match the records in widen.py")
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
    for at, n in inputs:
        kept[at : at + n] = False
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
            for size, bias in heads:
                tensor[at] = bias
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
