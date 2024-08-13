// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;
import "ds-test/test.sol";
import "../cheats/Vm.sol";
import "./Deposit.sol";

contract ZkInvariantTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);
    // forge-config: default.invariant.runs = 2
    Deposit deposit;

    function setUp() external {
        deposit = new Deposit();
        vm.deal(address(deposit), 100 ether);
    }

    // forge-config: default.invariant.runs = 2
    function testZkInvariantDeposit() external payable {
        deposit.deposit{value: 1 ether}();
        uint256 balanceBefore = deposit.balance(address(this));
        assertEq(balanceBefore, 1 ether);
        deposit.withdraw();
        uint256 balanceAfter = deposit.balance(address(this));
        assertGt(balanceBefore, balanceAfter);
    }

    receive() external payable {}
}
