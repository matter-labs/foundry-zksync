// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract CheatcodeRollTest is Test {
    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;
    uint256 constant NEW_BLOCK_NUMBER = 10;

    function testRoll() public {
        uint256 initialBlockNumber = block.number;
        console.log("blockNumber before:", initialBlockNumber);

        require(
            NEW_BLOCK_NUMBER != initialBlockNumber,
            "block number must be different than current block number"
        );

        (bool success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("roll(uint256)", NEW_BLOCK_NUMBER)
        );
        require(success, "roll failed");
        uint256 finalBlockNumber = block.number;
        console.log("blockNumber after :", finalBlockNumber);

        require(
            finalBlockNumber == NEW_BLOCK_NUMBER,
            "block number was not changed"
        );
        console.log("failed?", failed());
    }
}
