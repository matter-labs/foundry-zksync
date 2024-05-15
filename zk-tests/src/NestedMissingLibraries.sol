// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {NestedMaths} from "./NestedMaths.sol";
import 'forge-std/Test.sol';

contract NestedMathematician {
    uint256 public number;

    constructor(uint256 _number) {
        number = _number;
    }

    function square() public view returns (uint256) {
        return NestedMaths.square(number);
    }
}

contract NestedMathematicianTest is Test {
    function testNestedLibraries() external {
        NestedMathematician maths = new NestedMathematician(2);

        assertEq(maths.square(), 4);
    }
}
