"""The per-side network: a network trained at eight a side loads at twenty,
its trimmed batches give the same scores as untrimmed ones, and the Rust
port the engine plays gives the same scores as PyTorch."""

import importlib.util
from pathlib import Path

import numpy as np
import torch

from callisto_rl import default_pool
from callisto_rl._engine import SideEngine, side_net_logits
from callisto_rl.sidenet import SideNet
from callisto_rl.vec_env import DESIGNS

HERE = Path(__file__).resolve().parents[2]
CHECKPOINT = HERE / "checkpoints" / "side-4" / "final.pt"


def views(scenarios, games=4):
    eng = SideEngine(default_pool(scenarios), str(DESIGNS), games, threads=2)
    return [torch.as_tensor(v) for v in eng.observe()[:5]]


def net():
    return SideNet.load(CHECKPOINT) if CHECKPOINT.exists() else SideNet()


def test_trimming_changes_nothing():
    """A batch of small fleets scores the same alone as inside a batch with
    a twenty-ship side, which leaves it untrimmed."""
    model = net().eval()
    small = views(["Tessaly Crossing.json"])
    big = views(["Fighter Swarm.json"])
    with torch.no_grad():
        alone, value_alone = model(*small)
        mixed, value_mixed = model(*[torch.cat([s, b]) for s, b in zip(small, big)])
    rows = len(small[0])
    # Only ships that are there: an empty row's single open choice is never scored.
    open_ = small[4].bool() & (small[0][..., 0] > 0.5).unsqueeze(-1)
    assert torch.allclose(value_alone, value_mixed[:rows], atol=1e-4)
    assert torch.allclose(alone[open_], mixed[:rows][open_], atol=1e-4)


def test_trimming_keeps_empty_rows_certain():
    """An empty row's log-probability is 0 whether or not the batch is
    trimmed past it, so playing and learning, which trim differently, agree."""
    model = net().eval()
    small = views(["Tessaly Crossing.json"])
    big = views(["Fighter Swarm.json"])
    actions = torch.zeros(len(small[0]), small[0].shape[1], 15, dtype=torch.long)
    with torch.no_grad():
        alone, _, _ = model.evaluate(*small, actions)
        mixed, _, _ = model.evaluate(*[torch.cat([s, b]) for s, b in zip(small, big)],
                                     torch.cat([actions, torch.zeros(len(big[0]), big[0].shape[1], 15, dtype=torch.long)]))
    assert torch.allclose(alone, mixed[: len(small[0])], atol=1e-3)


def test_rust_port_matches(tmp_path):
    model = net().eval()
    spec = importlib.util.spec_from_file_location("export_side", HERE / "scripts" / "export_side.py")
    export_side = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(export_side)
    path = tmp_path / "net.pt"
    model.save(path)
    cside = export_side.export(str(path))
    for scenario in ["Tessaly Crossing.json", "Gazelle Duel.json"]:
        own, foes, side, pairs, masks = views([scenario], games=2)
        with torch.no_grad():
            ours, _ = model(own, foes, side, pairs, masks)
        for entry in range(len(own)):
            rust = torch.as_tensor(side_net_logits(str(cside), own[entry].numpy(), foes[entry].numpy(), side[entry].numpy(), pairs[entry].numpy()))
            open_ = masks[entry].bool() & (own[entry][:, 0] > 0.5).unsqueeze(-1)
            assert torch.allclose(ours[entry][open_], rust[open_], atol=1e-4), scenario
