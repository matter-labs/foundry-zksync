// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Maths} from "./Maths.sol";
import 'forge-std/Script.sol';

contract Mathematician {
    uint256 public number;

    constructor(uint256 _number) {
        number = _number;
    }

    function square() public view returns (uint256) {
        return Maths.square(number);
    }
}

contract MathematicianScript is Script {
    function run() external {
        Mathematician maths = new Mathematician(2);

        assert(maths.square() == 4);
    }
}
