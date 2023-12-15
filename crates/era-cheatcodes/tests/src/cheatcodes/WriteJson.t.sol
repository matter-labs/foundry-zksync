// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";
import {Utils} from "./Utils.sol";

contract FsTest is Test {
    function testWriteJson() public {
        string
            memory json = '{"boolean": true, "number": 342, "object": { "title": "finally json serialization" } }';
        string memory path = "src/fixtures/Json/write_test.json";

        // Write json to file
        (bool success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("writeJson(string,string)", json, path)
        );
        require(success, "writeJson failed");

        bytes memory readRawData;
        (success, readRawData) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("readFile(string)", path)
        );
        require(success, "readFile failed");
        bytes memory readData = Utils.trimReturnBytes(readRawData);

        require(
            keccak256(readData) ==
                keccak256(
                    bytes(
                        '{\n  "boolean": true,\n  "number": 342,\n  "object": {\n    "title": "finally json serialization"\n  }\n}'
                    )
                ),
            "read data did not match write data"
        );

        // Write json to key b
        (success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature(
                "writeJson(string,string,string)",
                json,
                path,
                "b"
            )
        );
        require(success, "writeJson to key failed");

        (success, readRawData) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("readFile(string)", path)
        );
        require(success, "readFile failed");
        readData = Utils.trimReturnBytes(readRawData);

        require(
            keccak256(readData) ==
                keccak256(
                    bytes(
                        '{\n  "boolean": true,\n  "number": 342,\n  "object": {\n    "title": "finally json serialization"\n  },\n  "b": {\n    "boolean": true,\n    "number": 342,\n    "object": {\n      "title": "finally json serialization"\n    }\n  }\n}'
                    )
                ),
            "read data did not match write data"
        );

        // Replace the key b with single value
        (success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature(
                "writeJson(string,string,string)",
                '"test"',
                path,
                "b"
            )
        );
        require(success, "writeJson to key failed");

        (success, readRawData) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("readFile(string)", path)
        );
        require(success, "readFile failed");
        readData = Utils.trimReturnBytes(readRawData);

        require(
            keccak256(readData) ==
                keccak256(
                    bytes(
                        '{\n  "boolean": true,\n  "number": 342,\n  "object": {\n    "title": "finally json serialization"\n  },\n  "b": "test"\n}'
                    )
                ),
            "read data did not match write data"
        );
    }
}
