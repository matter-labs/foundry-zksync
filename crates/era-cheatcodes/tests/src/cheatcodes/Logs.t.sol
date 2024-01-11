// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, Vm, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract LogsTest is Test {
    event LogTopic1(uint256 indexed topic1, bytes data);

    function testRecordAndGetLogs() public {
        bytes memory testData1 = "test";

        vm.recordLogs();

        Vm.Log[] memory entries;

        emit LogTopic1(1, testData1);

        entries = vm.getRecordedLogs();

        console.log("logs length: %d", entries.length);
        require(entries.length == 1, "wrong number of logs");
    }
}