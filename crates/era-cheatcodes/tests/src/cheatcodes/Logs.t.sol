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

    event LogTopic2(uint256 indexed topic1, bytes data);

    function testRecordAndGetLogs() public {
        bytes memory testData1 = "test";

        Emitter emitter = new Emitter();

        (bool success, ) = Constants.CHEATCODE_ADDRESS.call(    
            abi.encodeWithSignature("expectEmit()")
        );

        emit LogTopic1(2, 1, testData1);
        //calls
        //calls
        emit LogTopic2(7, testData1);
 
        //For some reason this is logging first
        emitter.emitEvent(1, 254, 255, testData1);
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

contract Emitter {
    uint256 public thing;

    event Something(
        uint256 indexed topic1,
        uint256 indexed topic2,
        uint256 indexed topic3,
        bytes data
    );

    function emitEvent(
        uint256 topic1,
        uint256 topic2,
        uint256 topic3,
        bytes memory data
    ) public {
        emit Something(topic1, topic2, topic3, data);
    }
}
