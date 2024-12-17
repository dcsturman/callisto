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

cargo test

# Run the server a few times to get coverage of command line args
export RUST_LOG=info
echo "Running server a few times to get coverage of command line args. Logging = info"
echo "Test help"
./target/debug/callisto --help
echo "Test version"
./target/debug/callisto --version
echo "Test long flags"
cargo run -- --port 4100  --scenario_file ./tests/test_scenario.json --design_file ./tests/test_ship_templates.json --users_file ./config/authorized_users.json --web-server http://localhost:50001 &
echo "Test long test flag"
cargo run -- --port 4101  --test &

echo "Kill all the test servers."
kill %1 %2

cargo llvm-cov report
cargo llvm-cov report --lcov --output-path=coverage/tests.lcov

