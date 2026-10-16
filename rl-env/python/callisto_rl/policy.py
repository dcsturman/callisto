"""The policy network, and every action head handled in one masked block.

The network is the one Stable-Baselines3 trained (separate 512-512-256 tanh
stacks for the policy and the value, then a linear layer each), so its
checkpoints load here unchanged (:meth:`Policy.from_sb3`).

The heads are of different sizes. Each is padded to the widest, so the
network's outputs become one ``[batch, heads, widest]`` block; padding and
masked-out choices get a very low score; one log-softmax then gives every
head's probabilities at once. Sampling, log-probabilities and entropy are
each a single operation over the block, rather than one distribution per
head.
"""

from __future__ import annotations

from pathlib import Path

import numpy as np
import torch
from torch import nn

from ._engine import HEAD_SIZES, MASK_LEN, OBS_LEN

NEG = -1e8


def mlp(widths: list[int]) -> nn.Sequential:
    layers: list[nn.Module] = []
    for inputs, outputs in zip(widths, widths[1:]):
        layers += [nn.Linear(inputs, outputs), nn.Tanh()]
    return nn.Sequential(*layers)


class Heads:
    """Where each head's choices sit in the outputs and in the padded block."""

    def __init__(self, sizes=HEAD_SIZES):
        self.sizes = list(sizes)
        self.widest = max(self.sizes)
        index = torch.full((len(self.sizes), self.widest), -1, dtype=torch.long)
        at = 0
        for head, size in enumerate(self.sizes):
            index[head, :size] = torch.arange(at, at + size)
            at += size
        self.index = index
        self.valid = index >= 0
        self.flat = index[self.valid]

    def to(self, device) -> "Heads":
        self.index, self.valid, self.flat = self.index.to(device), self.valid.to(device), self.flat.to(device)
        return self

    def block(self, flat: torch.Tensor, fill) -> torch.Tensor:
        """A ``[batch, outputs]`` tensor laid out as ``[batch, heads, widest]``."""
        out = torch.full((flat.shape[0], *self.valid.shape), fill, dtype=flat.dtype, device=flat.device)
        out[:, self.valid] = flat[:, self.flat]
        return out

    def log_probs(self, logits: torch.Tensor, masks: torch.Tensor) -> tuple[torch.Tensor, torch.Tensor]:
        """Every head's log-probabilities, and which choices are open."""
        open_ = self.block(masks, False)
        scores = self.block(logits, NEG).masked_fill(~open_, NEG)
        return torch.log_softmax(scores, dim=-1), open_

    @staticmethod
    def sample(log_probs: torch.Tensor, open_: torch.Tensor, deterministic: bool = False) -> torch.Tensor:
        if deterministic:
            return log_probs.argmax(-1)
        # Gumbel: the argmax of log-probability plus Gumbel noise is a sample.
        noise = -torch.log(-torch.log(torch.rand_like(log_probs).clamp_min(1e-20)))
        return (log_probs + noise).masked_fill(~open_, NEG).argmax(-1)

    @staticmethod
    def log_prob(log_probs: torch.Tensor, actions: torch.Tensor) -> torch.Tensor:
        return log_probs.gather(2, actions.unsqueeze(2)).squeeze(2).sum(1)

    @staticmethod
    def entropy(log_probs: torch.Tensor, open_: torch.Tensor) -> torch.Tensor:
        return -(log_probs.exp() * log_probs).masked_fill(~open_, 0.0).sum((1, 2))


class Policy(nn.Module):
    """The network: a policy and a value, each 512-512-256 with tanh."""

    def __init__(self, hidden=(512, 512, 256)):
        super().__init__()
        self.hidden = list(hidden)
        self.pi = mlp([OBS_LEN, *hidden])
        self.vf = mlp([OBS_LEN, *hidden])
        self.action_net = nn.Linear(hidden[-1], MASK_LEN)
        self.value_net = nn.Linear(hidden[-1], 1)
        self.heads = Heads()

    def to(self, *args, **kwargs):  # keep the head layout on the same device
        module = super().to(*args, **kwargs)
        device = next(self.parameters()).device
        self.heads.to(device)
        return module

    def forward(self, obs: torch.Tensor) -> tuple[torch.Tensor, torch.Tensor]:
        return self.action_net(self.pi(obs)), self.value_net(self.vf(obs)).squeeze(-1)

    def act(self, obs: torch.Tensor, masks: torch.Tensor, deterministic: bool = False):
        """Choices, their log-probability, and the value, for a batch."""
        logits, value = self(obs)
        log_probs, open_ = self.heads.log_probs(logits, masks)
        actions = self.heads.sample(log_probs, open_, deterministic)
        return actions, self.heads.log_prob(log_probs, actions), value

    def evaluate(self, obs: torch.Tensor, masks: torch.Tensor, actions: torch.Tensor):
        """Log-probability of these choices, entropy, and value: for learning."""
        logits, value = self(obs)
        log_probs, open_ = self.heads.log_probs(logits, masks)
        return self.heads.log_prob(log_probs, actions), self.heads.entropy(log_probs, open_), value

    @torch.no_grad()
    def predict(self, obs, action_masks=None, deterministic: bool = False):
        """Stable-Baselines3's interface, for the opponent pool and the arena."""
        device = next(self.parameters()).device
        obs_t = torch.as_tensor(np.asarray(obs), dtype=torch.float32, device=device)
        masks = np.ones((len(obs_t), MASK_LEN), dtype=bool) if action_masks is None else np.asarray(action_masks)
        masks_t = torch.as_tensor(masks, device=device)
        actions, _, _ = self.act(obs_t, masks_t, deterministic)
        return actions.cpu().numpy(), None

    def set_training_mode(self, mode: bool) -> None:
        self.train(mode)

    # -- saving and loading ---------------------------------------------------

    def save(self, path: str | Path, extra: dict | None = None) -> None:
        torch.save({"hidden": self.hidden, "state": self.state_dict(), **(extra or {})}, path)

    @classmethod
    def load(cls, path: str | Path, device="cpu") -> "Policy":
        saved = torch.load(path, map_location=device, weights_only=False)
        policy = cls(saved.get("hidden", (512, 512, 256)))
        policy.load_state_dict(saved["state"])
        return policy.to(device)

    @classmethod
    def from_sb3(cls, path: str | Path, device="cpu") -> "Policy":
        """A Stable-Baselines3 MaskablePPO checkpoint, as this network."""
        from sb3_contrib import MaskablePPO

        sb3 = MaskablePPO.load(path, device="cpu").policy
        policy = cls()
        state = {
            **{f"pi.{k}": v for k, v in sb3.mlp_extractor.policy_net.state_dict().items()},
            **{f"vf.{k}": v for k, v in sb3.mlp_extractor.value_net.state_dict().items()},
            **{f"action_net.{k}": v for k, v in sb3.action_net.state_dict().items()},
            **{f"value_net.{k}": v for k, v in sb3.value_net.state_dict().items()},
        }
        policy.load_state_dict(state)
        return policy.to(device)

    @classmethod
    def from_any(cls, path: str | Path, device="cpu") -> "Policy":
        return cls.from_sb3(path, device) if str(path).endswith(".zip") else cls.load(path, device)
