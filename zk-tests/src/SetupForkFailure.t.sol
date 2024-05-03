// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test} from "forge-std/Test.sol";

contract ZkSetupForkFailureTest is Test {
    uint256 constant ETH_FORK_BLOCK = 18993187;

    function setUp() public {
        vm.createSelectFork(
            "https://eth-mainnet.alchemyapi.io/v2/Lc7oIGYeL_QvInzI0Wiu_pOZZDEKBrdf",
            ETH_FORK_BLOCK
        );
    }

    // We test that the following function is called after EVM fork from zk context
    function testFail_ZkSetupForkFailureExecutesTest() public pure {
        assert(false);
    }
}
