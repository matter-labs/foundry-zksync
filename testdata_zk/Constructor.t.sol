// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.18;

import "utils/Test.sol";
import "utils/console.sol";
import "./Bank.sol";

contract ZkConstructorTest is Test {
    function testZkConstructorWorksWithValue() public {
        Bank bank = new Bank{value: 1 ether}();
        assertEq(bank.balance(), 1 ether);
    }

    function testZkConstructorWorksWithoutValue() public {
        Bank bank = new Bank();
        assertEq(bank.balance(), 0);
    }
}
