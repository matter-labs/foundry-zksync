// SPDX-License-Identifier: UNLICENSED

pragma solidity >=0.8.7 <0.9.0;

library Foo {
    function add(uint a, uint b) external pure returns (uint c){
        c = a + b;
    }
}

contract UsesFoo {
    uint number;

    constructor() {
        number = Foo.add(42, 0);
    }
}
