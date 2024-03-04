// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract Storage {
    uint256 slot0 = 10;
}

contract LoadTest is Test {
    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;
    uint256 slot0 = 20;
    Storage store;

    function setUp() public {
        store = new Storage();
    }

    function testLoadOwnStorage() public {
        uint256 slot;
        assembly {
            slot := slot0.slot
        }

        bytes32 data = vm.load(address(this), bytes32(slot));

        assertEq(uint256(data), 20, "load failed");
    }

    function testLoadOtherStorage() public {
        bytes32 data = vm.load(address(store), bytes32(0));

        assertEq(uint256(data), 10, "load failed");
    }
}
