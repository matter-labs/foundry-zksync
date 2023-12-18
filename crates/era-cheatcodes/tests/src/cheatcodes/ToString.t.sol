// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";
import {Utils} from "./Utils.sol";

contract CheatcodeToStringTest is Test {
    function testToStringFromAddress() external {
        address testAddress = 0x413D15117be7a498e68A64FcfdB22C6e2AaE1808;
        (bool success, bytes memory rawData) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("toString(address)", testAddress)
        );
        require(success, "toString failed");
        bytes memory data = Utils.trimReturnBytes(rawData);
        string memory testString = string(abi.encodePacked(data));
        require(
            keccak256(bytes(testString)) ==
                keccak256(bytes("0x413D15117be7a498e68A64FcfdB22C6e2AaE1808")),
            "toString mismatch"
        );
        console.log("failed?", failed());
    }

    function testToStringFromBool() external {
        (bool success, bytes memory rawData) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("toString(bool)", false)
        );
        require(success, "toString failed");
        bytes memory data = Utils.trimReturnBytes(rawData);
        string memory testString = string(abi.encodePacked(data));
        require(
            keccak256(bytes(testString)) == keccak256(bytes("false")),
            "toString mismatch"
        );

        (success, rawData) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("toString(bool)", true)
        );
        require(success, "toString failed");
        data = Utils.trimReturnBytes(rawData);
        testString = string(abi.encodePacked(data));
        require(
            keccak256(bytes(testString)) == keccak256(bytes("true")),
            "toString mismatch"
        );
        console.log("failed?", failed());
    }

    function testToStringFromUint256() external {
        uint256 value = 99;
        string memory stringValue = "99";
        (bool success, bytes memory rawData) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("toString(uint256)", value)
        );
        require(success, "toString failed");
        bytes memory data = Utils.trimReturnBytes(rawData);
        string memory testString = string(abi.encodePacked(data));
        require(
            keccak256(bytes(testString)) == keccak256(bytes(stringValue)),
            "toString mismatch"
        );
        console.log("failed?", failed());
    }

    function testToStringFromInt256() external {
        int256 value = -99;
        string memory stringValue = "-99";
        (bool success, bytes memory rawData) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("toString(int256)", value)
        );
        require(success, "toString failed");
        bytes memory data = Utils.trimReturnBytes(rawData);
        string memory testString = string(abi.encodePacked(data));
        require(
            keccak256(bytes(testString)) == keccak256(bytes(stringValue)),
            "toString mismatch"
        );
        console.log("failed?", failed());
    }

    function testToStringFromBytes32() external {
        bytes32 testBytes = hex"4ec893b0a778b562e893cee722869c3e924e9ee46ec897cabda6b765a6624324";
        (bool success, bytes memory rawData) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("toString(bytes32)", testBytes)
        );
        require(success, "toString failed");
        bytes memory data = Utils.trimReturnBytes(rawData);
        string memory testString = string(abi.encodePacked(data));
        require(
            keccak256(bytes(testString)) ==
                keccak256(
                    bytes(
                        "0x4ec893b0a778b562e893cee722869c3e924e9ee46ec897cabda6b765a6624324"
                    )
                ),
            "toString mismatch"
        );
        console.log("failed?", failed());
    }

    function testToStringFromBytes() external {
        bytes
            memory testBytes = hex"89987299ea14decf0e11d068474a6e459439802edca8aacf9644222e490d8ef6db";
        (bool success, bytes memory rawData) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("toString(bytes)", testBytes)
        );
        require(success, "toString failed");
        bytes memory data = Utils.trimReturnBytes(rawData);
        string memory testString = string(abi.encodePacked(data));
        require(
            keccak256(bytes(testString)) ==
                keccak256(
                    bytes(
                        "0x89987299ea14decf0e11d068474a6e459439802edca8aacf9644222e490d8ef6db"
                    )
                ),
            "toString mismatch"
        );
        console.log("failed?", failed());
    }
}
