// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import "ds-test/test.sol";
import "cheats/Vm.sol";
import {Globals} from "../Globals.sol";

contract Counter {
    uint256 number;

    function get() public returns (uint256) {
        return number;
    }

    function inc() public {
        number += 1;
    }
}

// https://github.com/matter-labs/foundry-zksync/issues/687
contract Issue687 is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);

    uint256 constant ERA_FORK_BLOCK = 19579636;

    uint256 forkEra;

    function setUp() public {
        forkEra = vm.createSelectFork(Globals.ZKSYNC_MAINNET_URL, ERA_FORK_BLOCK);
    }

    function testZkEnsureContractMigratedWhenForkedIfPersistent() external {
        Counter counter = new Counter();
        counter.inc();
        assertEq(1, counter.get());
        vm.makePersistent(address(counter));
        assertTrue(vm.isPersistent(address(counter)));

        vm.createSelectFork(Globals.ZKSYNC_MAINNET_URL, ERA_FORK_BLOCK - 10);

        assertTrue(vm.isPersistent(address(counter)));
        assertEq(1, counter.get());
        counter.inc();
        assertEq(2, counter.get());
    }
}
