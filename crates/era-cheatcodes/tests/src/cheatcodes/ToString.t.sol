// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";
import {Utils} from "./Utils.sol";

contract CheatcodeToStringTest is Test {
    function testToStringFromAddress() external pure {
        address testAddress = 0x413D15117be7a498e68A64FcfdB22C6e2AaE1808;
        string memory testString = vm.toString(testAddress);
        require(
            keccak256(bytes(testString)) ==
                keccak256(bytes("0x413D15117be7a498e68A64FcfdB22C6e2AaE1808")),
            "toString mismatch"
        );
    }

    function testToStringFromBool() external pure {
        string memory testString = vm.toString(false);
        require(
            keccak256(bytes(testString)) == keccak256(bytes("false")),
            "toString mismatch"
        );

        string memory testString2 = vm.toString(true);
        require(
            keccak256(bytes(testString2)) == keccak256(bytes("true")),
            "toString mismatch"
        );
    }

    function testToStringFromUint256() external pure {
        uint256 value = 99;
        string memory stringValue = "99";
        string memory testString = vm.toString(value);
        require(
            keccak256(bytes(testString)) == keccak256(bytes(stringValue)),
            "toString mismatch"
        );
    }

    function testToStringFromInt256() external pure {
        int256 value = -99;
        string memory stringValue = "-99";
        string memory testString = vm.toString(value);
        require(
            keccak256(bytes(testString)) == keccak256(bytes(stringValue)),
            "toString mismatch"
        );
    }

    function testToStringFromBytes32() external pure {
        bytes32 testBytes = hex"4ec893b0a778b562e893cee722869c3e924e9ee46ec897cabda6b765a6624324";
        string memory testString = vm.toString(testBytes);
        require(
            keccak256(bytes(testString)) ==
                keccak256(
                    bytes(
                        "0x4ec893b0a778b562e893cee722869c3e924e9ee46ec897cabda6b765a6624324"
                    )
                ),
            "toString mismatch"
        );
    }

    function testToStringFromBytes() external pure {
        bytes
            memory testBytes = hex"89987299ea14decf0e11d068474a6e459439802edca8aacf9644222e490d8ef6db";
        string memory testString = vm.toString(testBytes);
        require(
            keccak256(bytes(testString)) ==
                keccak256(
                    bytes(
                        "0x89987299ea14decf0e11d068474a6e459439802edca8aacf9644222e490d8ef6db"
                    )
                ),
            "toString mismatch"
        );
    }
}
