#!/usr/bin/env bash

# Fail fast and on piped commands
set -o pipefail -e

REPO_ROOT=".."
SOLC_VERSION=${SOLC_VERSION:-"v0.8.26"}
SOLC="solc-${SOLC_VERSION}"
FORGE="${REPO_ROOT}/target/release/forge"
CAST="${REPO_ROOT}/target/release/cast"
ERA_TEST_NODE_VERSION="v0.1.0-alpha.25"
ERA_TEST_NODE_PID=0
RPC_URL="http://localhost:8011"
PRIVATE_KEY="0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e"

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
  while ! [ -x "${FORGE}" ]; do
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
echo "forge binary: ${FORGE}"
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

build_forge "${REPO_ROOT}"

"${FORGE}" install transmissions11/solmate Openzeppelin/openzeppelin-contracts --no-commit

start_era_test_node

echo "Running tests..."
RUST_LOG=warn "${FORGE}" test --use "./${SOLC}" --chain 300 -vvv --zk-compile || fail "forge test failed"

echo "Running tests with '--zk-startup'..."
RUST_LOG=warn "${FORGE}" test --use "./${SOLC}" --chain 300 -vvv --zk-startup || fail "forge test --zk-startup failed"

echo "Running script..."
RUST_LOG=warn "${FORGE}" script ./script/Deploy.s.sol:DeployScript --broadcast --private-key "$PRIVATE_KEY" --chain 260 --gas-estimate-multiplier 310 --rpc-url "$RPC_URL" --use "./${SOLC}" --slow  -vvv  --zk-compile || fail "forge script failed"
RUST_LOG=warn "${FORGE}" script ./script/Deploy.s.sol:DeployScript --broadcast --private-key "$PRIVATE_KEY" --chain 260 --gas-estimate-multiplier 310 --rpc-url "$RPC_URL" --use "./${SOLC}" --slow  -vvv  --zk-compile || fail "forge script failed on 2nd deploy"

# Temporarily disabled
# echo "Running factory deps script..."
# RUST_LOG=warn "${FORGE}" script ./src/LargeFactoryDependencies.t.sol:ZkLargeFactoryDependenciesScript --broadcast --private-key "$PRIVATE_KEY" --chain 260 --gas-estimate-multiplier 310 --rpc-url "$RPC_URL" --use "./${SOLC}" --slow -vvv --zk-startup || fail "forge script failed"

echo "Running NFT script"
RUST_LOG=warn "${FORGE}" script ./script/NFT.s.sol:MyScript --broadcast --private-key $PRIVATE_KEY --rpc-url $RPC_URL --use 0.8.26 --zk-startup || fail "forge script failed"

echo "Running Proxy script"
RUST_LOG=warn "${FORGE}" script ./script/Proxy.s.sol:ProxyScript --broadcast --private-key $PRIVATE_KEY --rpc-url $RPC_URL --use 0.8.26 --zk-startup  || fail "forge proxy script failed"

# Deploy ERC20
echo "Deploying MyToken..."
MYTOKEN_DEPLOYMENT=$(RUST_LOG=warn "${FORGE}" create ./src/ERC20.sol:MyToken --rpc-url $RPC_URL --private-key $PRIVATE_KEY --use 0.8.26 --zk-startup) || fail "forge script failed"
MYTOKEN_ADDRESS=$(echo $MYTOKEN_DEPLOYMENT | awk '/Deployed to:/ {for (i=1; i<=NF; i++) if ($i == "to:") print $(i+1)}')
echo "MyToken deployed at: $MYTOKEN_ADDRESS"

# Deploy TokenReceiver
echo "Deploying TokenReceiver..."
TOKENRECEIVER_DEPLOYMENT=$(RUST_LOG=warn "${FORGE}" create ./src/TokenReceiver.sol:TokenReceiver --rpc-url $RPC_URL --private-key $PRIVATE_KEY --use "./${SOLC}" --zk-startup) || fail "forge script failed"
TOKENRECEIVER_ADDRESS=$(echo $TOKENRECEIVER_DEPLOYMENT | awk '/Deployed to:/ {for (i=1; i<=NF; i++) if ($i == "to:") print $(i+1)}')
echo "TokenReceiver deployed at: $TOKENRECEIVER_ADDRESS"

# Wait for deployments to be mined
sleep 10

# Interact: Transfer tokens from MyToken to TokenReceiver
echo "Transferring tokens from MyToken to TokenReceiver..."
AMOUNT="1" # 1 token, for example
TRANSACTION=$("${CAST}" send --rpc-url $RPC_URL --private-key $PRIVATE_KEY $MYTOKEN_ADDRESS "transfer(address,uint256)" $TOKENRECEIVER_ADDRESS $AMOUNT)

# Assert that the transaction was committed looking for the transaction hash "transactionHash" keyword
echo "Transaction: ${TRANSACTION}"
echo "Checking transaction status..."
echo "${TRANSACTION}" | grep -q "transactionHash" || fail "Transaction failed"
stop_era_test_node

success
