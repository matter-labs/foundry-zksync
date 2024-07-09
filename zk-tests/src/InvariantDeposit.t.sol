// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;
import "forge-std/Test.sol";
import "../src/Deposit.sol";

contract InvariantDeposit is Test {
    // forge-config: default.invariant.runs = 2
    Deposit deposit;

    function setUp() external {
        deposit = new Deposit();
        vm.deal(address(deposit), 100 ether);
    }

    // forge-config: default.invariant.runs = 2
    function invariant_alwaysWithdrawable() external payable {
        deposit.deposit{value: 1 ether}();
        uint256 balanceBefore = deposit.balance(address(this));
        assertEq(balanceBefore, 1 ether);
        deposit.withdraw();
        uint256 balanceAfter = deposit.balance(address(this));
        assertGt(balanceBefore, balanceAfter);
    }

    receive() external payable {}
}
