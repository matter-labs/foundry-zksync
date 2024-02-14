// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract Victim {
    function assertCallerAndOrigin(
        address expectedSender,
        string memory senderMessage,
        address expectedOrigin,
        string memory originMessage
    ) public view {
        require(msg.sender == expectedSender, senderMessage);
        require(tx.origin == expectedOrigin, originMessage);
    }
}

contract ConstructorVictim is Victim {
    constructor(
        address expectedSender,
        string memory senderMessage,
        address expectedOrigin,
        string memory originMessage
    ) {
        require(msg.sender == expectedSender, senderMessage);
        require(tx.origin == expectedOrigin, originMessage);
    }
}

contract NestedVictim {
    Victim innerVictim;

    constructor(Victim victim) {
        innerVictim = victim;
    }

    function assertCallerAndOrigin(
        address expectedSender,
        string memory senderMessage,
        address expectedOrigin,
        string memory originMessage
    ) public view {
        require(msg.sender == expectedSender, senderMessage);
        require(tx.origin == expectedOrigin, originMessage);
        innerVictim.assertCallerAndOrigin(
            address(this),
            "msg.sender was incorrectly set for nested victim",
            expectedOrigin,
            "tx.origin was incorrectly set for nested victim"
        );
    }
}

contract PrankTest is Test {
    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;
    address constant TEST_ORIGIN = 0xdEBe90b7BFD87Af696B1966082F6515a6E72F3d8;

    function testPrankSender() public {
        // Perform the prank
        Victim victim = new Victim();
        vm.prank(TEST_ADDRESS);
        victim.assertCallerAndOrigin(
            TEST_ADDRESS, "msg.sender was not set during prank", tx.origin, "tx.origin invariant failed"
        );

        // Ensure we cleaned up correctly
        victim.assertCallerAndOrigin(
            address(this), "msg.sender was not cleaned up", tx.origin, "tx.origin invariant failed"
        );
    }

    function testPrankOrigin() public {
        address oldOrigin = tx.origin;

        // Perform the prank
        Victim victim = new Victim();
        vm.prank(TEST_ADDRESS, TEST_ORIGIN);
        victim.assertCallerAndOrigin(
            TEST_ADDRESS, "msg.sender was not set during prank", TEST_ORIGIN, "tx.origin was not set during prank"
        );

        // Ensure we cleaned up correctly
        victim.assertCallerAndOrigin(
            address(this), "msg.sender was not cleaned up", oldOrigin, "tx.origin was not cleaned up"
        );
    }

    function testPrank1AfterPrank0() public {
        // Perform the prank
        address oldOrigin = tx.origin;
        Victim victim = new Victim();
        vm.prank(TEST_ADDRESS);
        victim.assertCallerAndOrigin(
            TEST_ADDRESS, "msg.sender was not set during prank", oldOrigin, "tx.origin was not set during prank"
        );

        // Ensure we cleaned up correctly
        victim.assertCallerAndOrigin(
            address(this), "msg.sender was not cleaned up", oldOrigin, "tx.origin invariant failed"
        );

        // Overwrite the prank
        vm.prank(TEST_ADDRESS, TEST_ORIGIN);
        victim.assertCallerAndOrigin(
            TEST_ADDRESS, "msg.sender was not set during prank", TEST_ORIGIN, "tx.origin invariant failed"
        );

        // Ensure we cleaned up correctly
        victim.assertCallerAndOrigin(
            address(this), "msg.sender was not cleaned up", oldOrigin, "tx.origin invariant failed"
        );
    }

    function testPrank0AfterPrank1() public {
        // Perform the prank
        address oldOrigin = tx.origin;
        Victim victim = new Victim();
        vm.prank(TEST_ADDRESS, TEST_ORIGIN);
        victim.assertCallerAndOrigin(
            TEST_ADDRESS, "msg.sender was not set during prank", TEST_ORIGIN, "tx.origin was not set during prank"
        );

        // Ensure we cleaned up correctly
        victim.assertCallerAndOrigin(
            address(this), "msg.sender was not cleaned up", oldOrigin, "tx.origin invariant failed"
        );

        // Overwrite the prank
        vm.prank(TEST_ADDRESS);
        victim.assertCallerAndOrigin(
            TEST_ADDRESS, "msg.sender was not set during prank", oldOrigin, "tx.origin invariant failed"
        );

        // Ensure we cleaned up correctly
        victim.assertCallerAndOrigin(
            address(this), "msg.sender was not cleaned up", oldOrigin, "tx.origin invariant failed"
        );
    }

    function testPrankConstructorSender() public {
        vm.prank(TEST_ADDRESS);
        ConstructorVictim victim = new ConstructorVictim(
            TEST_ADDRESS, "msg.sender was not set during prank", tx.origin, "tx.origin invariant failed"
        );

        // Ensure we cleaned up correctly
        victim.assertCallerAndOrigin(
            address(this), "msg.sender was not cleaned up", tx.origin, "tx.origin invariant failed"
        );
    }

    function testPrankConstructorOrigin() public {
        // Perform the prank
        vm.prank(TEST_ADDRESS, TEST_ORIGIN);
        ConstructorVictim victim = new ConstructorVictim(
            TEST_ADDRESS, "msg.sender was not set during prank", TEST_ORIGIN, "tx.origin was not set during prank"
        );

        // Ensure we cleaned up correctly
        victim.assertCallerAndOrigin(
            address(this), "msg.sender was not cleaned up", tx.origin, "tx.origin was not cleaned up"
        );
    }

    /// Checks that `tx.origin` is set for all subcalls of a `prank`.
    ///
    /// Ref: issue #1210
    function testTxOriginInNestedPrank() public {
        address oldSender = msg.sender;
        address oldOrigin = tx.origin;

        Victim innerVictim = new Victim();
        NestedVictim victim = new NestedVictim(innerVictim);

        vm.prank(TEST_ADDRESS, TEST_ORIGIN);
        victim.assertCallerAndOrigin(
            TEST_ADDRESS, "msg.sender was not set correctly", TEST_ORIGIN, "tx.origin was not set correctly"
        );
    }
}
