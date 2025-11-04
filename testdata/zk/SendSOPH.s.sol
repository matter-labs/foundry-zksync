// SPDX-License-Identifier: MIT
// Reproduction script for issue #1191: Cannot send the fee token with --zksync
// This script tests sending native tokens (ETH/SOPH) with the --zksync flag
// 
// The issue was that when converting TransactionRequest to ZkTransactionRequest,
// the value field was not being preserved, causing the transaction to send 0 value.
//
// Usage:
//   forge script testdata/zk/SendSOPH.s.sol:SendSOPH --rpc-url $RPC_URL --private-key $PRIVATE_KEY --broadcast --zksync -vvvv

pragma solidity ^0.8.13;

import "../lib/ds-test/src/test.sol";
import "../cheats/Vm.sol";

/// @notice Simple script to send native tokens (ETH/SOPH) to an address
/// This is based on issue #1191 - testing sending fee tokens with --zksync flag
contract SendSOPH is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);

    function run() external {
        // Get the recipient address from environment or use a default
        address payable recipient = payable(vm.envOr("RECIPIENT", address(0x1234567890123456789012345678901234567890)));
        uint256 amount = vm.envOr("AMOUNT", uint256(0.001 ether));
        
        vm.startBroadcast();
        
        // Simple ETH/SOPH transfer
        (bool success,) = recipient.call{value: amount}("");
        require(success, "Transfer failed");
        
        vm.stopBroadcast();
    }
}

