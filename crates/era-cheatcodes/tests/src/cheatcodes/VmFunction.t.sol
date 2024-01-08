// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract VmFunctionTest is Test {
    function testCheatcodeCall() public {
        vm.recordLogs();
    }
}
