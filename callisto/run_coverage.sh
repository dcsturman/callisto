#!/bin/zsh
# Set the environment variables needed to get coverage.
source <(cargo llvm-cov show-env --export-prefix)
# Remove artifacts that may affect the coverage results.
# This command should be called after show-env.
cargo llvm-cov clean --workspace
# Above two commands should be called before build binaries.

cargo build # Build rust binaries.
# Commands using binaries in target/debug/*, including `cargo test` and other cargo subcommands.
# ...

cargo llvm-cov report
