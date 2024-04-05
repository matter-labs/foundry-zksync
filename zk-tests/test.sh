#!/usr/bin/env bash

# Fail fast and on piped commands
set -o pipefail -e

REPO_ROOT="../"
SOLC_VERSION=${SOLC_VERSION:-"v0.8.20"}
SOLC="solc-${SOLC_VERSION}"
BINARY_PATH="${REPO_ROOT}/target/release/forge"
ERA_TEST_NODE_VERSION="v0.1.0-alpha.15"
ERA_TEST_NODE_PID=0

function cleanup() {
  echo "Cleaning up..."
  stop_era_test_node
  cleanup() {

  # Force stop era test node
  # era-test-node port
  PORT=8011

  # Using lsof to find the PID of the process using the specified port
  PID=$(lsof -ti:$PORT)

  # If a PID is found, kill the process
  if [ ! -z "$PID" ]; then
    echo "Killing process on port $PORT with PID $PID"
    kill -9 $PID
  else
    echo "No process found on port $PORT"
  fi
}

  rm -f "./${SOLC}"
  rm -rf "./lib/solmate"
  rm -rf "./lib/openzeppelin-contracts"
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
  echo ''
  echo '=================================='
  printf "\e[31m> [FAILURE]\e[0m %s\n" "$1"
  echo '=================================='
  echo ''
  cleanup
  exit 1
}

function download_solc() {
  case "$(uname -s)" in
  Darwin*) arch=macos ;;
  *) arch=static-linux ;;
  esac
  if [ ! -x "${SOLC}" ]; then
    wget --quiet -O "${SOLC}" "https://github.com/ethereum/solidity/releases/download/${1}/solc-${arch}"
    chmod +x "${SOLC}"
  fi
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
function build_forge() {
  echo "Building ${1}..."
  cargo build --release --manifest-path="${1}/Cargo.toml"
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
echo "forge binary: ${BINARY_PATH}"
echo "era-test-node: ${ERA_TEST_NODE_VERSION}"

# Download solc
download_solc "${SOLC_VERSION}"

# Download era test node
download_era_test_node "${ERA_TEST_NODE_VERSION}"

forge install transmissions11/solmate Openzeppelin/openzeppelin-contracts --no-commit

# Check for necessary tools
command -v cargo &>/dev/null || {
  echo "cargo not found, exiting"
  exit 1
}
command -v git &>/dev/null || {
  echo "git not found, exiting"
  exit 1
}

build_forge "${REPO_ROOT}"

echo "Running tests..."
RUST_LOG=warn "${BINARY_PATH}" test --use "./${SOLC}" -vvv  || fail "forge test failed"

echo "Running tests with '--zksync'..."
RUST_LOG=warn "${BINARY_PATH}" test --use "./${SOLC}" -vvv --zksync  || fail "forge test --zksync failed"

echo "Running script..."
start_era_test_node
RUST_LOG=warn "${BINARY_PATH}" script ./script/Deploy.s.sol:DeployScript --broadcast --private-key "0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e" --chain 260 --gas-estimate-multiplier 310 --rpc-url http://localhost:8011 --use "./${SOLC}" --slow  -vvv  || fail "forge script failed"
RUST_LOG=warn "${BINARY_PATH}" script ./script/Deploy.s.sol:DeployScript --broadcast --private-key "0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e" --chain 260 --gas-estimate-multiplier 310 --rpc-url http://localhost:8011 --use "./${SOLC}" --slow  -vvv  || fail "forge script failed on 2nd deploy"
echo "Running NFT script"
RUST_LOG=warn "${BINARY_PATH}" script script/NFT.s.sol:MyScript --broadcast --private-key "0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e" --rpc-url http://localhost:8011 --use 0.8.20 --zksync  || fail "forge script failed"
stop_era_test_node

success
