// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "ds-test/test.sol";
import "../cheats/Vm.sol";
import "./Deposit.sol";

// partial from forge-std/StdInvariant.sol
abstract contract StdInvariant {
    struct FuzzSelector {
        address addr;
        bytes4[] selectors;
    }

    address[] internal _targetedContracts;

    function targetContracts() public view returns (address[] memory) {
        return _targetedContracts;
    }

    FuzzSelector[] internal _targetedSelectors;

    function targetSelectors() public view returns (FuzzSelector[] memory) {
        return _targetedSelectors;
    }

    address[] internal _targetedSenders;

    function targetSenders() public view returns (address[] memory) {
        return _targetedSenders;
    }
}

contract ZkInvariantTest is DSTest, StdInvariant {
    Vm constant vm = Vm(HEVM_ADDRESS);
    Deposit deposit;

    uint256 constant dealAmount = 1 ether;

    function setUp() external {
        // to fund for fees
        _targetedSenders.push(address(65536 + 1));
        _targetedSenders.push(address(65536 + 12));
        _targetedSenders.push(address(65536 + 123));
        _targetedSenders.push(address(65536 + 1234));

        for (uint256 i = 0; i < _targetedSenders.length; i++) {
            vm.deal(_targetedSenders[i], dealAmount); // to pay fees
        }

        deposit = new Deposit();
        _targetedContracts.push(address(deposit));
    }

    //FIXME: seems to not be detected, forcing values in test config
    // forge-config: default.invariant.runs = 2
    function invariant_itWorks() external payable {}

    receive() external payable {}
}
