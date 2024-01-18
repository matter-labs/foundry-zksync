// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract CheatcodeTransactTest is Test {
    /// Random recent block & tx
    uint constant SAMPLE_BLOCK = 23942350;
    bytes32 constant SAMPLE_TX = 0x272c2251368cae9eceaea67f52855c9858fd6b00dd68d6dfadab3ab1d66f9e4b;
    address constant SAMPLE_TX_RECEIVER = 0xC16e4F1237C7d7414a4DED7A4bADB2899AF6e91A;
    uint constant START_BALANCE = 195359993982204;
    uint constant SENT_VALUE = 1990000000000063;

    function setUp() public {
        vm.createSelectFork("mainnet", SAMPLE_BLOCK);

        console.log(SAMPLE_TX_RECEIVER.balance);
        require(SAMPLE_TX_RECEIVER.balance == START_BALANCE, "balance not as expected");
    }

    function testTransact() public {
        console.log("receiver before: ", SAMPLE_TX_RECEIVER.balance);
        vm.transact(SAMPLE_TX);

        console.log("receiver after: ", SAMPLE_TX_RECEIVER.balance);
        require(SAMPLE_TX_RECEIVER.balance == (START_BALANCE + SENT_VALUE), "tx didn't execute");
    }

    function testRollInsteadOfTransact() public {
        vm.roll(SAMPLE_BLOCK + 1);

        console.log(SAMPLE_TX_RECEIVER.balance);
        require(SAMPLE_TX_RECEIVER.balance == (START_BALANCE + SENT_VALUE), "tx didn't execute");
    }
}
