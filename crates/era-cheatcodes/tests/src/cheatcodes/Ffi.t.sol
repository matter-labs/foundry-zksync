// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";
import {Utils} from "./Utils.sol";

contract FfiTest is Test {
    function testFfi() public {
        string[] memory inputs = new string[](3);
        inputs[0] = "bash";
        inputs[1] = "-c";
        inputs[
            2
        ] = "echo -n 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000966666920776f726b730000000000000000000000000000000000000000000000";

        (bool success, bytes memory rawData) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("ffi(string[])", inputs)
        );
        require(success, "ffi failed");

        bytes memory data = Utils.trimReturnBytes(rawData);
        string memory output = abi.decode(data, (string));
        require(
            keccak256(bytes(output)) == keccak256(bytes("ffi works")),
            "ffi failed"
        );

        console.log("failed?", failed());
    }

    function testFfiString() public {
        string[] memory inputs = new string[](3);
        inputs[0] = "echo";
        inputs[1] = "-n";
        inputs[2] = "gm";

        (bool success, bytes memory rawData) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("ffi(string[])", inputs)
        );
        require(success, "ffi failed");
        bytes memory data = Utils.trimReturnBytes(rawData);
        require(keccak256(data) == keccak256(bytes("gm")), "ffi failed");
    }
}
