// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "utils/Test.sol";

contract ZkSetupForkFailureTest is Test {
    uint256 constant ETH_FORK_BLOCK = 18993187;

    function setUp() public {
        vm.createSelectFork("https://eth-mainnet.alchemyapi.io/v2/cZPtUjuF-Kp330we94LOvfXUXoMU794H", ETH_FORK_BLOCK); // trufflehog:ignore
    }

    // We test that the following function is called after EVM fork from zk context
    function test_ZkSetupForkFailureExecutesTest() public {
        // We check this test to fails on the cargo side.
        assert(false);
    }
}
