// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "forge-std/Test.sol";

contract Printer {
    function print() public pure {
        console.log("print");
    }
}

contract ZkConsoleTest is Test {
    uint256 constant ERA_FORK_BLOCK = 19579636;
    uint256 constant ERA_FORK_BLOCK_TS = 1700601590;

    uint256 forkEra;

    function setUp() public {
        forkEra = vm.createFork("mainnet", ERA_FORK_BLOCK);
    }

    // The test must be run with parameter `-vv` to print logs
    function testZkConsoleOutput() public {
        (bool success, ) = address(vm).call(
            abi.encodeWithSignature("zkVm(bool)", true)
        );
        require(success, "zkVm() call failed");
        
        Printer printer = new Printer();
        printer.print();
        console.log("outer print");
        console.logAddress(address(this));
        printer.print();
        console.logBytes1(0xff);
        printer.print();
    }
}
