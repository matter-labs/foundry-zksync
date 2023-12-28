// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract PrankTest is Test {
    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;
    address constant TEST_ORIGIN = 0xdEBe90b7BFD87Af696B1966082F6515a6E72F3d8;
    function testPrankSender() public {
        address original_msg_sender = msg.sender;
        address original_tx_origin = tx.origin;       
        // Perform the prank
        PrankVictim victim = new PrankVictim();

       // Verify that the victim is set up correctly
        victim.assertCallerAndOrigin(
            address(this),
            "Prank failed: victim.assertCallerAndOrigin failed",
            original_tx_origin,
            "Prank failed: victim.assertCallerAndOrigin failed"
        );

        (bool success1, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("prank(address)", TEST_ADDRESS)
        );
        victim.assertCallerAndOrigin( 
            TEST_ADDRESS,
            "msg.sender was not set during prank",
            tx.origin,
            "tx.origin invariant failed"
        );

        // Ensure we cleaned up correctly
        victim.assertCallerAndOrigin(
            address(this),
            "msg.sender was not cleaned up",
            tx.origin,
            "tx.origin invariant failed"
        );
    }

    // function testPrankOrigin(address sender, address origin) public {
    //     address oldOrigin = tx.origin;

    //     // Perform the prank
    //     Victim victim = new Victim();
    //     vm.prank(sender, origin);
    //     victim.assertCallerAndOrigin(
    //         sender,
    //         "msg.sender was not set during prank",
    //         origin,
    //         "tx.origin was not set during prank"
    //     );

    //     // Ensure we cleaned up correctly
    //     victim.assertCallerAndOrigin(
    //         address(this),
    //         "msg.sender was not cleaned up",
    //         oldOrigin,
    //         "tx.origin was not cleaned up"
    //     );
    // }
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
