// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test} from "forge-std/Test.sol";
import {Script} from "forge-std/Script.sol";
import {Counter} from "../src/Counter.sol";

contract CounterScript is Script, Test {
    Counter counter;

    function setUp() public {
        // Deploy the Counter contract
        counter = new Counter();
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