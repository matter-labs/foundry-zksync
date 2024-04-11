// SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.0;

import {Test} from 'forge-std/Test.sol';

/// Set of tests for factory contracts
///
/// *Constructor factories build their dependencies in their constructors
/// *User factories don't deploy but assume the given address to be a deployed factory

contract MyContract {
    uint256 public number;
    constructor(uint256 _number) {
        number = _number;
    }
}

contract MyClassicFactory {
    MyContract item;

    function create(uint256 _number) public {
        item = new MyContract(_number);
    }

    function getNumber() public view returns (uint256) {
        return item.number();
    }
}

contract MyConstructorFactory {
    MyContract item;

    constructor(uint256 _number) {
        item = new MyContract(_number);
    }

    function getNumber() public view returns (uint256) {
        return item.number();
    }
}

contract MyNestedFactory {
    MyClassicFactory nested;

    function create(uint256 _number) {
        nested = new MyClassicFactory();

        nested.create(_number);
    }

    function getNumber() public view returns (uint256) {
        return nested.getNumber();
    }
}

contract MyNestedConstructorFactory {
    MyClassicFactory nested;

    constructor(uint256 _number) {
        nested = new MyClassicFactory();

        nested.create(_number);
    }

    function getNumber() public view returns (uint256) {
        return nested.getNumber();
    }
}

contract MyUserFactory {
    function create(address classicFactory, uint256 _number) public {
        MyClassicFactory(classicFactory).create(_number);
    }

    function getNumber(address classicFactory) public returns (uint256) {
        return MyClassicFactory(classicFactory).getNumber();
    }
}

contract MyUserConstructorFactory {
    constructor(address classicFactory, uint256 _number) {
        MyClassicFactory(classicFactory).create(_number);
    }

    function getNumber(address classicFactory) public returns (uint256) {
        return MyClassicFactory(classicFactory).getNumber();
    }
}

contract ZkFactory is Test {
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
        MyUserFactory user = new MyUserFactory(address(factory));
        user.create(42);

        assert(user.getNumber() == 42);
    }

    function testUserConstructorFactory() public {
        MyClassicFactory factory = new MyClassicFactory();
        MyUserConstructorFactory factory = new MyUserConstructorFactory(address(factory), 42);

        assert(factory.getNumber() == 42);
    }
}
