// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract Storage {
    uint256 public slot0 = 10;
    uint256 public slot1 = 20;
}

contract StoreTest is Test {
    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;
    Storage store;

    function setUp() public {
        store = new Storage();
    }

    function testStore() public {
        assertEq(store.slot0(), 10, "initial value for slot 0 is incorrect");
        assertEq(store.slot1(), 20, "initial value for slot 1 is incorrect");

        (bool success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature(
                "store(address,bytes32,bytes32)",
                address(store),
                bytes32(0),
                bytes32(uint256(1))
            )
        );
        require(success, "store failed");
        assertEq(store.slot0(), 1, "store failed");
        assertEq(store.slot1(), 20, "store failed");
        console.log("failed?", failed());
    }
}
