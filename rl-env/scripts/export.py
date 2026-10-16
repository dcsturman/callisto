"""Export a checkpoint's policy for Rust: `<checkpoint>.cpol` beside it.

    .venv/bin/python scripts/export.py checkpoints/selfplay-1/final.zip

The file is what `callisto_rl::net::Policy` reads: the bytes ``CPOL``, a
little-endian u32 header length, a JSON header, then each layer's weights
(outputs by inputs) and biases as little-endian float32. Only the policy is
exported; the value network is for training alone.
"""

import argparse
import json
import struct
from pathlib import Path

import numpy as np
import torch
from sb3_contrib import MaskablePPO

from callisto_rl import HEAD_SIZES, OBS_LEN


def export(checkpoint: str, out: str | None = None) -> Path:
    policy = MaskablePPO.load(checkpoint, device="cpu").policy
    linears = [m for m in policy.mlp_extractor.policy_net if isinstance(m, torch.nn.Linear)]
    activations = {type(m).__name__ for m in policy.mlp_extractor.policy_net if not isinstance(m, torch.nn.Linear)}
    if activations != {"Tanh"}:
        raise SystemExit(f"only tanh networks are supported, not {activations}")
    linears.append(policy.action_net)
    header = {
        "activation": "tanh",
        "obs_len": OBS_LEN,
        "head_sizes": [int(size) for size in HEAD_SIZES],
        "layers": [[int(m.in_features), int(m.out_features)] for m in linears],
    }
    encoded = json.dumps(header).encode()
    path = Path(out) if out else Path(checkpoint).with_suffix(".cpol")
    with open(path, "wb") as file:
        file.write(b"CPOL")
        file.write(struct.pack("<I", len(encoded)))
        file.write(encoded)
        for m in linears:
            file.write(m.weight.detach().numpy().astype("<f4").tobytes())
            file.write(m.bias.detach().numpy().astype("<f4").tobytes())
    return path


def check(checkpoint: str, path: Path) -> float:
    """The largest difference between PyTorch's scores and a NumPy replay of
    the exported file, on random observations."""
    policy = MaskablePPO.load(checkpoint, device="cpu").policy
    obs = np.random.default_rng(0).uniform(-1, 1, (16, OBS_LEN)).astype(np.float32)
    with torch.no_grad():
        latent = policy.mlp_extractor.forward_actor(policy.extract_features(torch.tensor(obs)))
        expected = policy.action_net(latent).numpy()
    data = path.read_bytes()
    length = struct.unpack("<I", data[4:8])[0]
    header = json.loads(data[8 : 8 + length])
    floats = np.frombuffer(data[8 + length :], dtype="<f4")
    x, at = obs, 0
    for i, (inputs, outputs) in enumerate(header["layers"]):
        w = floats[at : at + inputs * outputs].reshape(outputs, inputs)
        at += inputs * outputs
        b = floats[at : at + outputs]
        at += outputs
        x = x @ w.T + b
        if i < len(header["layers"]) - 1:
            x = np.tanh(x)
    return float(np.abs(x - expected).max())


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("checkpoints", nargs="+")
    args = parser.parse_args()
    for checkpoint in args.checkpoints:
        path = export(checkpoint)
        print(f"{path}  (largest difference from PyTorch: {check(checkpoint, path):.2e})")


if __name__ == "__main__":
    main()
