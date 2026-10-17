"""Export a per-side network for Rust: `<checkpoint>.cside` beside it.

    .venv/bin/python scripts/export_side.py checkpoints/side-2/final.pt

The file is the bytes ``CSID``, a little-endian u32 header length, a JSON
header (the network's settings, the layout it was trained for, and every
tensor's name and shape in order), then every tensor as little-endian
float32, row-major. callisto_rl::side_net reads it.
"""

import argparse
import json
import struct
from pathlib import Path

import numpy as np

from callisto_rl._engine import FOE_FEATS, FOES, OWN, OWN_FEATS, PAIR_FEATS, SIDE_FEATS, SIDE_HEAD_SIZES
from callisto_rl.sidenet import SideNet


def export(checkpoint: str) -> Path:
    net = SideNet.load(checkpoint)
    state = {k: v.detach().cpu().numpy().astype("<f4") for k, v in net.state_dict().items()}
    header = {
        "config": net.config,
        "layout": {
            "own": OWN, "foes": FOES, "own_feats": OWN_FEATS, "foe_feats": FOE_FEATS,
            "side_feats": SIDE_FEATS, "pair_feats": PAIR_FEATS, "head_sizes": [int(n) for n in SIDE_HEAD_SIZES],
        },
        "tensors": [[name, [int(d) for d in array.shape]] for name, array in state.items()],
    }
    encoded = json.dumps(header).encode()
    path = Path(checkpoint).with_suffix(".cside")
    with open(path, "wb") as file:
        file.write(b"CSID")
        file.write(struct.pack("<I", len(encoded)))
        file.write(encoded)
        for array in state.values():
            file.write(np.ascontiguousarray(array).tobytes())
    return path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("checkpoints", nargs="+")
    for checkpoint in parser.parse_args().checkpoints:
        print(export(checkpoint))


if __name__ == "__main__":
    main()
