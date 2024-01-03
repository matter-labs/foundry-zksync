// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

struct Log {
    bytes32[] topics;
    bytes data;
    address emitter;
}

contract LogsTest is Test {
    event LogTopic1(uint256 indexed topic1, bytes data);

    function testRecordAndGetLogs() public {
        bytes memory testData1 = "test";

        (bool success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("recordLogs()")
        );
        require(success, "recordLogs failed");

        emit LogTopic1(1, testData1);

        (bool success2, bytes memory rawData) = Constants
            .CHEATCODE_ADDRESS
            .call(abi.encodeWithSignature("getRecordedLogs()"));

        require(success2, "getRecordedLogs failed");

        Log[] memory logs = abi.decode(rawData, (Log[]));
        console.log("logs length: %d", logs.length);
        require(logs.length == 1, "wrong number of logs");
    }
}