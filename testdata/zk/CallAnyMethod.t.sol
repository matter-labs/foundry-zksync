// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "ds-test/test.sol";
import "../cheats/Vm.sol";
import {CallAnyMethod} from "./CallAnyMethod.sol";

// This test checks that the test contract can still be invoked
// from EraVM context, including preservation of immutable variables.
contract CallAnyMethodTest is DSTest {
    CallAnyMethod public callAnyMethod;
    Vm constant vm = Vm(HEVM_ADDRESS);

    uint256 immutable initialNumber;
    uint256 immutable otherNumber = 100;

    constructor() {
        initialNumber = 42;
    }

    function setUp() public {
        callAnyMethod = new CallAnyMethod();
    }

    function callable() public pure returns (bool) {
        return true;
    }

    function callableFails() public pure {
        revert("This function always fails");
    }

    function returnsImmutable() public view returns (uint256) {
        return initialNumber + otherNumber;
    }

    function test_callAnyFunction() public {
        bytes memory data = abi.encodeWithSignature("callable()");
        bytes memory result = callAnyMethod.callAnyMethod(address(this), data);
        bool success = abi.decode(result, (bool));
        assertTrue(success);
    }

    function test_callAnyFunctionReverts() public {
        bytes memory data = abi.encodeWithSignature("callableFails()");
        vm.expectRevert("Call failed");
        callAnyMethod.callAnyMethod(address(this), data);
    }

    // Checks that immutable variables in the test contract are preserved
    // during migration to EraVM context.
    function test_getImmutable() public {
        bytes memory data = abi.encodeWithSignature("returnsImmutable()");
        bytes memory result = callAnyMethod.callAnyMethod(address(this), data);
        uint256 number = abi.decode(result, (uint256));
        assertEq(number, initialNumber + otherNumber);
    }
}
