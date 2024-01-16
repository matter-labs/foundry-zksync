#!/usr/bin/env bash

# Fail fast and on piped commands
set -o pipefail -e

REPO_ROOT="../../.."
SOLC_VERSION=${SOLC_VERSION:-"v0.8.20"}
SOLC="solc-${SOLC_VERSION}"
BINARY_PATH="${REPO_ROOT}/target/debug/zkforge"

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
  cargo build --manifest-path="${1}/Cargo.toml"
  wait_for_build 30
}

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

build_zkforge "${REPO_ROOT}"

echo "Running tests..."
RUST_LOG=debug "${BINARY_PATH}" test --use "./${SOLC}" --mt "test_Sign"