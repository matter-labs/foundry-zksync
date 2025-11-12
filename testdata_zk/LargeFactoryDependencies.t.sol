// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import "utils/Test.sol";
import "./LargeContracts.sol";

interface Cheatcodes {
    function broadcast(address sender) external view;
    function zkGetTransactionNonce(address account) external view returns (uint64 nonce);
    function zkGetDeploymentNonce(address account) external view returns (uint64 nonce);
}

contract ZkLargeFactoryDependenciesTest is Test {
    Cheatcodes internal constant vmExt = Cheatcodes(HEVM_ADDRESS);

    function testLargeFactoryDependenciesAreDeployedInBatches() public {
        new LargeContract();
    }

    function testConsistentNoncesWithBatchedFactoryDependencies() public {
        address sender = tx.origin;
        uint256 txNonce = vmExt.zkGetTransactionNonce(sender);
        uint256 deploymentNonce = vmExt.zkGetDeploymentNonce(sender);

        // LargeContract should be deployed in 3 batches
        uint256 numBatches = 3;

        vm.broadcast(sender); // otherwise nonce is not propagated
        new LargeContract();
        assertEq(vmExt.zkGetDeploymentNonce(sender), deploymentNonce + 1);
        assertEq(vmExt.zkGetTransactionNonce(sender), txNonce + numBatches);
    }
}
