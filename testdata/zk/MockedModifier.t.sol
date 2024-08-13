// SPDX-License-Identifier: MIT OR Apache-2.0

pragma solidity ^0.8.13;

import "ds-test/test.sol";
import "../cheats/Vm.sol";

contract MockInner {
    // this covers an edge case that mainfests when returning >=5 items
    function mockedMethod() external pure returns (uint256, uint256, uint256, uint256, uint256) {
        // We fail if this function isn't mocked
        assert(false);
        return (0, 0, 0, 0, 0);
    }
}

contract Echoer {
    MockInner internal mockInner;

    struct Foo {
        uint256 foo;
    }

    modifier needsMocking(uint256 n) {
        //we just check that we actually mock the value (to avoid optimization)
        (,, uint256 r,,) = mockInner.mockedMethod();
        assert(r == n);
        _;
    }

    constructor(address _mockInnerAddress) {
        mockInner = MockInner(_mockInnerAddress);
    }

    function echo(uint256 n) external view needsMocking(42) returns (uint256) {
        return n;
    }

    function echo(uint256[] memory n) external view needsMocking(42) returns (uint256[] memory) {
        assert(n.length == 1);
        return n;
    }

    function echo(Foo memory n) external view needsMocking(42) returns (Foo memory) {
        return n;
    }
}

contract MockedModifierTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);

    Echoer target;
    MockInner mockInner;

    function setUp() public {
        mockInner = new MockInner();
        target = new Echoer(address(mockInner));
    }

    function testMockedModifierTestCanMockNumber() public {
        uint256 n = 10;

        vm.mockCall(
            address(mockInner), abi.encodeWithSelector(MockInner.mockedMethod.selector), abi.encode(0, 0, 42, 0, 0, 0)
        );

        assertEq(n, target.echo(n));
    }

    function testMockedModifierTestCanMockArray() public {
        uint256[] memory n = new uint256[](1);
        n[0] = 10;

        vm.mockCall(
            address(mockInner), abi.encodeWithSelector(MockInner.mockedMethod.selector), abi.encode(0, 0, 42, 0, 0, 0)
        );

        assertEq(n[0], target.echo(n)[0]);
    }

    function testMockedModifierTestCanMockStruct() public {
        Echoer.Foo memory n = Echoer.Foo({foo: 10});

        vm.mockCall(
            address(mockInner), abi.encodeWithSelector(MockInner.mockedMethod.selector), abi.encode(0, 0, 42, 0, 0, 0)
        );

        assertEq(n.foo, target.echo(n).foo);
    }
}
