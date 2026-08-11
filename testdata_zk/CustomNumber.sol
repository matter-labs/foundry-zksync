// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity >=0.8.7 <0.9.0;

contract CustomNumber {
    uint8 value;

    constructor(uint8 _value) {
        value = _value;
    }

    function number() public view returns (uint8) {
        return value;
    }
}
