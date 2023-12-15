// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";

contract Counter {
    uint256 public number = 0;

    function setNumber(uint256 newNumber) public {
        number = newNumber;
    }

    function increment() public {
        console.log("increment");
        number += 1;
    }

    function incrementBy(uint64 amount) public {
        console.log("incrementBy");
        number += uint256(amount);
    }
}

contract CounterTest is Test {
    Counter counter;

    function setUp() public {
        counter = new Counter();
    }

    function test_Increment() public {
        counter.increment();
        if (counter.number() == 1) {
            console.log("[INT-TEST] PASS");
        } else {
            console.log("[INT-TEST] FAIL");
        }
    }

    function test_FailIncrement() public {
        counter.increment();
        assertEq(counter.number(), 200);
    }

    function testFail_Increment() public {
        counter.increment();
        assertEq(counter.number(), 200);
    }

    function testFuzz_Increment(uint64 amount) public {
        uint256 numBefore = counter.number();
        counter.incrementBy(amount);
        uint256 numAfter = counter.number();
        assertEq(numBefore + amount, numAfter);
    }
}
