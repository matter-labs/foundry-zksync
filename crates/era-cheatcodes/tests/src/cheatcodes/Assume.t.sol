// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";

contract AssumeTest is Test {
    function testAssume() public {
        vm.assume(true);
        if (true) {
            console.log("did not discard inputs");
        }
        assertTrue(false, "did not discard inputs");
    }
}
