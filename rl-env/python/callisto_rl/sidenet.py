"""The per-side network: attention over every ship, orders for every ship.

Each of the side's ships, each enemy, and the side itself is a token. Three
attention layers let every token take in every other; between one of our
ships and one enemy, the pair table (range, contact, lock, the odds each way,
detection odds, last round's fire) is turned into a bias on how much they
attend to each other, so attention knows the fight between them, not only
where they are.

Each own ship's final token gives its ordinary heads directly. Its target
heads point: the helm target, fire, lock and the second operator's target
score her token against each enemy's, with their pair row; "none" has a score
of its own; the guide scores her against each of her friends. The value is
read from the side token.

Choices, log-probabilities and entropy use the same masked block as the
per-ship policy (:mod:`callisto_rl.policy`), one block per ship.
"""

from __future__ import annotations

from pathlib import Path

import numpy as np
import torch
from torch import nn

from ._engine import (
    FOE_FEATS,
    FOES,
    OWN,
    OWN_FEATS,
    PAIR_FEATS,
    SIDE_FEATS,
    SIDE_HEAD_SIZES,
    SIDE_MASK_LEN,
)
from .policy import NEG, Heads

# Head order, as the engine numbers them.
VERB, HELM_TARGET, BAND, GUIDE, FIRE, LOCK, POSTURE, DODGE, ASSIST, JUMP, SENSOR1, SENSOR2, TARGET2, CAPTAIN, COMPUTER = range(15)
PLAIN = [VERB, BAND, POSTURE, DODGE, ASSIST, JUMP, SENSOR1, SENSOR2, CAPTAIN, COMPUTER]
AT_FOES = [HELM_TARGET, FIRE, LOCK, TARGET2]  # FIRE, LOCK, TARGET2 also have "none" first
WITH_NONE = {FIRE, LOCK, TARGET2}


