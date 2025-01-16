// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.13;

import "ds-test/test.sol";
import "../cheats/Vm.sol";

contract Counter {
    uint256 public number;

    function setNumber(uint256 newNumber) public {
        number = newNumber;
    }
}

contract PayableCounter {
    uint256 public number;

    constructor() payable {}

    function setNumber(uint256 newNumber) public {
        number = newNumber;
    }
}

contract ZkTxSenderTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);

    function testZkTxSenderNoncesAreConsistent() public {
        address msgSender = msg.sender;
        address thisAddr = address(this);

        assertEq(msgSender, tx.origin, "msg.sender and tx.origin must be same for top level");

        uint256 thisAddrTxNonce = vm.zkGetTransactionNonce(thisAddr);
        uint256 thisAddrDeployNonce = vm.zkGetDeploymentNonce(thisAddr);
        uint256 msgSenderTxNonce = vm.zkGetTransactionNonce(msgSender);
        uint256 msgSenderDeployNonce = vm.zkGetDeploymentNonce(msgSender);

        Counter counter = new Counter();
        assertEq(msgSenderTxNonce, vm.zkGetTransactionNonce(msgSender), "deployment#1: msg.sender tx nonce mismatch");
        assertEq(
            msgSenderDeployNonce, vm.zkGetDeploymentNonce(msgSender), "deployment#1: msg.sender deploy nonce mismatch"
        );
        assertEq(thisAddrTxNonce, vm.zkGetTransactionNonce(thisAddr), "deployment#1: self tx nonce mismatch");
        assertEq(thisAddrDeployNonce + 1, vm.zkGetDeploymentNonce(thisAddr), "deployment#1: self deploy nonce mismatch");

        new Counter();
        assertEq(msgSenderTxNonce, vm.zkGetTransactionNonce(msgSender), "deployment#2: msg.sender tx nonce mismatch");
        assertEq(
            msgSenderDeployNonce, vm.zkGetDeploymentNonce(msgSender), "deployment#2: msg.sender deploy nonce mismatch"
        );
        assertEq(thisAddrTxNonce, vm.zkGetTransactionNonce(thisAddr), "deployment#2: self tx nonce mismatch");
        assertEq(thisAddrDeployNonce + 2, vm.zkGetDeploymentNonce(thisAddr), "deployment#2: self deploy nonce mismatch");

        counter.setNumber(0);
        assertEq(msgSenderTxNonce, vm.zkGetTransactionNonce(msgSender), "tx: msg.sender tx nonce mismatch");
        assertEq(msgSenderDeployNonce, vm.zkGetDeploymentNonce(msgSender), "tx: msg.sender deploy nonce mismatch");
        assertEq(thisAddrTxNonce, vm.zkGetTransactionNonce(thisAddr), "tx: self tx nonce mismatch");
        assertEq(thisAddrDeployNonce + 2, vm.zkGetDeploymentNonce(thisAddr), "tx: self deploy nonce mismatch");
    }

    function testZkTxSenderNoncesAreConsistentInBroadcast() public {
        address msgSender = msg.sender;
        address thisAddr = address(this);

        assertEq(msgSender, tx.origin, "msg.sender and tx.origin must be same for top level");

        // Start broadcasting on msg.sender
        vm.startBroadcast(msgSender);

        uint256 thisAddrTxNonce = vm.zkGetTransactionNonce(thisAddr);
        uint256 thisAddrDeployNonce = vm.zkGetDeploymentNonce(thisAddr);
        uint256 msgSenderTxNonce = vm.zkGetTransactionNonce(msgSender);
        uint256 msgSenderDeployNonce = vm.zkGetDeploymentNonce(msgSender);

        Counter counter = new Counter();
        assertEq(
            msgSenderTxNonce + 1, vm.zkGetTransactionNonce(msgSender), "deployment#1: msg.sender tx nonce mismatch"
        );
        assertEq(
            msgSenderDeployNonce + 1,
            vm.zkGetDeploymentNonce(msgSender),
            "deployment#1: msg.sender deploy nonce mismatch"
        );
        assertEq(thisAddrTxNonce, vm.zkGetTransactionNonce(thisAddr), "deployment#1: self tx nonce mismatch");
        assertEq(thisAddrDeployNonce, vm.zkGetDeploymentNonce(thisAddr), "deployment#1: self deploy nonce mismatch");

        new Counter();
        assertEq(
            msgSenderTxNonce + 2, vm.zkGetTransactionNonce(msgSender), "deployment#2: msg.sender tx nonce mismatch"
        );
        assertEq(
            msgSenderDeployNonce + 2,
            vm.zkGetDeploymentNonce(msgSender),
            "deployment#2: msg.sender deploy nonce mismatch"
        );
        assertEq(thisAddrTxNonce, vm.zkGetTransactionNonce(thisAddr), "deployment#2: self tx nonce mismatch");
        assertEq(thisAddrDeployNonce, vm.zkGetDeploymentNonce(thisAddr), "deployment#2: self deploy nonce mismatch");

        counter.setNumber(0);
        assertEq(msgSenderTxNonce + 3, vm.zkGetTransactionNonce(msgSender), "tx: msg.sender tx nonce mismatch");
        assertEq(msgSenderDeployNonce + 2, vm.zkGetDeploymentNonce(msgSender), "tx: msg.sender deploy nonce mismatch");
        assertEq(thisAddrTxNonce, vm.zkGetTransactionNonce(thisAddr), "tx: self tx nonce mismatch");
        assertEq(thisAddrDeployNonce, vm.zkGetDeploymentNonce(thisAddr), "tx: self deploy nonce mismatch");

        vm.stopBroadcast();
    }

    function testZkTxSenderBalancesAreConsistent() public {
        address thisAddr = address(this);
        address msgSender = msg.sender;

        assertEq(msgSender, tx.origin, "msg.sender and tx.origin must be same for top level");

        uint256 thisAddrBalance = thisAddr.balance;
        uint256 msgSenderBalance = msgSender.balance;

        PayableCounter counter = new PayableCounter{value: 1 ether}();
        assertEq(thisAddrBalance - 1 ether, thisAddr.balance);
        assertEq(msgSenderBalance, msgSender.balance);
        assertEq(1 ether, address(counter).balance);

        counter.setNumber(0);
        assertEq(thisAddrBalance - 1 ether, thisAddr.balance);
        assertEq(msgSenderBalance, msgSender.balance);
        assertEq(1 ether, address(counter).balance);
    }
}
