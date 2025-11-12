// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

library Globals {
    string public constant ETHEREUM_MAINNET_URL =
        "https://eth-mainnet.alchemyapi.io/v2/cZPtUjuF-Kp330we94LOvfXUXoMU794H"; // trufflehog:ignore
    string public constant ZKSYNC_MAINNET_URL = "mainnet";

    address public constant SYSTEM_CONTEXT_ADDR = address(0x000000000000000000000000000000000000800B);
}
