// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.13;

import "ds-test/test.sol";
import "../cheats/Vm.sol";

contract Number {
    function one() public pure returns (uint8) {
        return 1;
    }

    function two() public pure returns (uint8) {
        return 2;
    }

    function echo(uint8 value) public pure returns (uint8) {
        return value;
    }
}

/// Additionally validate the inner workings of zk-evm as the bytecode is decommitted only once.
/// When a mock is set, the bytecode is updated in zk-evm memory to simulate a "force return", which
/// could cause issues for any subsequent calls if implemented incorrectly.
contract NumberFactory {
    Number inner;

    constructor(Number _inner) {
        inner = _inner;
    }

    function oneAndTwo() public view returns (uint8, uint8) {
        return (inner.one(), inner.two());
    }

    function echoOneAndTwo() public view returns (uint8, uint8) {
        return (inner.echo(1), inner.echo(2));
    }
}

/// A simple scenario to ensure that the "forced return" functionality of zk-evm works as intended.
contract ZkRetTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);

    function testZkForcedRetOverrideWorks() public {
        Number inner = new Number();
        NumberFactory target = new NumberFactory(inner);
        vm.mockCall(address(inner), abi.encodeWithSelector(inner.one.selector), abi.encode(42));

        (uint8 mockedOne, uint8 two) = target.oneAndTwo();

        assertEq(42, mockedOne);

        assertEq(2, two);
    }

    function testZkForcedRetOverrideWorksWithConstructorArgs() public {
        Number inner = new Number();
        NumberFactory target = new NumberFactory(inner);
        vm.mockCall(address(inner), abi.encodeWithSelector(inner.echo.selector, 1), abi.encode(42));

        (uint8 mockedOne, uint8 two) = target.echoOneAndTwo();

        assertEq(42, mockedOne);

        assertEq(2, two);
    }
}
