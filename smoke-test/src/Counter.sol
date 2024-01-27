// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "forge-std/Test.sol";

contract Counter {
    uint256 public number = 0;

    function increment() public {
        console.log("increment");
        number += 1;
    }

    function incrementBy(uint8 amount) public {
        console.log("incrementBy", amount);
        number += uint256(amount);
    }

    function setNumber(uint256 value) public {
        console.log("setNumber", value);
        number = value;
    }
}

contract CounterTest is Test {
    Counter counter;

    function setUp() public {
        counter = new Counter();

        // exclude these contract addresses from invariant testing
        // using these addresses causes VM to halt.
        excludeSender(address(this));
        excludeSender(address(counter));
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

    function testFuzz_Increment(uint8 amount) public {
        uint256 numBefore = counter.number();
        counter.incrementBy(amount);
        uint256 numAfter = counter.number();
        assertEq(numBefore + amount, numAfter);
    }

    function invariant_alwaysIncrements() external {
        uint256 numBefore = counter.number();
        counter.incrementBy(10);
        uint256 numAfter = counter.number();
        assertGt(numAfter, numBefore);
        counter.increment();
        uint256 numAfterAgain = counter.number();
        assertGt(numAfterAgain, numAfter);
    }
}
