// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

contract Greeter {
    string name;
    uint256 age;

    event Greet(string greet);

    function greeting(string memory _name) public returns (string memory) {
        name = _name;

        string memory greet = string(abi.encodePacked("Hello ", _name));
        emit Greet(greet);

        return greet;
    }

    function greeting2(string memory _name, uint256 n) public returns (uint256) {
        name = _name;

        string memory greet = string(abi.encodePacked("Hello ", _name));
        emit Greet(greet);

        return n * 2;
    }

    function setAge(uint256 _age) public {
        age = _age;
    }

    function getAge() public view returns (uint256) {
        return age;
    }
}
