// SPDX-License-Identifier: UNLICENSED

pragma solidity >=0.8.7 <0.9.0;

import "forge-std/console.sol";

library Foo {
    function foo() external pure {
        console.log("I'm a library");
    }
}

contract UsesFoo {
    constructor() {
        Foo.foo();
    }
}
