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
    event LogTopic1(uint256 indexed topic1, uint256 topic2, bytes data);

    function testRecordAndGetLogs() public {
        bytes memory testData1 = "test";

        (bool success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("recordLogs()")
        );
        require(success, "recordLogs failed");

        emit LogTopic1(2, 1, testData1);

        (bool success2, bytes memory rawData) = Constants
            .CHEATCODE_ADDRESS
            .call(abi.encodeWithSignature("getRecordedLogs()"));
        require(success2, "getRecordedLogs failed");

        Log[] memory logs = abi.decode(rawData, (Log[]));
        require(logs.length == 1, "logs length should be 1");
        // the first topic is the hash of the event signature
        require(logs[0].topics.length == 3, "topics length should be 3");
    }

    function trimReturnBytes(
        bytes memory rawData
    ) internal pure returns (bytes memory) {
        uint256 lengthStartingPos = rawData.length - 32;
        bytes memory lengthSlice = new bytes(32);

        for (uint256 i = 0; i < 32; i++) {
            lengthSlice[i] = rawData[lengthStartingPos + i];
        }

        uint256 length = abi.decode(lengthSlice, (uint256));
        bytes memory data = new bytes(length);

        for (uint256 i = 0; i < length; i++) {
            data[i] = rawData[i];
        }

        return data;
    }
}
