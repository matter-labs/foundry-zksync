// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {NestedMaths} from "./NestedMaths.sol";
import 'forge-std/Script.sol';

contract NestedMathematician {
    uint256 public number;

    constructor(uint256 _number) {
        number = _number;
    }

    function square() public view returns (uint256) {
        return NestedMaths.square(number);
    }
}

contract NestedMathematicianScript is Script {
    function run() external {
        NestedMathematician maths = new NestedMathematician(2);

        assert(maths.square() == 4);
    }
}
