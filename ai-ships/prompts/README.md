# Prompt experiments

A model commander can be given a system prompt of its own, by naming it
`model@path`: `deepseek@prompts/deepseek.md` plays DeepSeek with this
directory's `deepseek.md` in place of the built-in `src/prompt/system.md`.
Plain `gemini` or `deepseek` keeps the built-in prompt, so one model can be
tried on a new prompt while the other stays the yardstick.

The file is read when the commander is made, so an edit takes effect on the
next game with no rebuild. The stealth and hunting sections, and every
round's situation, are shared: only the fixed part is replaced.

To compare, run the harness with each model on each side, for example:

    cargo run --release --bin harness -- --scenario "../callisto/scenarios/Tessaly Crossing.json" \
      --side Red=deepseek@prompts/deepseek.md --side Blue=gemini --games 4

and again with the sides swapped. The summary reports wins, hull kept and
what each side's model calls cost.
