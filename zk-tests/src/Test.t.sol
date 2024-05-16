// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test} from "forge-std/Test.sol";

contract MyContract11 {
    function transact() public {}
}

contract ZkMyTest is Test {
    function testFoo11() public {
       new MyContract11();
    }
}
