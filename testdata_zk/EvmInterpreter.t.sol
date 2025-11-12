// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import "utils/Test.sol";
import "utils/console.sol";

contract Counter {
    uint256 number;

    constructor() {
        number = 0x01;
    }

    function get() public view returns (uint256) {
        return number;
    }

    function inc() public {
        number += 1;
    }
}

contract EvmInterpreterTest is Test {
    function testCreate() public {
        Counter counter = new Counter();

        // uses EVM address derivation
        // 0xB5c1DF089600415B21FB76bf89900Adb575947c8 in eraVM.
        assertEq(address(0x7D8CB8F412B3ee9AC79558791333F41d2b1ccDAC), address(counter));
    }

    function testCall() public {
        Counter counter = new Counter();

        // uses EVM address derivation
        // 0xB5c1DF089600415B21FB76bf89900Adb575947c8 in eraVM.
        assertEq(address(0x7D8CB8F412B3ee9AC79558791333F41d2b1ccDAC), address(counter));

        assertEq(1, counter.get());
        counter.inc();
        assertEq(2, counter.get());
    }
}
