// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import "forge-std/Test.sol";

// https://github.com/matter-labs/foundry-zksync/issues/497
contract Issue497 is Test {
    uint256 constant ERA_FORK_BLOCK = 19579636;

    uint256 forkEra;

    function setUp() public {
        forkEra = vm.createFork("mainnet", ERA_FORK_BLOCK);
    }

    function testZkEnsureContractMigratedWhenForkZkSyncThenZkVmOff() external {
       vm.selectFork(forkEra);
       (bool success, ) = address(vm).call(
           abi.encodeWithSignature("zkVm(bool)", false)
       );
       assert(address(vm).codehash != 0);
    }
}
