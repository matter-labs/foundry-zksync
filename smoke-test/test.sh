#!/usr/bin/env bash

# Fail fast and on piped commands
set -o pipefail -e

SOLC_VERSION=${SOLC_VERSION:-"v0.8.20"}
SOLC="solc-${SOLC_VERSION}"
ERA_TEST_NODE_VERSION="v0.1.0-alpha.15"
ERA_TEST_NODE_PID=0
BINARY_PATH="../target/release/zkforge"

function cleanup() {
  echo "Cleaning up..."
  stop_era_test_node
  rm -f "./${SOLC}"
}

function success() {
  echo ''
  echo '================================='
  printf "\e[32m> [SUCCESS]\e[0m\n"
  echo '================================='
  echo ''
  cleanup
  exit 0
}

function fail() {
  echo "Displaying run.log..."
  cat run.log
  echo ''
  echo '=================================='
  printf "\e[31m> [FAILURE]\e[0m %s\n" "$1"
  echo '=================================='
  echo ''
  cleanup
  exit 1
}

function download_solc() {
  wget --quiet -O "${SOLC}" "https://github.com/ethereum/solidity/releases/download/${1}/solc-static-linux"
  chmod +x "${SOLC}"
}

function download_era_test_node() {
  local arch
  case "$(uname -s)" in
  Darwin*) arch="apple-darwin" ;;
  *) arch="unknown-linux-gnu" ;;
  esac
  wget --quiet -O "era_test_node.tar.gz" "https://github.com/matter-labs/era-test-node/releases/download/${1}/era_test_node-${1}-x86_64-${arch}.tar.gz"
  tar -xvf "era_test_node.tar.gz" && rm "era_test_node.tar.gz"
  chmod +x "era_test_node"
}

function wait_for_build() {
  local timeout=$1
  while ! [ -x "${BINARY_PATH}" ]; do
    ((timeout--))
    if [ $timeout -le 0 ]; then
      echo "Build timed out waiting for binary to be created."
      exit 1
    fi
    sleep 1
  done
}

# We want this to fail-fast and hence are put on separate lines
# See https://unix.stackexchange.com/questions/312631/bash-script-with-set-e-doesnt-stop-on-command
function build_zkforge() {
  echo "Building..."
  cargo build --manifest-path="../Cargo.toml" --release
  wait_for_build 30
}

function stop_era_test_node() {
  echo "Stopping era-test-node..."
  if [ ${ERA_TEST_NODE_PID} -ne 0 ]; then
    kill -9 "${ERA_TEST_NODE_PID}"
  fi;
  ERA_TEST_NODE_PID=0
  sleep 3
}

function start_era_test_node() {
  echo "Starting era-test-node..."
  ./era_test_node run &
  ERA_TEST_NODE_PID=$!
  sleep 3
}

trap cleanup ERR

echo "Solc: ${SOLC_VERSION}"
echo "Zkforge binary: ${BINARY_PATH}"
echo "era-test-node: ${ERA_TEST_NODE_VERSION}"

# Download solc
download_solc "${SOLC_VERSION}"

# Download era test node
download_era_test_node "${ERA_TEST_NODE_VERSION}"

# Check for necessary tools
command -v cargo &>/dev/null || {
  echo "cargo not found, exiting"
  exit 1
}
command -v git &>/dev/null || {
  echo "git not found, exiting"
  exit 1
}

build_zkforge

echo "Running tests..."

echo "[1] Check test suite passed"
RUST_LOG=debug "${BINARY_PATH}" test --use "./${SOLC}" --match-test 'test_Increment' &>run.log || fail "zkforge test failed"

echo "[2] Check console logs are printed in era-test-node"
grep '\[INT-TEST\] PASS' run.log &>/dev/null || fail "zkforge test console output failed"

echo "[3] Check asserts fail tests"
set +e
if RUST_LOG=debug "${BINARY_PATH}" test --use "./${SOLC}" --match-test 'test_FailIncrement' &>run.log; then
  fail "zkforge test did not fail"
fi

echo "[4] Check testFail works"
RUST_LOG=debug "${BINARY_PATH}" test --use "./${SOLC}" --match-test 'testFail_Increment' &>run.log || fail "zkforge testFail failed"

echo "[5] Check fuzz test works"
RUST_LOG=debug "${BINARY_PATH}" test --use "./${SOLC}" --match-test 'testFuzz_Increment' &>run.log || fail "zkforge fuzz test failed"

echo "[6] Check invariant test works"
RUST_LOG=debug "${BINARY_PATH}" test --use "./${SOLC}" --match-test 'invariant_alwaysIncrements' &>run.log || fail "zkforge invariant test failed"

echo "Running scripts..."

echo "[1] Contract transacts"
start_era_test_node
RUST_LOG=info "${BINARY_PATH}" script ./script/Counter.s.sol:CounterScript --broadcast --private-key "0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e" \
  --chain 260 --gas-estimate-multiplier 310 --rpc-url http://localhost:8011 --use "./${SOLC}"  &>run.log || fail "zkforge script transact failed"
stop_era_test_node

echo "[2] Contract deploys (once)"
start_era_test_node
RUST_LOG=info "${BINARY_PATH}" script ./script/NFT.s.sol:NFTScript --broadcast --private-key "0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e" \
  --chain 260 --gas-estimate-multiplier 310 --rpc-url http://localhost:8011 --use "./${SOLC}"  &>run.log || fail "zkforge script deploy (once) failed"
echo "[3] Contract deploys (twice)"
RUST_LOG=info "${BINARY_PATH}" script ./script/NFT.s.sol:NFTScript --broadcast --private-key "0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e" \
  --chain 260 --gas-estimate-multiplier 310 --rpc-url http://localhost:8011 --use "./${SOLC}"  &>run.log || fail "zkforge script deploy (twice) failed"
stop_era_test_node


success
