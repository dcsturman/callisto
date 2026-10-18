# Training on a rented GPU

How to rent a machine for the 20-ship network, set it up, train on it and
bring the results home. Training starts only after side-5 finishes.

## What to rent

Training is three jobs: the Rust engine plays games on CPU cores, the network
chooses every ship's orders, and the network learns. Measured on the Mac at 8
ships a side (while side-5 trained, so roughly):

| Job | Speed | How it grows to 20 ships a side |
|---|---|---|
| The engine | about 10,000 ship decisions a second per core | barely: 0.08 ms per ship-round at 8 a side, 0.12 at 12 to 20 |
| Choosing orders, on the cpu | about 38,000 a second, the same on 1, 2 or 4 threads | about 2.5 times the work per decision, or more |
| Learning, on the Mac's GPU | about 57,000 a second per pass; 4 passes and a re-score each batch | the same as choosing |

So the growth with fleet size lands on the network, which is GPU work, not on
the engine. A dozen cores is plenty; 16 is comfortable. 30 is more than we
need. Before renting, choosing orders moves onto the GPU too, since at 20
ships the cpu would hold it back.

| Option | GPU | CPUs | Price an hour | For |
|---|---|---|---|---|
| **Lambda, 1× A10** (recommended) | A10, 24 GB | 30 | about $0.75 | CUDA PyTorch installed, billed by the minute, and an API to launch and terminate unattended |
| RunPod pod, RTX 4090 | 4090, 24 GB | choose 16 or more | $0.34 community, $0.69 secure | cheaper, also has an API |
| Vast.ai, RTX 4090 | 4090, 24 GB | varies by host | $0.15 to $0.50 | cheapest; interruptible offers can stop at any time |

Lambda's extra cores are wasted, but its price is close to a RunPod 4090's,
and it is the simplest to run unattended. A RunPod community 4090 with 16
vCPUs at about $0.34 halves the cost if that matters more. Prices were
checked in October 2026; the console shows the current one before launch.

**Budget.** side-4, 40M decisions, took 2 hours 40 minutes on the Mac. Expect
10 to 20 hours of machine time for a first 20-ship run, $8 to $15. Set a $50
limit to start; the first hour's speed turns this into a number.

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

**When it finishes, terminate the instance.** Lambda bills every minute an
instance exists, idle or not; a forgotten A10 costs about $18 a day.

- **Terminate is the only way to stop paying.** Lambda instances cannot be
  stopped or paused, only launched, restarted or terminated.
- **No wiping.** Terminating deletes the instance and its disk; nothing is
  left to clean. So copy the checkpoints home first, or keep them on a
  filesystem.
- **Shutting down from inside does not stop billing.** `sudo shutdown`
  leaves the instance in an alert state, still billed. Terminate from the
  console or the API.

## Unattended runs, overnight

The Mac can run the whole thing with Lambda's API: launch, set up, train,
copy the network and results home, terminate, and check that nothing is
left running.

- **A Lambda API key,** made in the dashboard under API keys, kept at
  `~/.config/callisto/lambda_api_key` with only you able to read it, like
  the Anthropic key. It is never printed.
- **A hard deadline.** Whatever happens, the instance is terminated when the
  run's time budget is spent, finished or not, after copying what exists.
- **The Mac must stay awake and online,** held by `caffeinate`, since it is
  the one that terminates. If it sleeps or loses its connection, the
  instance keeps billing.
- **A backup on the machine itself:** a timer that terminates the instance
  through the API a little after the deadline. This needs the API key on the
  rented machine for the run, which can launch instances too; it is deleted
  with the machine. A billing alert in the Lambda console is the last line.

The API calls: `POST /api/v1/instance-operations/launch`,
`POST /api/v1/instance-operations/terminate` and `GET /api/v1/instances`,
each with `Authorization: Bearer <key>`.

## Before the first rental

Work that needs no GPU, done on the Mac while side-5 trains:

- the network's buffers raised from 8 to about 20 ships a side, with each
  batch padded only to its largest fleet;
- a second fire target, so heavy mounts and turrets can aim at different
  ships;
- random variation in each game's starting positions;
- a round limit of 60 to 80 for big scenarios;
- the firepower reward capped at a target's hull, so overkill counts nothing;
- choosing orders on the GPU, not the cpu;
- the unattended controller: launch, train, copy home, terminate, with the
  deadline and the backup timer;
- a short run on the machine's first hour to measure its speed, so the
  budget above becomes a number.