class Layer(nn.Module):
    """Attention with a bias between tokens, then a feed-forward block."""

    def __init__(self, width: int, heads: int, hidden: int):
        super().__init__()
        self.heads = heads
        self.qkv = nn.Linear(width, 3 * width)
        self.out = nn.Linear(width, width)
        self.norm1 = nn.LayerNorm(width)
        self.norm2 = nn.LayerNorm(width)
        self.ff = nn.Sequential(nn.Linear(width, hidden), nn.GELU(), nn.Linear(hidden, width))

    def forward(self, x: torch.Tensor, bias: torch.Tensor, keep: torch.Tensor) -> torch.Tensor:
        b, n, w = x.shape
        h = self.heads
        q, k, v = self.qkv(self.norm1(x)).view(b, n, 3, h, w // h).permute(2, 0, 3, 1, 4)
        scores = q @ k.transpose(-1, -2) / (w // h) ** 0.5 + bias
        scores = scores.masked_fill(~keep[:, None, None, :], NEG)
        attended = (scores.softmax(-1) @ v).transpose(1, 2).reshape(b, n, w)
        x = x + self.out(attended)
        return x + self.ff(self.norm2(x))


class SideNet(nn.Module):
    def __init__(self, width: int = 128, heads: int = 4, layers: int = 3, hidden: int = 256, pair_width: int = 64):
        super().__init__()
        self.config = dict(width=width, heads=heads, layers=layers, hidden=hidden, pair_width=pair_width)
        self.heads_n = heads
        self.own_in = nn.Sequential(nn.Linear(OWN_FEATS, width), nn.GELU(), nn.Linear(width, width))
        self.foe_in = nn.Sequential(nn.Linear(FOE_FEATS, width), nn.GELU(), nn.Linear(width, width))
        self.side_in = nn.Sequential(nn.Linear(SIDE_FEATS, width), nn.GELU(), nn.Linear(width, width))
        self.pair_in = nn.Sequential(nn.Linear(PAIR_FEATS, pair_width), nn.GELU(), nn.Linear(pair_width, pair_width))
        self.pair_bias = nn.Linear(pair_width, 2 * heads)  # own->foe and foe->own, per attention head
        self.layers = nn.ModuleList(Layer(width, heads, hidden) for _ in range(layers))
        self.norm = nn.LayerNorm(width)
        self.plain_sizes = [SIDE_HEAD_SIZES[h] for h in PLAIN]
        self.plain = nn.Linear(width, sum(self.plain_sizes))
        self.point = nn.Sequential(nn.Linear(2 * width + pair_width, hidden), nn.GELU(), nn.Linear(hidden, len(AT_FOES)))
        self.none = nn.Linear(width, len(WITH_NONE))
        self.guide_q = nn.Linear(width, width)
        self.guide_k = nn.Linear(width, width)
        self.value = nn.Sequential(nn.Linear(width, hidden), nn.GELU(), nn.Linear(hidden, 1))
        self.layout = Heads(SIDE_HEAD_SIZES)
        # Each head's first choice, where an empty row's mask is open.
        self.first_choices = self.layout.index[:, 0].clone()

    def to(self, *args, **kwargs):
        module = super().to(*args, **kwargs)
        self.layout.to(next(self.parameters()).device)
        return module

    def forward(self, own, foes, side, pairs, masks):
        """Logits for every ship (``[entries, OWN, SIDE_MASK_LEN]``) and the
        side's value (``[entries]``).

        Rows are filled from the front, so the batch is first trimmed to its
        largest fleet on each side: attention and the pair table then cost
        what the ships present need, not what twenty a side would. Rows past
        the trim get no scores; their masks open only the first choice."""
        b = own.shape[0]
        own_present, foe_present = own[..., 0] > 0.5, foes[..., 0] > 0.5

        def filled(present: torch.Tensor) -> int:
            rows = present.any(0).nonzero()
            return int(rows.max().item()) + 1 if len(rows) else 1

        k, kf = filled(own_present), filled(foe_present)
        own, foes, pairs = own[:, :k], foes[:, :kf], pairs[:, :k, :kf]
        own_present, foe_present = own_present[:, :k], foe_present[:, :kf]
        side_token = self.side_in(side).unsqueeze(1)
        x = torch.cat([self.own_in(own), self.foe_in(foes), side_token], dim=1)
        keep = torch.cat([own_present, foe_present, torch.ones(b, 1, dtype=torch.bool, device=own.device)], dim=1)
        pair = self.pair_in(pairs)  # [b, k, kf, pw]
        bias_of = self.pair_bias(pair)  # [b, k, kf, 2h]
        n = k + kf + 1
        bias = torch.zeros(b, self.heads_n, n, n, device=own.device)
        h = self.heads_n
        bias[:, :, :k, k : k + kf] = bias_of[..., :h].permute(0, 3, 1, 2)
        bias[:, :, k : k + kf, :k] = bias_of[..., h:].permute(0, 3, 2, 1)
        for layer in self.layers:
            x = layer(x, bias, keep)
        x = self.norm(x)
        mine, theirs, whole = x[:, :k], x[:, k : k + kf], x[:, -1]

        logits = torch.full((b, OWN, SIDE_MASK_LEN), NEG, device=own.device)
        index = self.layout.index
        # Rows past the trim hold no ship, and their masks open only each
        # head's first choice. Scoring that choice 0 makes it certain, as an
        # empty row's choice is however the batch was trimmed: left at NEG
        # with the rest, it would share its probability with the closed ones.
        logits[:, k:, self.first_choices] = 0.0
        plain = self.plain(mine)
        at = 0
        for head, size in zip(PLAIN, self.plain_sizes):
            logits[:, :k, index[head, :size]] = plain[..., at : at + size]
            at += size
        joined = torch.cat(
            [mine.unsqueeze(2).expand(-1, -1, kf, -1), theirs.unsqueeze(1).expand(-1, k, -1, -1), pair], dim=-1
        )
        pointed = self.point(joined)  # [b, k, kf, 4]
        none = self.none(mine)  # [b, k, 3]
        for j, head in enumerate(AT_FOES):
            if head in WITH_NONE:
                logits[:, :k, index[head, 0]] = none[..., sorted(WITH_NONE).index(head)]
                logits[:, :k, index[head, 1 : 1 + kf]] = pointed[..., j]
            else:
                logits[:, :k, index[head, :kf]] = pointed[..., j]
        guide = self.guide_q(mine) @ self.guide_k(mine).transpose(1, 2) / mine.shape[-1] ** 0.5
        logits[:, :k, index[GUIDE, :k]] = guide
        return logits, self.value(whole).squeeze(-1)

    def _dist(self, logits, masks):
        b = logits.shape[0]
        flat_logits = logits.reshape(b * OWN, SIDE_MASK_LEN)
        flat_masks = masks.reshape(b * OWN, SIDE_MASK_LEN)
        log_probs, open_ = self.layout.log_probs(flat_logits, flat_masks)
        return log_probs, open_

    def act(self, own, foes, side, pairs, masks, deterministic: bool = False):
        logits, value = self(own, foes, side, pairs, masks)
        log_probs, open_ = self._dist(logits, masks)
        actions = Heads.sample(log_probs, open_, deterministic)  # [b*OWN, H]
        b = own.shape[0]
        logp = Heads.log_prob(log_probs, actions).view(b, OWN).sum(1)
        return actions.view(b, OWN, -1), logp, value

    def evaluate(self, own, foes, side, pairs, masks, actions, per_ship: bool = False):
        """Log-probability and entropy of each entry's actions, summed over its
        ships, or each ship's own (``[entries, OWN]``) with `per_ship`; and
        the side's value."""
        logits, value = self(own, foes, side, pairs, masks)
        log_probs, open_ = self._dist(logits, masks)
        b = own.shape[0]
        flat_actions = actions.reshape(b * OWN, -1)
        logp = Heads.log_prob(log_probs, flat_actions).view(b, OWN)
        entropy = Heads.entropy(log_probs, open_).view(b, OWN)
        if per_ship:
            return logp, entropy, value
        return logp.sum(1), entropy.sum(1), value

    @torch.no_grad()
    def predict(self, views, deterministic: bool = False):
        tensors = [torch.as_tensor(np.asarray(v)) for v in views]
        actions, _, _ = self.act(*tensors, deterministic=deterministic)
        return actions.numpy()

    def set_training_mode(self, mode: bool) -> None:
        self.train(mode)

    def save(self, path: str | Path, extra: dict | None = None) -> None:
        torch.save({"kind": "side", "config": self.config, "state": self.state_dict(), **(extra or {})}, path)

    @classmethod
    def load(cls, path: str | Path, device="cpu") -> "SideNet":
        saved = torch.load(path, map_location=device, weights_only=False)
        net = cls(**saved["config"])
        net.load_state_dict(saved["state"])
        return net.to(device)
