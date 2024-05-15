// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Maths} from "./Maths.sol";
import 'forge-std/Test.sol';

contract Mathematician {
    uint256 public number;

    constructor(uint256 _number) {
        number = _number;
    }

    function square() public view returns (uint256) {
        return Maths.square(number);
    }
}

contract MathematicianTest is Test {
    function testLibraries() external {
        Mathematician maths = new Mathematician(2);

        assertEq(maths.square(), 4);
    }
}
