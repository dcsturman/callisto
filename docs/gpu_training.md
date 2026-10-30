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

## RunPod against Lambda

| | RunPod, RTX 4090 pod | Lambda, 1× A10 |
|---|---|---|
| Price an hour | $0.34 community, $0.69 secure | about $0.75 |
| GPU | RTX 4090: roughly two to three times an A10 for this work | A10 |
| CPUs | you choose; filter for 16 or more (the cheapest offers have 8) | 30, fixed |
| Billing | by the second | by the minute |
| Stopping | stop (pay only for the volume disk) or terminate | terminate only |
| Machine | a container with CUDA PyTorch; you are root | a full Ubuntu machine with CUDA PyTorch |
| Payment | prepaid credit; needs at least an hour's worth to launch | card, billed after |
| Unattended | REST API, a command-line tool and a Python SDK: create, stop, terminate | REST API: launch, terminate |
| Catches | community hosts vary, and a community pod's IP can change on restart | availability can be thin |

**RunPod is better for us.** For the same money or less it has the faster
GPU, it bills by the second, and its API does everything an unattended run
needs. Use **Secure Cloud** for unattended runs, at about $0.69: steady hosts
and a fixed IP, at Lambda's price with a faster GPU. **Community Cloud**, at
about $0.34, halves that for runs you watch. Lambda is the fallback if
RunPod has no 4090 with 16 cores free. Prices were checked in October 2026;
the console shows the current one before launch.

**Budget.** side-4, 40M decisions, took 2 hours 40 minutes on the Mac. Expect
10 to 20 hours of machine time for a first 20-ship run: $7 to $14 on secure,
half that on community. Load $25 of credit to start; the first hour's speed
turns the estimate into a number.

## Accounts, once

1. **RunPod.** Sign up at runpod.io and add $25 of credit under Billing. Its
   default spending cap is $80 an hour across everything; lower it under
   Settings if you like.
2. **Your SSH key.** Settings, SSH Public Keys: paste your public key, copied
   with `pbcopy < ~/.ssh/id_ed25519.pub`. It is served by 1Password's agent,
   so connecting asks 1Password to approve, as your commits do.
3. **A RunPod API key,** for unattended runs: Settings, API Keys, create one,
   and save it to `~/.config/callisto/runpod_api_key` with only you able to
   read it, like the Anthropic key. It is never printed.
4. **A network volume, optional:** about 20 GB, about $1.40 a month. It
   keeps checkpoints even if a pod is terminated before they are copied
   home. We copy everything home anyway.
5. **W&B.** Nothing new: on the pod, `wandb login` with the key from
   wandb.ai/authorize. Runs go to `callistoflight/callisto-rl` as now.
6. **GitHub.** Nothing: the repository is public.
7. **No model keys.** The Gemini and Claude benchmarks stay on the Mac;
   nothing that costs money per call goes on the pod.

## Each run

Deploy a pod: Pods, Deploy, an RTX 4090 with 16 or more vCPUs, the RunPod
PyTorch template, TCP port 22 exposed for SSH. The pod's Connect menu gives
the SSH address and port. Then:

```
# copy the setup script and the starting checkpoints over
scp -P <port> rl-env/scripts/cloud_setup.sh root@<ip>:
ssh -p <port> root@<ip> bash cloud_setup.sh
rsync -avz -e "ssh -p <port>" rl-env/checkpoints/teachers rl-env/checkpoints/side-5 root@<ip>:callisto/rl-env/checkpoints/

# on the pod: sign in to W&B and start training inside tmux, so it survives
# the connection dropping
ssh -p <port> root@<ip>
cd callisto/rl-env && .venv/bin/wandb login
tmux new -s train
.venv/bin/python scripts/side_ppo.py --init-from checkpoints/side-5/final.pt --threads 14 \
  --name side-20-1 --wandb ...   # the run's settings, decided when we get there
# detach with ctrl-b then d; reattach later with: tmux attach -t train
```

Watch it in W&B as usual. To bring results home while it runs, or after:

```
rsync -avz -e "ssh -p <port>" root@<ip>:callisto/rl-env/checkpoints/side-20-1 rl-env/checkpoints/
```

**When it finishes, terminate the pod.**

- **Terminate stops all charges and deletes the pod and its disks.** No
  wiping: there is nothing left to clean. Copy the checkpoints home first,
  or keep them on a network volume.
- **Stop is the in-between.** It stops the GPU charge but keeps the volume
  disk, at $0.20 a GB a month, and the same GPU may not be free when you
  start it again. For us, terminate.
- **Shutting down from inside is not stopping.** Use the console, the
  command-line tool or the API.
