// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, Vm, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";
import {Utils} from "./Utils.sol";

struct Storage {
    uint256 slot0;
    uint256 slot1;
}

contract SnapshotTest is Test {
    Storage store;

    function setUp() public {
        store.slot0 = 10;
        store.slot1 = 20;
    }

    function testSnapshot() public {
        console.log("calling snapshot");

        store.slot0 = 10;
        store.slot1 = 20;

        uint256 snapshot = vm.snapshot();

        console.log("snapshot ", snapshot);

        console.log("store values: ", store.slot0, store.slot1);

        store.slot0 = 300;
        store.slot1 = 400;

        assertEq(store.slot0, 300);
        assertEq(store.slot1, 400);

        console.log("store values: ", store.slot0, store.slot1);
        console.log("calling revertTo");

        //this still does not work with vm
        // bool success = vm.revertTo(snapshot);
        // require(success, "revertTo failed");

        (bool success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("revertTo(uint256)", snapshot)
        );
        require(success, "revertTo failed");

        console.log("store values: ", store.slot0, store.slot1);

        assertEq(store.slot0, 10, "snapshot revert for slot 0 unsuccessful");
        assertEq(store.slot1, 20, "snapshot revert for slot 1 unsuccessful");
    }

    function testBlockValues() public {
        uint256 num = block.number;
        uint256 time = block.timestamp;

        uint256 snapshot = vm.snapshot();
        
        vm.warp(1337);

        assertEq(block.timestamp, 1337);

        vm.roll(99);

        assertEq(block.number, 99);

        (bool success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("revertTo(uint256)", snapshot)
        );
        require(success, "revertTo failed");

        assertEq(
            block.number,
            num,
            "snapshot revert for block.number unsuccessful"
        );
        assertEq(
            block.timestamp,
            time,
            "snapshot revert for block.timestamp unsuccessful"
        );
    }
}
