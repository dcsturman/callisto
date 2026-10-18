# Training on a rented GPU

How to rent a machine for the 20-ship network, set it up, train on it and
bring the results home. Training starts only after side-5 finishes.

## What to rent

Our training is two jobs at once: playing games, which is the Rust engine on
CPU cores, and learning, which is the network on the GPU. The 20-ship network
makes learning heavier, which is why we want a GPU, but games still need many
cores. So pick a machine by CPU count as much as by GPU.

| Option | GPU | CPUs | Price an hour | For |
|---|---|---|---|---|
| **Lambda, 1× A10** (recommended) | A10, 24 GB | 30 | about $0.75 | many cores, a full Ubuntu machine with CUDA PyTorch installed, billed by the minute |
| RunPod pod, RTX 4090 | 4090, 24 GB | varies, filter for 24 or more | $0.34 community, $0.69 secure | cheaper GPU; check the CPU count of each offer |
| Vast.ai, RTX 4090 | 4090, 24 GB | varies by host | $0.15 to $0.50 | cheapest; interruptible offers can stop at any time |

Prices were checked in October 2026 and move; the console shows the current
one before you launch. Any of the three works. Lambda is the simplest: one
fixed shape with 30 cores, nothing to filter, and the software we need already
installed. If Lambda has no A10 free, a 1× A100 there (30 CPUs, $1.30 to $2.00)
or a RunPod 4090 with 24 or more vCPUs is the fallback.

**Budget.** side-4, 40M decisions, took 2 hours 40 minutes on the Mac. The
20-ship network does several times the work per big game, and will want more
decisions. Expect 10 to 20 hours of machine time for a first 20-ship run,
$8 to $15 on Lambda. Set a $50 limit to start.

## Accounts, once

1. **Lambda.** Sign up at lambda.ai, add a card under Billing, and set a
   spending alert.
2. **An SSH key on Lambda.** Settings, SSH keys, Add: paste your public key.
   Yours is `~/.ssh/id_ed25519.pub`, served by 1Password's agent; copy it with
   `pbcopy < ~/.ssh/id_ed25519.pub`. Connecting will then ask 1Password to
   approve, as your commits do.
3. **A filesystem on Lambda,** about 20 GB, in the region you will launch
   in. It keeps checkpoints if an instance is terminated before you copy them
   home, for about $4 a month. Optional: we also copy everything home.
4. **W&B.** Nothing new: on the machine, `wandb login` with the key from
   wandb.ai/authorize. Runs go to `callistoflight/callisto-rl` as now.
5. **GitHub.** Nothing: the repository is public.
6. **No model keys.** The Gemini and Claude benchmarks stay on the Mac;
   nothing that costs money per call goes on the rented machine.

## Each run

On the Mac, launch a 1× A10 in the Lambda console (attach the filesystem if
you made one), and note its IP address. Then:

```
# copy the setup script and the starting checkpoints over
scp rl-env/scripts/cloud_setup.sh ubuntu@<ip>:
ssh ubuntu@<ip> bash cloud_setup.sh
rsync -avz rl-env/checkpoints/teachers rl-env/checkpoints/side-5 ubuntu@<ip>:callisto/rl-env/checkpoints/

# on the machine: sign in to W&B and start training inside tmux, so it
# survives the connection dropping
ssh ubuntu@<ip>
cd callisto/rl-env && .venv/bin/wandb login
tmux new -s train
.venv/bin/python scripts/side_ppo.py --init-from checkpoints/side-5/final.pt --threads 28 \
  --name side-20-1 --wandb ...   # the run's settings, decided when we get there
# detach with ctrl-b then d; reattach later with: tmux attach -t train
```

Watch it in W&B as usual. To bring results home while it runs, or after:

```
rsync -avz ubuntu@<ip>:callisto/rl-env/checkpoints/side-20-1 rl-env/checkpoints/
```

**When it finishes, terminate the instance in the console.** Lambda bills
every minute an instance exists, idle or not; a forgotten A10 costs about
$18 a day. Copy the checkpoints home first, or keep them on the filesystem.

## Before the first rental

Work that needs no GPU, done on the Mac while side-5 trains:

- the network's buffers raised from 8 to about 20 ships a side, with each
  batch padded only to its largest fleet;
- a second fire target, so heavy mounts and turrets can aim at different
  ships;
- random variation in each game's starting positions;
- a round limit of 60 to 80 for big scenarios;
- the firepower reward capped at a target's hull, so overkill counts nothing;
- a short run on the machine's first hour to measure its speed, so the
  budget above becomes a number.
