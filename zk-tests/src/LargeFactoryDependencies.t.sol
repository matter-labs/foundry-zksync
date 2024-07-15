// SPDX-License-Identifier: MIT 
pragma solidity ^0.8.18;

import "forge-std/Test.sol";
import "forge-std/Script.sol";
import {LargeContract} from "./LargeContracts.sol";

// Temporarily disabled due to issues with batching

// contract ZkLargeFactoryDependenciesTest is Test {
//     function testLargeFactoryDependenciesAreDeployedInBatches() public {
//         new LargeContract();
//     }
// }

// contract ZkLargeFactoryDependenciesScript is Script {
//     function run() external {
//         vm.broadcast();
//         new LargeContract();
//     }
// }
