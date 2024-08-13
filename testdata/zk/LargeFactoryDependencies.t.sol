// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import "ds-test/test.sol";
import "./LargeContracts.sol";

// Temporarily disabled due to issues with batching
contract ZkLargeFactoryDependenciesTest is DSTest {
    function testLargeFactoryDependenciesAreDeployedInBatches() public {
        new LargeContract();
    }
}
