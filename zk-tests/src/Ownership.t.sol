// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test} from "forge-std/Test.sol";

contract MyOwnable {
    address public createOwner;
    address public txOwner;

    constructor() {
        createOwner = msg.sender;
    }

    function transact() public {
        txOwner = msg.sender;
    }
}

contract ZkOwnershipTest is Test {
    address OWNER_ADDRESS = address(0x11abcd);
    address TX_ADDRESS = address(0x22abcd);
    
    function testOwnership() public {
        // set owner balance to 0 to make sure deployment fails
        // if it's used for payment
        vm.deal(OWNER_ADDRESS, 0);
        vm.prank(OWNER_ADDRESS);
        MyOwnable ownable = new MyOwnable();

        vm.deal(TX_ADDRESS, 0);
        vm.prank(TX_ADDRESS);
        ownable.transact();

        
        assertEq(OWNER_ADDRESS, ownable.createOwner());
        assertEq(TX_ADDRESS, ownable.txOwner());
    }
}
