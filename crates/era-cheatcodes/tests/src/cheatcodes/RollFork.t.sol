// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract RollForkTest is Test {
    uint256 mainnetFork;

    function setUp() public {
        (bool success, bytes memory data) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("createFork(string)", "mainnet")
        );
        require(success, "createFork failed");

        mainnetFork = uint256(bytes32(data));
    }

    // test that we can switch between forks, and "roll" blocks
    function testCanRollFork() public {
        (bool success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("selectFork(uint256)", mainnetFork)
        );
        require(success, "selectFork failed");

        uint256 mainBlock = block.number;

        console.log("target block_number: ", block.number - 1);
        console.log("before block_number: ", block.number);

        (success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("rollFork(uint256)", block.number - 1)
        );
        require(success, "rollFork failed");

        console.log("after block_number: ", block.number);

        assertEq(block.number, mainBlock - 1);

        // can also roll by id
        bytes memory data;
        (success, data) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature(
                "createFork(string,uint256)",
                "mainnet",
                block.number - 1
            )
        );
        require(success, "createFork failed");
        uint256 otherMain = uint256(bytes32(data));

        (success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature(
                "rollFork(uint256,uint256)",
                otherMain,
                mainBlock - 10
            )
        );
        require(success, "rollFork failed");

        console.log("same block_number: ", block.number);
        assertEq(block.number, mainBlock - 1); // should not have rolled

        (success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("selectFork(uint256)", otherMain)
        );
        require(success, "selectFork failed");

        (success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("selectFork(uint256)", otherMain)
        );
        require(success, "selectFork failed");

        console.log("target block_number: ", mainBlock - 10);
        console.log("actual block_number: ", block.number);

        assertEq(block.number, mainBlock - 10);
    }
}
