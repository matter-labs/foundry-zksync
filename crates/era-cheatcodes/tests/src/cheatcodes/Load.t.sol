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
        (bool success, bytes memory data) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature(
                "load(address,bytes32)",
                address(this),
                bytes32(slot)
            )
        );
        require(success, "load failed");
        uint256 val = abi.decode(data, (uint256));
        assertEq(val, 20, "load failed");
    }

    function testLoadOtherStorage() public {
        (bool success, bytes memory data) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature(
                "load(address,bytes32)",
                address(store),
                bytes32(0)
            )
        );
        require(success, "load failed");
        uint256 val = abi.decode(data, (uint256));
        assertEq(val, 10, "load failed");
    }
}
