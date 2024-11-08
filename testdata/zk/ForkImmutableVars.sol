// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "ds-test/test.sol";
import "../cheats/Vm.sol";
import {Globals} from "./Globals.sol";

contract Counter {
    uint256 public immutable SOME_IMMUTABLE_VARIABLE;

    constructor(uint256 value) {
        SOME_IMMUTABLE_VARIABLE = value;
    }

    uint256 public a;

    function set(uint256 _a) public {
        a = _a;
    }

    function get() public view returns (uint256) {
        return a;
    }
}

contract ZkForkImmutableVarsTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);
    uint256 constant ERA_FORK_BLOCK = 19579636;
    uint256 constant IMMUTABLE_VAR_VALUE = 0xdeadbeef;

    function setUp() public {
        vm.createSelectFork(Globals.ZKSYNC_MAINNET_URL, ERA_FORK_BLOCK);
    }

    function testZkImmutableVariablesPersistedAfterFork() public {
        Counter counter = new Counter(IMMUTABLE_VAR_VALUE);
        assertEq(IMMUTABLE_VAR_VALUE, counter.SOME_IMMUTABLE_VARIABLE());

        vm.makePersistent(address(counter));
        assertTrue(vm.isPersistent(address(counter)));

        vm.createSelectFork(Globals.ZKSYNC_MAINNET_URL, ERA_FORK_BLOCK - 100);

        assertEq(IMMUTABLE_VAR_VALUE, counter.SOME_IMMUTABLE_VARIABLE());
    }
}
