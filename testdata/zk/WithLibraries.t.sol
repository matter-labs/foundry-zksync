// SPDX-License-Identifier: UNLICENSED

pragma solidity >=0.8.7;

import "ds-test/test.sol";
import "../cheats/Vm.sol";

import {UsesFoo} from "./WithLibraries.sol";

contract GetCodeUnlinked is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);

    function testGetCodeUnlinked() external {
        bytes memory _code = vm.getCode("UsesFoo");
    }
}

contract DeployTimeLinking is DSTest {
    function testUseUnlinkedContract() external {
        // we check that `UsesFoo` is fully linked
        // and that the inner library is usable
        UsesFoo user = new UsesFoo();
        assertEq(user.number(), 42);
    }
}
