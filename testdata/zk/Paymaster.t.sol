// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import "../lib/zksync-contracts/zksync-contracts/l2/system-contracts/Constants.sol";
import {MyPaymaster, MyERC20} from "./MyPaymaster.sol";

contract TestPaymasterFlow is Test {
    MyERC20 private erc20;
    MyPaymaster private paymaster;
    DoStuff private do_stuff;
    address private alice;
    bytes private paymaster_encoded_input;

    function setUp() public {
        alice = makeAddr("Alice");
        do_stuff = new DoStuff();
        erc20 = new MyERC20("Test", "JR", 1);
        paymaster = new MyPaymaster(address(erc20));

        // Initial funding
        vm.deal(address(do_stuff), 1 ether);
        vm.deal(alice, 1 ether);
        vm.deal(address(paymaster), 10 ether);

        // Mint and approve ERC20 tokens
        erc20.mint(alice, 1);
        vm.prank(alice, alice);
        erc20.approve(address(paymaster), 1);

        // Encode paymaster input
        paymaster_encoded_input = abi.encodeWithSelector(
            bytes4(keccak256("approvalBased(address,uint256,bytes)")),
            address(erc20),
            uint256(1),
            bytes("0x")
        );
    }

    function testCallWithPaymaster() public {
        require(address(do_stuff).balance == 1 ether, "Balance is not 1 ether");

        uint256 alice_balance = address(alice).balance;
        (bool success, ) = address(vm).call(
            abi.encodeWithSignature(
                "zkUsePaymaster(address,bytes)",
                address(paymaster),
                paymaster_encoded_input
            )
        );
        require(success, "zkUsePaymaster call failed");

        vm.prank(alice, alice);
        do_stuff.do_stuff();

        require(address(do_stuff).balance == 0, "Balance is not 0 ether");
        require(address(alice).balance == alice_balance, "Balance is not the same");
    }

    function testCreateWithPaymaster() public {
        uint256 alice_balance = address(alice).balance;
        (bool success, ) = address(vm).call(
            abi.encodeWithSignature(
                "zkUsePaymaster(address,bytes)",
                address(paymaster),
                paymaster_encoded_input
            )
        );
        require(success, "zkUsePaymaster call failed");

        vm.prank(alice, alice);
        DoStuff new_do_stuff = new DoStuff();

        require(address(alice).balance == alice_balance, "Balance is not the same");
    }

    function testFailPaymasterBalanceDoesNotUpdate() public {
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
        do_stuff.do_stuff();

        require(address(alice).balance == alice_balance, "Balance is not the same");
        require(address(paymaster).balance < paymaster_balance, "Paymaster balance is not less");
    }
}

contract DoStuff {
    function do_stuff() public {
        (bool success, ) = payable(BOOTLOADER_FORMAL_ADDRESS).call{
            value: address(this).balance
        }("");
        require(success, "Failed to transfer tx fee to the bootloader. Paymaster balance might not be enough.");
    }
}
