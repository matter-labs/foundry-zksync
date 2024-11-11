// SPDX-License-Identifier: UNLICENSED

pragma solidity >=0.8.7 <0.9.0;

library Foo {
    function add(uint256 a, uint256 b) external pure returns (uint256 c) {
        c = a + b;
    }
}

contract UsesFoo {
    uint256 number;

    constructor() {
        number = Foo.add(42, 0);
    }
}
