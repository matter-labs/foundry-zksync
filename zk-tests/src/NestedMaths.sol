// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Maths} from './Maths.sol';

library NestedMaths {
    function square(uint256 x) public pure returns (uint256) {
        return Maths.square(x);
    }
}
