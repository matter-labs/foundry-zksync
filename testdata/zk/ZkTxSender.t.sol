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
        address thisAddr = address(this);
        address msgSender = msg.sender;

        assertEq(
            msgSender,
            tx.origin,
            "msg.sender and tx.origin must be same for top level"
        );

        uint256 thisAddrTxNonce = vm.zkGetTransactionNonce(thisAddr);
        uint256 thisAddrDeployNonce = vm.zkGetDeploymentNonce(thisAddr);
        uint256 msgSenderTxNonce = vm.zkGetTransactionNonce(msgSender);
        uint256 msgSenderDeployNonce = vm.zkGetDeploymentNonce(msgSender);

        Counter counter = new Counter();
        assertEq(thisAddrTxNonce, vm.zkGetTransactionNonce(thisAddr));
        assertEq(thisAddrDeployNonce, vm.zkGetDeploymentNonce(thisAddr));
        assertEq(msgSenderTxNonce, vm.zkGetTransactionNonce(thisAddr));
        assertEq(msgSenderDeployNonce + 1, vm.zkGetDeploymentNonce(thisAddr));

        new Counter();
        assertEq(thisAddrTxNonce, vm.zkGetTransactionNonce(thisAddr));
        assertEq(thisAddrDeployNonce, vm.zkGetDeploymentNonce(thisAddr));
        assertEq(msgSenderTxNonce, vm.zkGetTransactionNonce(thisAddr));
        assertEq(msgSenderDeployNonce + 2, vm.zkGetDeploymentNonce(thisAddr));

        counter.setNumber(0);
        assertEq(thisAddrTxNonce, vm.zkGetTransactionNonce(thisAddr));
        assertEq(thisAddrDeployNonce, vm.zkGetDeploymentNonce(thisAddr));
        assertEq(msgSenderTxNonce + 1, vm.zkGetTransactionNonce(thisAddr));
        assertEq(msgSenderDeployNonce + 2, vm.zkGetDeploymentNonce(thisAddr));
    }

    function testZkTxSenderNoncesAreConsistentInBroadcast() public {
        address thisAddr = address(this);
        address msgSender = msg.sender;

        assertEq(
            msgSender,
            tx.origin,
            "msg.sender and tx.origin must be same for top level"
        );

        // Start broadcasting on msg.sender
        vm.startBroadcast(msgSender);

        uint256 thisAddrTxNonce = vm.zkGetTransactionNonce(thisAddr);
        uint256 thisAddrDeployNonce = vm.zkGetDeploymentNonce(thisAddr);
        uint256 msgSenderTxNonce = vm.zkGetTransactionNonce(msgSender);
        uint256 msgSenderDeployNonce = vm.zkGetDeploymentNonce(msgSender);

        Counter counter = new Counter();
        assertEq(thisAddrTxNonce + 1, vm.zkGetTransactionNonce(thisAddr));
        assertEq(thisAddrDeployNonce + 1, vm.zkGetDeploymentNonce(thisAddr));
        assertEq(msgSenderTxNonce, vm.zkGetTransactionNonce(thisAddr));
        assertEq(msgSenderDeployNonce, vm.zkGetDeploymentNonce(thisAddr));

        new Counter();
        assertEq(thisAddrTxNonce + 2, vm.zkGetTransactionNonce(thisAddr));
        assertEq(thisAddrDeployNonce + 2, vm.zkGetDeploymentNonce(thisAddr));
        assertEq(msgSenderTxNonce, vm.zkGetTransactionNonce(thisAddr));
        assertEq(msgSenderDeployNonce, vm.zkGetDeploymentNonce(thisAddr));

        counter.setNumber(0);
        assertEq(thisAddrTxNonce + 3, vm.zkGetTransactionNonce(thisAddr));
        assertEq(thisAddrDeployNonce + 2, vm.zkGetDeploymentNonce(thisAddr));
        assertEq(msgSenderTxNonce, vm.zkGetTransactionNonce(thisAddr));
        assertEq(msgSenderDeployNonce, vm.zkGetDeploymentNonce(thisAddr));

        vm.stopBroadcast();
    }

    function testZkTxSenderBalancesAreConsistent() public {
        address thisAddr = address(this);
        address msgSender = msg.sender;

        assertEq(
            msgSender,
            tx.origin,
            "msg.sender and tx.origin must be same for top level"
        );

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
