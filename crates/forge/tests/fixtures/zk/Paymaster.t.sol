// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import "zksync-contracts/zksync-contracts/l2/system-contracts/Constants.sol";
import {MyPaymaster} from "./MyPaymaster.sol";

contract TestPaymasterFlow is Test {
    MyPaymaster private paymaster;
    DoStuff private do_stuff;
    address private alice;
    address private bob;
    bytes private paymaster_encoded_input;

    function setUp() public {
        alice = makeAddr("Alice");
        bob = makeAddr("Bob");
        do_stuff = new DoStuff();
        paymaster = new MyPaymaster();
        vm.deal(address(paymaster), 10 ether);

        // Encode paymaster input
        paymaster_encoded_input = abi.encodeWithSelector(bytes4(keccak256("general(bytes)")), bytes("0x"));
    }

    function testCallWithPaymaster() public {
        vm.deal(address(do_stuff), 1 ether);
        require(address(do_stuff).balance == 1 ether, "Balance is not 1 ether");
        require(address(alice).balance == 0, "Balance is not 0 ether");

        uint256 alice_balance = address(alice).balance;
        (bool success,) = address(vm).call(
            abi.encodeWithSignature("zkUsePaymaster(address,bytes)", address(paymaster), paymaster_encoded_input)
        );
        require(success, "zkUsePaymaster call failed");

        vm.prank(alice, alice);
        do_stuff.do_stuff(bob);

        require(address(do_stuff).balance == 0, "Balance is not 0 ether");
        require(address(alice).balance == alice_balance, "Balance is not the same");
        require(address(bob).balance == 1 ether, "Balance is not 1 ether");
    }

    function testCreateWithPaymaster() public {
        uint256 alice_balance = address(alice).balance;
        (bool success,) = address(vm).call(
            abi.encodeWithSignature("zkUsePaymaster(address,bytes)", address(paymaster), paymaster_encoded_input)
        );
        require(success, "zkUsePaymaster call failed");

        vm.prank(alice, alice);
        DoStuff new_do_stuff = new DoStuff();

        require(address(alice).balance == alice_balance, "Balance is not the same");
    }

    // We check that the balance of the paymaster does not update
    // because we have an issue where the paymaster balance doesn't get updated
    // within the test environment execution.
    // See original PR: https://github.com/matter-labs/foundry-zksync/pull/591
    function testPaymasterBalanceDoesNotUpdate() public {
        uint256 alice_balance = address(alice).balance;
        uint256 paymaster_balance = address(paymaster).balance;
        (bool success, ) = address(vm).call(
            abi.encodeWithSignature(
                "zkUsePaymaster(address,bytes)",
                address(paymaster),
                paymaster_encoded_input
            )
        );
        require(success, "zkUsePaymaster call failed");

        vm.prank(alice, alice);
        do_stuff.do_stuff(bob);

        require(
            address(alice).balance == alice_balance,
            "Balance is not the same"
        );
        require(
            address(paymaster).balance == paymaster_balance,
            "Paymaster balance is not the same when expected to be the same in execution environment"
        );
    }

    /// forge-config: default.allow_internal_expect_revert = true
    function testRevertsWhenNotUsingPaymaster() public {
        vm.deal(address(do_stuff), 1 ether);
        require(address(alice).balance == 0, "Balance is not 0 ether");
        vm.prank(alice, alice);

        vm.expectRevert();
        do_stuff.do_stuff(bob);
    }
}

contract DoStuff {
    function do_stuff(address recipient) public {
        uint256 amount = address(this).balance;
        (bool success,) = payable(recipient).call{value: amount}("");
        require(success, "Failed to transfer balance to the recipient.");
    }
}
