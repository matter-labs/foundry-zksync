// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract CheatcodeReadCallers is Test {
    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;
    address constant TEST_ORIGIN = 0xdEBe90b7BFD87Af696B1966082F6515a6E72F3d8;

    // enum CallerMode {
    //     None,
    //     Broadcast,
    //     RecurrentBroadcast,
    //     Prank,
    //     RecurrentPrank
    //  }

    function testNormalReadCallers() public {
        (bool success, bytes memory data) = Constants.CHEATCODE_ADDRESS.call(
                abi.encodeWithSignature("readCallers()"));
        require(success, "readCallers failed");

        (uint8 mode, address sender, address origin) = abi.decode(data, (uint8, address, address));
        require(mode == 0, "normal call mode");
        require(sender == msg.sender, "sender not overridden");
        require(origin == tx.origin, "origin not overridden");
    }

    function testPrankedReadCallers() public {
        (bool success1, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("startPrank(address)", TEST_ADDRESS)
        );
        require(success1, "startPrank failed");

        (bool success, bytes memory data) = Constants.CHEATCODE_ADDRESS.call(
                abi.encodeWithSignature("readCallers()"));
        require(success, "readCallers failed");

        (uint8 mode, address sender, address origin) = abi.decode(data, (uint8, address, address));
        require(mode == 4, "recurrent prank call mode");
        require(sender == TEST_ADDRESS, "sender overridden");
        require(origin == tx.origin, "origin not overridden");

        console.log("failed?", failed());
    }

    function testFullyPrankedReadCallers() public {
        (bool success1, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("startPrank(address,address)", TEST_ADDRESS, TEST_ORIGIN)
        );
        require(success1, "startPrank failed");

        (bool success, bytes memory data) = Constants.CHEATCODE_ADDRESS.call(
                abi.encodeWithSignature("readCallers()"));
        require(success, "readCallers failed");

        (uint8 mode, address sender, address origin) = abi.decode(data, (uint8, address, address));

        require(mode == 4, "recurrent prank call mode");
        require(sender == TEST_ADDRESS, "sender overridden");
        require(origin == TEST_ORIGIN, "origin overridden");

        console.log("failed?", failed());
    }
}
