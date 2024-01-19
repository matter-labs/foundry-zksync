// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract CheatcodeTransactTest is Test {
    function setUp() public {
        vm.createSelectFork("mainnet", 23942350);
    }

    function testTransact() public {
        // A random block https://explorer.zksync.io/block/23942350
        // vm.createSelectFork("mainnet", 23942350);

        // A random transfer in the next block
        bytes32 transaction = 0x272c2251368cae9eceaea67f52855c9858fd6b00dd68d6dfadab3ab1d66f9e4b;
        address sender = 0xE4eDb277e41dc89aB076a1F049f4a3EfA700bCE8;
        address recipient = 0xC16e4F1237C7d7414a4DED7A4bADB2899AF6e91A;

        console.log("before sender balance: ", sender.balance);
        console.log("before recipient balance: ", recipient.balance);

        assertEq(sender.balance, 152522463532909498719);
        assertEq(recipient.balance, 195359993982204);

        // Transfer amount: 0.001990000000000063 Ether
        uint256 transferAmount = 1990000000000063;
        uint256 expectedRecipientBalance = recipient.balance + transferAmount;
        uint256 expectedSenderBalance = sender.balance - transferAmount;

        // Execute the transaction
        vm.transact(transaction);

        console.log("after sender balance: ", sender.balance);
        console.log("after recipient balance: ", recipient.balance);

        // Recipient received transfer
        assertEq(recipient.balance, expectedRecipientBalance);

        // Sender balance decreased by transferAmount and gas
        assert(sender.balance < expectedSenderBalance);
    }
}
