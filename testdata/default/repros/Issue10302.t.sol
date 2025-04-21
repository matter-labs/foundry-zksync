// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import "ds-test/test.sol";
import "cheats/Vm.sol";

contract A {
    function foo() public pure returns (bool) {
        return true;
    }
}
// TODO: add back deep prank support, add `true` to startPrank @dustin
// See: https://github.com/matter-labs/foundry-zksync/issues/1027

contract Issue10302Test is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);

    function testDelegateFails() external {
        vm.createSelectFork("sepolia");
        A a = new A();
        vm.startPrank(0x0fe884546476dDd290eC46318785046ef68a0BA9);
        (bool success,) = address(a).delegatecall(abi.encodeWithSelector(A.foo.selector));
        require(success, "Delegate call should succeed");
    }

    function testDelegatePassesWhenBalanceSetToZero() external {
        vm.createSelectFork("sepolia");
        A a = new A();
        vm.startPrank(0x0fe884546476dDd290eC46318785046ef68a0BA9);
        vm.deal(0x0fe884546476dDd290eC46318785046ef68a0BA9, 0 ether);
        (bool success,) = address(a).delegatecall(abi.encodeWithSelector(A.foo.selector));
        vm.stopPrank();
        require(success, "Delegate call should succeed");
    }

    function testDelegateCallSucceeds() external {
        vm.createSelectFork("sepolia");
        A a = new A();
        vm.startPrank(0xd363339eE47775888Df411A163c586a8BdEA9dbf);
        (bool success,) = address(a).delegatecall(abi.encodeWithSelector(A.foo.selector));
        vm.stopPrank();
        require(success, "Delegate call should succeed");
    }

    function testDelegateFailsWhenBalanceGtZero() external {
        vm.createSelectFork("sepolia");
        A a = new A();
        vm.startPrank(0xd363339eE47775888Df411A163c586a8BdEA9dbf);
        vm.deal(0xd363339eE47775888Df411A163c586a8BdEA9dbf, 1 ether);
        (bool success,) = address(a).delegatecall(abi.encodeWithSelector(A.foo.selector));
        vm.stopPrank();
        require(success, "Delegate call should succeed");
    }
}
