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
        vm.writeJson(json, path);

        string memory readData = vm.readFile(path);

        require(
            keccak256(bytes(readData)) ==
                keccak256(
                    bytes(
                        '{\n  "boolean": true,\n  "number": 342,\n  "object": {\n    "title": "finally json serialization"\n  }\n}'
                    )
                ),
            "read data did not match write data"
        );

        // Write json to key b
        vm.writeJson(json, path, "b");

        string memory readData2 = vm.readFile(path);

        require(
            keccak256(bytes(readData2)) ==
                keccak256(
                    bytes(
                        '{\n  "boolean": true,\n  "number": 342,\n  "object": {\n    "title": "finally json serialization"\n  },\n  "b": {\n    "boolean": true,\n    "number": 342,\n    "object": {\n      "title": "finally json serialization"\n    }\n  }\n}'
                    )
                ),
            "read data did not match write data"
        );

        // Replace the key b with single value
        vm.writeJson('"test"', path, "b");

        string memory readData3 = vm.readFile(path);

        require(
            keccak256(bytes(readData3)) ==
                keccak256(
                    bytes(
                        '{\n  "boolean": true,\n  "number": 342,\n  "object": {\n    "title": "finally json serialization"\n  },\n  "b": "test"\n}'
                    )
                ),
            "read data did not match write data"
        );
    }
}
