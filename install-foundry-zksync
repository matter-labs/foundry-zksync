#!/bin/bash

set -e

# URLs to the raw files on GitHub
INSTALL_SCRIPT_URL="https://raw.githubusercontent.com/matter-labs/foundry-zksync/main/foundryup-zksync/install"
FOUNDRYUP_ZKSYNC_URL="https://raw.githubusercontent.com/matter-labs/foundry-zksync/main/foundryup-zksync/foundryup-zksync"

if [ -n "${CI}" ]; then
    echo "Using local install script..."
else
    # Download the install script
    echo "Downloading the install script..."
    curl -L "$INSTALL_SCRIPT_URL" -o install
fi

echo "Making install script executable..."
chmod +x ./install

echo "Running the installation script..."
./install | tee install.log  # Capture the output to both stdout and a log file

# Extract the exact path from the install.log
# Use sed to precisely capture the path after 'source' and before the next apostrophe
SHELL_CONFIG_FILE=$(sed -n "s/.*Run 'source \(.*\)'.*/\1/p" install.log)
FOUNDRY_BIN_DIR="${XDG_CONFIG_HOME:-$HOME}/.foundry/bin"

if [ -n "$SHELL_CONFIG_FILE" ]; then
    if [ -n "${CI}" ]; then
        # Add manually to $PATH in CI mode as GHA does not work with `.`
        echo "adding '${FOUNDRY_BIN_DIR}' to PATH and GITHUB_PATH"
        export PATH="$PATH:$FOUNDRY_BIN_DIR"
        echo "${FOUNDRY_BIN_DIR}" >> $GITHUB_PATH
    else
        echo "Sourcing the shell configuration file: '$SHELL_CONFIG_FILE'"
        # Use dot (.) to source the file, which is more universally compatible
        . "$SHELL_CONFIG_FILE"
    fi
else
    echo "No shell configuration file detected. Please source your shell manually or start a new terminal session."
fi

if [ -n "${CI}" ]; then
    echo "Using local foundryup-zksync script..."
else
    echo "Downloading foundryup-zksync..."
    curl -L "$FOUNDRYUP_ZKSYNC_URL" -o foundryup-zksync
fi

echo "Making foundryup-zksync executable..."
chmod +x ./foundryup-zksync

echo "Running foundryup-zksync setup..."
./foundryup-zksync

# Cleanup: remove install and install.log
# Keeps foundryup-zksync for ease of use
echo "Cleaning up installation artifacts..."
rm -f ./install ./install.log
echo "Cleanup completed."

echo "Installation completed successfully!"

echo "Verifying installation..."
FORGE_VERSION_OUTPUT=$("${FOUNDRY_BIN_DIR}/forge" --version 2>/dev/null || true)

echo $FORGE_VERSION_OUTPUT

if [ -z "$FORGE_VERSION_OUTPUT" ]; then
    echo "Installation verification failed. 'forge --version' returned empty or an error."
    exit 1
fi

if echo "$FORGE_VERSION_OUTPUT" | grep -E -q "[0-9]+\.[0-9]+\.[0-9]+"; then
    echo "Forge is successfully installed with version: $FORGE_VERSION_OUTPUT"
else
    echo "Forge is installed, but no semantic version (x.x.x) was detected."
    echo "Installed version output: $FORGE_VERSION_OUTPUT"
fi
