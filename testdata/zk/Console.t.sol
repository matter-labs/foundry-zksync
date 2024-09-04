// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.18;

import "ds-test/test.sol";
import "../cheats/Vm.sol";
import "../default/logs/console.sol";

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

contract ZkConsoleTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);

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
