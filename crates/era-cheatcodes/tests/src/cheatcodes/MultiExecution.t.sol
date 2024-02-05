// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;
import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract Counter {
    uint256 public number = 0;

    function increment() public {
        number += 1;
    }
}

contract CounterTest is Test {
    Counter counter;
    uint256 mainnetFork;  // zk mainnet
    uint256 arbitrumFork; // evm-compatible chain

    function setUp() public {
        counter = new Counter();
        mainnetFork = vm.createSelectFork(vm.rpcUrl('mainnet'), 16128510);
        arbitrumFork = vm.createSelectFork(vm.rpcUrl('arbitrum'), 76261612);
    }

    function test_Bridge() public {
        counter.increment();
        require(counter.number() == 1);

        vm.selectFork(arbitrumFork);
        counter.increment();
        require(counter.number() == 2);

        vm.selectFork(mainnetFork);
        counter.increment();
        require(counter.number() == 3);
    }
}