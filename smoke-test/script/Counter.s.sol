// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test} from "forge-std/Test.sol";
import {Script} from "forge-std/Script.sol";

contract CounterExample {
    uint256 public number = 0;

    function increment() public {
        number += 1;
    }

    function setNumber(uint256 value) public {
        number = value;
    }
}

contract CounterScript is Script, Test {
    CounterExample counter;

    function setUp() public {
        // Deploy the CounterExample contract
        counter = new CounterExample();
    }

    function run() public {
        // Set the number to a specific value
        uint256 initialValue = 42;
        vm.startBroadcast(address(0xBC989fDe9e54cAd2aB4392Af6dF60f04873A033A));
        counter.setNumber(initialValue);

        // Increment the number
        counter.increment();
        vm.stopBroadcast();
    }
}