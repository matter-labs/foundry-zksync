// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.18;

import "ds-test/test.sol";
import "../default/logs/console.sol";

contract InnerNumber {
    event Value(uint8);

    function innerFive() public returns (uint8) {
        emit Value(5);
        return 5;
    }
}

contract Number {
    function five() public returns (uint8) {
        InnerNumber num = new InnerNumber();
        return num.innerFive();
    }
}

contract Adder {
    function add() public returns (uint8) {
        Number num = new Number();
        return num.five() + num.five();
    }
}

contract ConstructorAdder {
    constructor() {
        Number num = new Number();
        uint8 value = num.five() + num.five();
        console.log(value);
        assert(value == 10);
    }
}

contract ZkTraceTest is DSTest {
    // The test must be run with parameter `-vvv` to print traces
    function testZkTraceOutputDuringCall() public {
        Adder adder = new Adder();
        uint8 value = adder.add();
        assert(value == 10);
        console.log(value);
    }

    function testZkTraceOutputDuringCreate() public {
        new ConstructorAdder();
    }
}
