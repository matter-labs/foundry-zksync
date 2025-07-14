// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import "ds-test/test.sol";
import "./LargeContracts.sol";

interface Cheatcodes {
    function broadcast(address sender) external view;
    function zkGetTransactionNonce(address account) external view returns (uint64 nonce);
    function zkGetDeploymentNonce(address account) external view returns (uint64 nonce);
}

contract ZkLargeFactoryDependenciesTest is DSTest {
    Cheatcodes internal constant vm = Cheatcodes(HEVM_ADDRESS);

    function testLargeFactoryDependenciesAreDeployedInBatches() public {
        new LargeContract();
    }

    function testConsistentNoncesWithBatchedFactoryDependencies() public {
        address sender = tx.origin;
        uint256 txNonce = vm.zkGetTransactionNonce(sender);
        uint256 deploymentNonce = vm.zkGetDeploymentNonce(sender);

        // LargeContract should be deployed in 3 batches
        uint256 numBatches = 3;

        vm.broadcast(sender); // otherwise nonce is not propagated
        new LargeContract();
        assertEq(vm.zkGetDeploymentNonce(sender), deploymentNonce + 1);
        assertEq(vm.zkGetTransactionNonce(sender), txNonce + numBatches);
    }
}
