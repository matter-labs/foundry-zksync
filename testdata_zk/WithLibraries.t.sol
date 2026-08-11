// SPDX-License-Identifier: UNLICENSED

pragma solidity >=0.8.7;

import "utils/Test.sol";

import {UsesFoo} from "./WithLibraries.sol";

contract DeployTimeLinking is Test {
    function testUseUnlinkedContract() external {
        // we check that `UsesFoo` is fully linked
        // and that the inner library is usable
        UsesFoo user = new UsesFoo();
        assertEq(user.number(), 42);
    }
}
