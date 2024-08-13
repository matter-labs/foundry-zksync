// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.0;

import "ds-test/test.sol";
import "../cheats/Vm.sol";

import "./Factory.sol";

contract ZkFactoryTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);

    function testClassicFactory() public {
        MyClassicFactory factory = new MyClassicFactory();
        factory.create(42);

        assert(factory.getNumber() == 42);
    }

    function testConstructorFactory() public {
        MyConstructorFactory factory = new MyConstructorFactory(42);

        assert(factory.getNumber() == 42);
    }

    function testNestedFactory() public {
        MyNestedFactory factory = new MyNestedFactory();
        factory.create(42);

        assert(factory.getNumber() == 42);
    }

    function testNestedConstructorFactory() public {
        MyNestedConstructorFactory factory = new MyNestedConstructorFactory(42);

        assert(factory.getNumber() == 42);
    }

    function testUserFactory() public {
        MyClassicFactory factory = new MyClassicFactory();
        MyUserFactory user = new MyUserFactory();
        user.create(address(factory), 42);

        assert(user.getNumber(address(factory)) == 42);
    }

    function testUserConstructorFactory() public {
        MyConstructorFactory factory = new MyConstructorFactory(42);
        MyUserFactory user = new MyUserFactory();

        assert(user.getNumber(address(factory)) == 42);
    }
}
