// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test} from "forge-std/Test.sol";

contract ZkFuzzTest is Test {
    function testZkFuzzAvoidSystemAddresses(address addr) public pure {
        assert(addr > address(65535));
    }
}
