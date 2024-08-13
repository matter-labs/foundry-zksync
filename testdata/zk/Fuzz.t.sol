// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "ds-test/test.sol";

contract ZkFuzzTest is DSTest {
    function testZkFuzzAvoidSystemAddresses(address addr) public pure {
        assert(addr > address(65535));
    }
}
