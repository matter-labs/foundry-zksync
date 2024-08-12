// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import "ds-test/test.sol";
import "cheats/Vm.sol";
import {Globals} from "../Globals.sol";

// https://github.com/matter-labs/foundry-zksync/issues/497
contract Issue497 is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);

    uint256 constant ERA_FORK_BLOCK = 19579636;

    uint256 forkEra;

    function setUp() public {
        forkEra = vm.createFork(Globals.ZKSYNC_MAINNET_URL, ERA_FORK_BLOCK);
    }

    function testZkEnsureContractMigratedWhenForkZkSyncThenZkVmOff() external {
        vm.selectFork(forkEra);
        vm.zkVm(false);
        assert(address(vm).codehash != 0);
    }
}
