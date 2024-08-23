// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "ds-test/test.sol";
import "../cheats/Vm.sol";

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

contract Delegator {
    /// Retuns the current `address(this), msg.sender` as a tuple.
    function transact() public view returns (address, address) {
        address thisAddress = address(this);
        address msgSender = msg.sender;
        return (thisAddress, msgSender);
    }
}

contract ZkOwnershipTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);
    address OWNER_ADDRESS = address(0x11abcd);
    address TX_ADDRESS = address(0x22abcd);

    function testZkOwnership() public {
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

    function testZkOwnershipDelegateCall() public {
        Delegator target = new Delegator();
        address thisAddress = address(this);
        address msgSender = msg.sender;

        (bool success, bytes memory data) =
            address(target).delegatecall(abi.encodeWithSelector(target.transact.selector));
        (address thisAddressTx, address msgSenderTx) = abi.decode(data, (address, address));

        assert(success);
        assertEq(thisAddressTx, thisAddress);
        assertEq(msgSenderTx, msgSender);
    }
}
