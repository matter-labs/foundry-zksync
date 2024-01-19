// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console, Vm} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

interface IERC20 {
    function transfer(address to, uint256 amount) external returns (bool);

    function balanceOf(address account) external view returns (uint256);
}

contract CheatcodeTransactTest is Test {
    IERC20 constant USDT = IERC20(0x493257fD37EDB34451f62EDf8D2a0C418852bA4C);

    event Transfer(address indexed from, address indexed to, uint256 value);

    function testTransact() public {
        // A random block https://explorer.zksync.io/block/23942350
        vm.createSelectFork("mainnet", 23942350);

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

    function testTransactCooperatesWithCheatcodes() public {
        // A random block https://explorer.zksync.io/block/21048437
        vm.createSelectFork("mainnet", 21048437);

        // a random ERC20 USDT transfer transaction in the next block: https://explorer.zksync.io/tx/0x2c8a84ec365b39e548e65d82aaf6989a17af2799e29ec537eea6ac7c4fb503b4
        bytes32 transaction = 0x2c8a84ec365b39e548e65d82aaf6989a17af2799e29ec537eea6ac7c4fb503b4;

        address sender = 0x4F170ae33F4e8950a969Ca495544f91254eEA64A;
        address recipient = 0x493257fD37EDB34451f62EDf8D2a0C418852bA4C;

        uint256 senderBalance = USDT.balanceOf(sender);
        uint256 recipientBalance = USDT.balanceOf(recipient);

        console.log("before sender balance: ", senderBalance);
        console.log("before recipient balance: ", recipientBalance);

        assertEq(senderBalance, 17685345938);
        assertEq(recipientBalance, 142926761);

        // transfer amount: 17685.345938 USDT
        uint256 transferAmount = 17685345938;
        uint256 expectedRecipientBalance = recipientBalance + transferAmount;
        uint256 expectedSenderBalance = senderBalance - transferAmount;

        // expect a call to USDT's transfer
        // With the current expect call behavior, in which we expect calls to be matched in the next call's subcalls,
        // expecting calls on vm.transact is impossible. This is because transact essentially creates another call context
        // that operates independently of the current one, meaning that depths won't match and will trigger a panic on REVM,
        // as the transact storage is not persisted as well and can't be checked.
        // vm.expectCall(address(USDT), abi.encodeWithSelector(IERC20.transfer.selector, recipient, transferAmount));

        // expect a Transfer event to be emitted
        vm.expectEmit(true, true, false, true, address(USDT));
        emit Transfer(address(sender), address(recipient), transferAmount);

        // start recording logs
        vm.recordLogs();

        // execute the transaction
        vm.transact(transaction);

        // extract recorded logs
        Vm.Log[] memory logs = vm.getRecordedLogs();

        senderBalance = USDT.balanceOf(sender);
        recipientBalance = USDT.balanceOf(recipient);

        console.log("after sender balance: ", senderBalance);
        console.log("after recipient balance: ", recipientBalance);

        // recipient received transfer
        assertEq(recipientBalance, expectedRecipientBalance);

        // decreased by transferAmount
        assertEq(senderBalance, expectedSenderBalance);

        // recorded a `Transfer` log
        assertEq(logs.length, 1);
    }
}