- **An empty balance stops pods on its own,** and terminates those without a
  network volume. Prepaid credit is a hard ceiling on what a mistake costs.

## Unattended runs, overnight

`rl-env/scripts/cloud_train.sh` runs a training on a pod and brings it home;
`rl-env/scripts/pod_supervise.sh` runs on the pod and removes it afterwards.

1. **Create the pod** (Claude does this through the RunPod plugin): Secure
   Cloud, the PyTorch 2.8 CUDA 12.8 image, SSH on port 22, and at least 16
   vCPUs. Note its SSH address and port.
2. **Start the controller on the Mac**, with the run's name, a time limit in
   hours, and the training's settings:

   ```
   cd rl-env
   nohup caffeinate -i scripts/cloud_train.sh <ip> <port> side-20-2 3 \
     --init-from checkpoints/side-20-1/final.pt --games 256 --groups 2 --threads 14 \
     ...the run's settings... --wandb > /dev/null 2>&1 &
   ```

   It sets the pod up and copies the starting networks over (`UPLOAD`).
3. **Sign in to W&B on the pod** when it asks; it prints the two commands:

   ```
   ssh -i ~/.ssh/id_ed25519 -p <port> root@<ip>
   cd callisto/rl-env && .venv/bin/wandb login --host https://api.forge.coreweave.com
   ```

   It waits for that, then starts training, and its log
   (`rl-env/logs/cloud-<run name>.log`) says so.
4. **Nothing else.** Every ten minutes the Mac copies the run's checkpoints and
   charts home (`checkpoints/<run name>`, `runs/<run name>`). When training
   ends, or the time limit passes, it makes a last copy and tells the pod,
   which removes itself.

**If the Mac sleeps or goes offline,** the pod still removes itself 45 minutes
after training stops, with whatever the Mac copied before. It uses RunPod's own
tool and the key RunPod gives every pod; no account API key is needed. The
credit balance is the last line: at $0 RunPod stops the pod.

Tested end to end on a 4090 pod (18 October 2026): set up, trained, copied
home, and the pod removed itself.

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
- a short run on the machine's first hour to measure its speed, so the
  budget above becomes a number.

## Run: side-cap-1 (capital ships and carriers)

The first run on the step-12 layout: the capital-ship and carrier inputs and the
`Flight` head (`docs/capital_ships_plan.md`). It starts from side-20-9 widened to
that layout (`checkpoints/side-20-9-wide/final.pt`, made by `scripts/widen.py`;
it plays Fighter Screen 20–0 against `close` from each seat, as side-20-9 does),
and trains on `CAPITAL_POOL`: the side pool plus twelve generated fights
(`gen:capital`, `gen:carrier`, `gen:fleet`, four seeds each). Settings are
side-20-9's, with side-20-9 added as a teacher.

```
cd rl-env
UPLOAD="checkpoints/side-20-9-wide/final.pt checkpoints/side-20-9/side_120017863_decisions.pt checkpoints/side-20-8/final.pt checkpoints/side-5/final.pt" \
nohup caffeinate -i scripts/cloud_train.sh <ip> <port> side-cap-1 8 \
  --init-from checkpoints/side-20-9-wide/final.pt --steps 120000000 --games 512 --rollout 8 --groups 2 --threads 14 \
  --teachers "0.15=pt~:checkpoints/side-20-9/side_120017863_decisions.pt,0.10=pt~:checkpoints/side-20-8/final.pt,0.05=pt~:checkpoints/side-5/final.pt" \
  --baselines-start 0.12 --baselines-end 0.12 --baselines close,hold,charger,kiter --hold-bonus 0.005 \
  --destroy-goals 1.0 --random-goals 0 --firepower-bonus 0.2 --random-starts 0.3 --max-rounds 60 \
  --learning-rate 0.00015 --final-lr-fraction 0.0 --entropy 0.01 --wandb > /dev/null 2>&1 &
```

`--threads 14` is for a 16-vCPU pod; raise it to the pod's vCPUs less two. 512
games × rollout 8 keeps about 4,000 samples per update. The 8-hour limit allows
for the generated fights being slower than the old pool.

A smoke run on the Mac (400,000 decisions, 32 games × rollout 128, 6 threads)
trained, exported and loaded in the AI service.

**After it, before shipping it:** export the last checkpoint and check
- Fighter Screen, Thebus and Aslan Border Clash: at least side-20-9's results
  (Fighter Screen 20–0 against `close` from each seat);
- Keelhaven and the generated fights: better than the crews' heuristic
  (`--side Blue=network --side Red=close` and the reverse), with carriers
  launching.
