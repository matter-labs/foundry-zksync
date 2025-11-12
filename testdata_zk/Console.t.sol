// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.18;

import "utils/Test.sol";
import "utils/console.sol";

contract Printer {
    function print() public view {
        console.log("print");
    }
}

contract ConstructorPrinter {
    constructor() {
        Printer printer = new Printer();
        printer.print();
        console.log("outer print");
        console.logAddress(address(this));
        printer.print();
        console.logBytes1(0xff);
        printer.print();
    }
}

contract ZkConsoleTest is Test {
    function testZkConsoleOutputDuringCall() public {
        vm.zkVm(true);

        Printer printer = new Printer();
        printer.print();
        console.log("outer print");
        console.logAddress(address(this));
        printer.print();
        console.logBytes1(0xff);
        printer.print();
    }

    function testZkConsoleOutputDuringCreate() public {
        vm.zkVm(true);

        new ConstructorPrinter();
    }
}
