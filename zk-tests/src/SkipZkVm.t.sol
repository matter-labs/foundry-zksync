// SPDX-License-Identifier: BUSL-1.1
pragma solidity ^0.8.0;

import "forge-std/Vm.sol";
import "forge-std/Test.sol";
import "ds-test/test.sol";
import "../src/nestedCheatcode.sol";

contract SomeCheatcodeTest is Test{
    function testRunAndFail() public {
        SomeCheatcode cheatcode = new SomeCheatcode();
        vm.expectRevert();
        cheatcode.someCheatcode(address(vm));
    }

    function testRunAndPass() public {
        vm.roll(100);
        assertEq(block.number, 100, "roll failed");
    }

    function testRunAndPass2() public {
        SomeCheatcode cheatcode = new SomeCheatcode();
        vm.skipZkVm();
        cheatcode.someCheatcode(address(vm));
    }
}