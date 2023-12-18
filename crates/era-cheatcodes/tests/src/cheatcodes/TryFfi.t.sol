// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";
import {Utils} from "./Utils.sol";

contract FfiTest is Test {
    struct FfiResult {
        int32 exitCode;
        bytes stdout;
        bytes stderr;
    }

    function testTryFfi() public {
        string[] memory inputs = new string[](3);
        inputs[0] = "bash";
        inputs[1] = "-c";
        inputs[
            2
        ] = "echo -n 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000966666920776f726b730000000000000000000000000000000000000000000000";

        (bool success, bytes memory data) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("tryFfi(string[])", inputs)
        );
        require(success, "tryFfi failed");

        FfiResult memory f = abi.decode(data, (FfiResult));
        string memory output = abi.decode(f.stdout, (string));

        require(
            keccak256(bytes(output)) == keccak256(bytes("ffi works")),
            "ffi failed"
        );
        require(f.exitCode == 0, "ffi failed");
    }

    function testTryFfiFail() public {
        string[] memory inputs = new string[](2);
        inputs[0] = "ls";
        inputs[1] = "wad";

        (bool success, bytes memory data) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("tryFfi(string[])", inputs)
        );
        require(success, "tryFfi failed");

        FfiResult memory f = abi.decode(data, (FfiResult));
        require(f.exitCode != 0, "ffi failed");
    }
}
