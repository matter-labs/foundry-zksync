// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;
import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract Mock {
    uint256 state = 0;

    function numberA() public pure returns (uint256) {
        return 1;
    }

    function numberB() public pure returns (uint256) {
        return 2;
    }

    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }

    function pay(uint256 a) public payable returns (uint256) {
        return a;
    }

    function noReturnValue() public {
        // Does nothing of value, but also ensures that Solidity will 100%
        // generate an `extcodesize` check.
        state += 1;
    }
}

contract NestedMock {
    Mock private inner;

    constructor(Mock _inner) {
        inner = _inner;
    }

    function sum() public view returns (uint256) {
        return inner.numberA() + inner.numberB();
    }
}

contract MockCallTest is Test {
    function testMockGetters() public {
        Mock target = new Mock();

        // pre-mock
        assertEq(target.numberA(), 1);
        assertEq(target.numberB(), 2);

        vm.mockCall(address(target), abi.encodeWithSelector(target.numberB.selector), abi.encode(10));

        // post-mock
        assertEq(target.numberA(), 1);
        assertEq(target.numberB(), 10);
    }

    function testMockNested() public {
        Mock inner = new Mock();
        NestedMock target = new NestedMock(inner);

        // pre-mock
        assertEq(target.sum(), 3);

        vm.mockCall(address(inner), abi.encodeWithSelector(inner.numberB.selector), abi.encode(9));

        // post-mock
        assertEq(target.sum(), 10);
    }

    function testMockSelector() public {
        Mock target = new Mock();
        assertEq(target.add(5, 5), 10);

        vm.mockCall(address(target), abi.encodeWithSelector(target.add.selector), abi.encode(11));

        assertEq(target.add(5, 5), 11);
    }

    function testMockCalldata() public {
        Mock target = new Mock();
        assertEq(target.add(5, 5), 10);
        assertEq(target.add(6, 4), 10);

        vm.mockCall(address(target), abi.encodeWithSelector(target.add.selector, 5, 5), abi.encode(11));

        assertEq(target.add(5, 5), 11);
        assertEq(target.add(6, 4), 10);
    }

    function testClearMockedCalls() public {
        Mock target = new Mock();

        vm.mockCall(address(target), abi.encodeWithSelector(target.numberB.selector), abi.encode(10));

        assertEq(target.numberA(), 1);
        assertEq(target.numberB(), 10);

        vm.clearMockedCalls();

        assertEq(target.numberA(), 1);
        assertEq(target.numberB(), 2);
    }

    function testMockCallMultiplePartialMatch() public {
        Mock mock = new Mock();

        vm.mockCall(address(mock), abi.encodeWithSelector(mock.add.selector), abi.encode(10));
        vm.mockCall(address(mock), abi.encodeWithSelector(mock.add.selector, 2), abi.encode(20));
        vm.mockCall(address(mock), abi.encodeWithSelector(mock.add.selector, 2, 3), abi.encode(30));

        assertEq(mock.add(1, 2), 10);
        assertEq(mock.add(2, 2), 20);
        assertEq(mock.add(2, 3), 30);
    }

    function testMockCallWithValue() public {
        Mock mock = new Mock();
        
        vm.mockCall(address(mock), 10, abi.encodeWithSelector(mock.pay.selector), abi.encode(10));

        assertEq(mock.pay{value: 10}(1), 10);
        assertEq(mock.pay(1), 1);

        for (uint256 i = 0; i < 100; i++) {
            vm.mockCall(address(mock), i, abi.encodeWithSelector(mock.pay.selector), abi.encode(i * 2));
        }

        assertEq(mock.pay(1), 0);
        assertEq(mock.pay{value: 10}(1), 20);
        assertEq(mock.pay{value: 50}(1), 100);
    }

    function testMockCallWithValueCalldataPrecedence() public {
        Mock mock = new Mock();

        vm.mockCall(address(mock), 10, abi.encodeWithSelector(mock.pay.selector), abi.encode(10));
        vm.mockCall(address(mock), abi.encodeWithSelector(mock.pay.selector, 2), abi.encode(2));

        assertEq(mock.pay{value: 10}(1), 10);
        assertEq(mock.pay{value: 10}(2), 2);
        assertEq(mock.pay(2), 2);
    }

    // This fails as calls to empty account cause panic in the VM
    // function testMockCallEmptyAccount() public {
    //     Mock mock = Mock(address(100));

    //     vm.mockCall(address(mock), abi.encodeWithSelector(mock.add.selector), abi.encode(10));
    //     vm.mockCall(address(mock), abi.encodeWithSelector(mock.noReturnValue.selector), abi.encode());

    //     assertEq(mock.add(1, 2), 10);
    //     mock.noReturnValue();
    // }
}