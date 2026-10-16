"""Train callisto ship commanders by reinforcement learning.

The engine runs in Rust (``callisto_rl._engine``): many games stepped at once
on a thread pool. :class:`ShipVecEnv` presents it to Stable-Baselines3 as a
vectorised environment with one entry per ship the network plays, all sharing
one policy. See ``docs/rl_plan.md``.
"""

from ._engine import HEAD_SIZES, MASK_LEN, OBS_LEN, Engine
from .vec_env import ShipVecEnv, default_pool

__all__ = ["Engine", "ShipVecEnv", "default_pool", "HEAD_SIZES", "MASK_LEN", "OBS_LEN"]
