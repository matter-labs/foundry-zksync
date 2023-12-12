// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract CheatcodeDealTest is Test {
    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;
    uint256 constant NEW_BALANCE = 10;

    function testDeal() public {
        uint256 balanceBefore = address(TEST_ADDRESS).balance;
        console.log("balance before:", balanceBefore);

        (bool success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature(
                "deal(address,uint256)",
                TEST_ADDRESS,
                NEW_BALANCE
            )
        );
        uint256 balanceAfter = address(TEST_ADDRESS).balance;
        console.log("balance after :", balanceAfter);

        require(balanceAfter == NEW_BALANCE, "balance mismatch");
        require(balanceAfter != balanceBefore, "balance unchanged");
        require(success, "deal failed");
        console.log("failed?", failed());
    }
}
