"""Carry a trained network over to a wider observation or more action heads.

    .venv/bin/python scripts/widen.py checkpoints/selfplay-3/final.zip checkpoints/selfplay-3/widened.zip
    .venv/bin/python scripts/widen.py checkpoints/side-20-9/final.pt checkpoints/widened-cap/final.pt

A per-side checkpoint (``"kind": "side"``) is widened by
:func:`callisto_rl.sidenet.widened`: new inputs get zero weight and a new
head's rows score zero, so the network plays as it did and a new head's open
choices start equally likely. The file written loads in this build without
further widening, and exports (`export_side.py`) with this build's layout.

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

# Every observation layout so far, by its total size: the length of the
# ship's own block, of each of the 19 friend slots, and of each of the 20 enemy
# slots. Each block only ever grew at its end, so an old input keeps its place
# within its block.
LAYOUTS = {
    1184: (40, 16, 42),
    1188: (44, 16, 42),
    1197: (53, 16, 42),
    1431: (67, 16, 53),
    1677: (80, 23, 58),
}
FRIENDS, ENEMIES = 19, 20


def column_map(old_size: int, new_size: int) -> list[int]:
    """For each input of the old layout, its place in the new one."""
    (old_self, old_friend, old_enemy), (new_self, new_friend, new_enemy) = LAYOUTS[old_size], LAYOUTS[new_size]
    places = list(range(old_self))
    for slot in range(FRIENDS):
        places += [new_self + slot * new_friend + i for i in range(old_friend)]
    for slot in range(ENEMIES):
        places += [new_self + FRIENDS * new_friend + slot * new_enemy + i for i in range(old_enemy)]
    assert len(places) == old_size
    return places


NEW_HEADS = {
    98: [(2, 5.0), (3, 2.0), (3, 2.0), (21, 0.0), (6, 0.0), (2, 2.0)],
    100: [(3, 2.0), (3, 2.0), (21, 0.0), (6, 0.0), (2, 2.0)],
    135: [],
}


def widen(old_path: str, new_path: str) -> None:
    old = MaskablePPO.load(old_path, device="cpu")
    old_obs = old.observation_space.shape[0]
    old_outputs = old.policy.action_net.out_features
    if old_obs not in LAYOUTS or OBS_LEN not in LAYOUTS or old_outputs not in NEW_HEADS:
        raise SystemExit(f"no record of a layout with {old_obs} inputs and {old_outputs} outputs")
    places = torch.tensor(column_map(old_obs, OBS_LEN))
    heads = NEW_HEADS[old_outputs]
    if old_outputs + sum(n for n, _ in heads) != sum(HEAD_SIZES):
        raise SystemExit("this build's heads do not match the records in widen.py")
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
    widened = {}
    for name, new_tensor in new_state.items():
        old_tensor = old_state[name]
        if old_tensor.shape == new_tensor.shape:
            widened[name] = old_tensor.clone()
        elif name.endswith("0.weight") and old_tensor.shape[1] == old_obs:
            # A first layer: old columns into the kept inputs, zeros for the new.
            tensor = torch.zeros_like(new_tensor)
            tensor[:, places] = old_tensor
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


def widen_pt(old_path: str, new_path: str) -> None:
    """The same for the new learner's .pt checkpoints."""
    from callisto_rl.policy import Policy

    saved = torch.load(old_path, map_location="cpu", weights_only=False)
    old_state = saved["state"]
    old_obs = old_state["pi.0.weight"].shape[1]
    old_outputs = old_state["action_net.weight"].shape[0]
    if old_obs not in LAYOUTS or OBS_LEN not in LAYOUTS or old_outputs not in NEW_HEADS:
        raise SystemExit(f"no record of a layout with {old_obs} inputs and {old_outputs} outputs")
    places = torch.tensor(column_map(old_obs, OBS_LEN))
    heads = NEW_HEADS[old_outputs]
    new = Policy(saved.get("hidden", (512, 512, 256)))
    new_state = new.state_dict()
    for name, tensor in new_state.items():
        old = old_state[name]
        if old.shape == tensor.shape:
            new_state[name] = old.clone()
        elif name in ("pi.0.weight", "vf.0.weight"):
            widened = torch.zeros_like(tensor)
            widened[:, places] = old
            new_state[name] = widened
        elif name == "action_net.weight":
            widened = tensor.clone() * 0.01
            widened[:old_outputs] = old
            new_state[name] = widened
        elif name == "action_net.bias":
            widened = torch.zeros_like(tensor)
            widened[:old_outputs] = old
            at = old_outputs
            for size, bias in heads:
                widened[at] = bias
                at += size
            new_state[name] = widened
        else:
            raise SystemExit(f"cannot carry {name} from {tuple(old.shape)} to {tuple(tensor.shape)}")
    new.load_state_dict(new_state)
    new.save(new_path, {k: v for k, v in saved.items() if k not in ("state", "hidden")})
    print(f"{old_path} ({old_obs} inputs, {old_outputs} outputs) -> {new_path} ({OBS_LEN}, {sum(HEAD_SIZES)})")


def widen_side(old_path: str, new_path: str) -> None:
    """A per-side network's checkpoint, in this build's layout."""
    from callisto_rl._engine import OWN_FEATS, PAIR_FEATS, SIDE_FEATS, SIDE_HEAD_SIZES
    from callisto_rl.sidenet import SideNet

    saved = torch.load(old_path, map_location="cpu", weights_only=False)
    before = {k: tuple(v.shape) for k, v in saved["state"].items()}
    net = SideNet.load(old_path)
    changed = [k for k, v in net.state_dict().items() if tuple(v.shape) != before[k]]
    extra = {k: v for k, v in saved.items() if k not in ("kind", "config", "state")}
    net.save(new_path, {**extra, "widened_from": str(old_path)})
    print(
        f"{old_path} -> {new_path}: own {before['own_in.0.weight'][1]} -> {OWN_FEATS}, "
        f"side {before['side_in.0.weight'][1]} -> {SIDE_FEATS}, pair {before['pair_in.0.weight'][1]} -> {PAIR_FEATS}, "
        f"plain {before['plain.weight'][0]} -> {net.state_dict()['plain.weight'].shape[0]} ({len(SIDE_HEAD_SIZES)} heads); "
        f"resized {', '.join(changed) or 'nothing'}"
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("old")
    parser.add_argument("new")
    args = parser.parse_args()
    if args.old.endswith(".zip"):
        widen(args.old, args.new)
    elif torch.load(args.old, map_location="cpu", weights_only=False).get("kind") == "side":
        widen_side(args.old, args.new)
    else:
        widen_pt(args.old, args.new)


if __name__ == "__main__":
    main()
