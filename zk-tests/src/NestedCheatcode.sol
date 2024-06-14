// SPDX-License-Identifier: BUSL-1.1
pragma solidity ^0.8.0;

import "forge-std/Vm.sol";
import "ds-test/test.sol";

contract SomeCheatcode {
    function someCheatcode(address vm) public {
        Vm vmcheatcode = Vm(vm);
        vmcheatcode.roll(100);
       require(block.number == 100, "roll failed");
    }
}