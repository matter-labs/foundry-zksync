// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract CheatcodeStartPrankTest is Test {
    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;
    address constant TEST_ORIGIN = 0xdEBe90b7BFD87Af696B1966082F6515a6E72F3d8;

    function testStartPrank() public {
        address original_msg_sender = msg.sender;
        address original_tx_origin = tx.origin;

        PrankVictim victim = new PrankVictim();

        // Verify that the victim is set up correctly
        victim.assertCallerAndOrigin(
            address(this),
            "startPrank failed: victim.assertCallerAndOrigin failed",
            original_tx_origin,
            "startPrank failed: victim.assertCallerAndOrigin failed"
        );

        // Start prank without tx.origin
        (bool success1, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("startPrank(address)", TEST_ADDRESS)
        );
        require(success1, "startPrank failed");

        require(
            msg.sender == TEST_ADDRESS,
            "startPrank failed: msg.sender unchanged"
        );
        require(
            tx.origin == original_tx_origin,
            "startPrank failed tx.origin changed"
        );
        victim.assertCallerAndOrigin(
            TEST_ADDRESS,
            "startPrank failed: victim.assertCallerAndOrigin failed",
            original_tx_origin,
            "startPrank failed: victim.assertCallerAndOrigin failed"
        );

        // Stop prank
        (bool success2, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("stopPrank()")
        );
        require(success2, "stopPrank failed");

        require(
            msg.sender == original_msg_sender,
            "stopPrank failed: msg.sender didn't return to original"
        );
        require(
            tx.origin == original_tx_origin,
            "stopPrank failed tx.origin changed"
        );
        victim.assertCallerAndOrigin(
            address(this),
            "startPrank failed: victim.assertCallerAndOrigin failed",
            original_tx_origin,
            "startPrank failed: victim.assertCallerAndOrigin failed"
        );

        console.log("failed?", failed());
    }

    function testStartPrankWithOrigin() external {
        address original_msg_sender = msg.sender;
        address original_tx_origin = tx.origin;

        PrankVictim victim = new PrankVictim();

        // Verify that the victim is set up correctly
        victim.assertCallerAndOrigin(
            address(this),
            "startPrank failed: victim.assertCallerAndOrigin failed",
            original_tx_origin,
            "startPrank failed: victim.assertCallerAndOrigin failed"
        );

        // Start prank with tx.origin
        (bool success1, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature(
                "startPrank(address,address)",
                TEST_ADDRESS,
                TEST_ORIGIN
            )
        );
        require(success1, "startPrank failed");

        require(
            msg.sender == TEST_ADDRESS,
            "startPrank failed: msg.sender unchanged"
        );
        require(
            tx.origin == TEST_ORIGIN,
            "startPrank failed: tx.origin unchanged"
        );
        victim.assertCallerAndOrigin(
            TEST_ADDRESS,
            "startPrank failed: victim.assertCallerAndOrigin failed",
            TEST_ORIGIN,
            "startPrank failed: victim.assertCallerAndOrigin failed"
        );

        // Stop prank
        (bool success2, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("stopPrank()")
        );
        require(success2, "stopPrank failed");

        require(
            msg.sender == original_msg_sender,
            "stopPrank failed: msg.sender didn't return to original"
        );
        require(
            tx.origin == original_tx_origin,
            "stopPrank failed: tx.origin didn't return to original"
        );
        victim.assertCallerAndOrigin(
            address(this),
            "startPrank failed: victim.assertCallerAndOrigin failed",
            original_tx_origin,
            "startPrank failed: victim.assertCallerAndOrigin failed"
        );

        console.log("failed?", failed());
    }
}

contract PrankVictim {
    function assertCallerAndOrigin(
        address expectedSender,
        string memory senderMessage,
        address expectedOrigin,
        string memory originMessage
    ) public view {
        console.log("msg.sender", msg.sender);
        console.log("tx.origin", tx.origin);
        require(msg.sender == expectedSender, senderMessage);
        require(tx.origin == expectedOrigin, originMessage);
    }
}
