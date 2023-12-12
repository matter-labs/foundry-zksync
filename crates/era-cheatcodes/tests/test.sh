#!/usr/bin/env bash

# Fail fast and on piped commands
set -o pipefail -e

TEST_REPO=${1:-$TEST_REPO}
TEST_REPO_DIR=${2:-$TEST_REPO_DIR}
SOLC_VERSION=${SOLC_VERSION:-"v0.8.20"}
SOLC="solc-${SOLC_VERSION}"
BINARY_PATH="../../../target/debug/zkforge"

if [ "${TEST_REPO}" == "foundry-zksync" ]; then
  BINARY_PATH="${TEST_REPO_DIR}/target/release/zkforge"
fi

function cleanup() {
  echo "Cleaning up..."
  rm "./${SOLC}"
}

function download_solc() {
  case "$(uname -s)" in
  Darwin*) arch=macos ;;
  *) arch=linux ;;
  esac
  if [ ! -x "${SOLC}" ]; then
    wget -O "${SOLC}" "https://github.com/ethereum/solidity/releases/download/${1}/solc-static-${arch}"
    chmod +x "${SOLC}"
  fi
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
  echo "Building ${1}..."
  cargo build --release --manifest-path="${1}/Cargo.toml"
  wait_for_build 30
}

# trap cleanup ERR

echo "Solc: ${SOLC_VERSION}"
echo "Zkforge binary: ${BINARY_PATH}"

# Download solc
download_solc "${SOLC_VERSION}"

# Check for necessary tools
command -v cargo &>/dev/null || {
  echo "cargo not found, exiting"
  exit 1
}
command -v git &>/dev/null || {
  echo "git not found, exiting"
  exit 1
}

build_zkforge "../../.."

echo "Running tests..."
# "${BINARY_PATH}" zkbuild --use "./${SOLC}"
RUST_LOG=debug "${BINARY_PATH}" test --use "./${SOLC}"

# cleanup
