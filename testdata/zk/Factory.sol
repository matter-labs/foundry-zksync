// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.0;

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

    function create(uint256 _number) public {
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

    function getNumber(address classicFactory) public view returns (uint256) {
        return MyClassicFactory(classicFactory).getNumber();
    }
}
