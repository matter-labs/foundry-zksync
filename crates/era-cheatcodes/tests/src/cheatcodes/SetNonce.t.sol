// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract CheatcodeSetNonceTest is Test {
    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;
    uint256 constant NEW_NONCE = uint256(123456);

    function testSetNonce() public {
        (bool success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature(
                "setNonce(address,uint64)",
                TEST_ADDRESS,
                NEW_NONCE
            )
        );
        require(success, "setNonce failed");
        console.log("failed?", failed());
    }
}
