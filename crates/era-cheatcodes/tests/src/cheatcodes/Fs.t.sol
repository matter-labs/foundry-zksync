// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";
import {Utils} from "./Utils.sol";

contract FsTest is Test {
    function testReadFile() public view {
        string memory path = "src/fixtures/File/read.txt";

        string memory data = vm.readFile(path);

        require(
            keccak256(bytes(data)) ==
                keccak256("hello readable world\nthis is the second line!\n"),
            "read data did not match expected data"
        );
    }

    function testWriteFile() public {
        string memory path = "src/fixtures/File/write_file.txt";
        string memory writeData = "hello writable world";

        vm.writeFile(path, writeData);

        string memory readData = vm.readFile(path);

        require(
            keccak256(bytes(readData)) == keccak256(bytes(writeData)),
            "read data did not match write data"
        );
    }
}
