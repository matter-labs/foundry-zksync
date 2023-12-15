// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.18;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

interface Cheatcodes {
    function expectRevert() external;
    function expectRevert(bytes4 revertData) external;
    function expectRevert(bytes calldata revertData) external;
}

contract Reverter {
    error CustomError();

    function revertWithMessage(string memory message) public pure {
        revert(message);
    }

    function doNotRevert() public pure {}

    function panic() public pure returns (uint256) {
        return uint256(100) - uint256(101);
    }

    function revertWithCustomError() public pure {
        revert CustomError();
    }

    function nestedRevert(Reverter inner, string memory message) public pure {
        inner.revertWithMessage(message);
    }

    function callThenRevert(Dummy dummy, string memory message) public pure {
        dummy.callMe();
        revert(message);
    }

    function revertWithoutReason() public pure {
        revert();
    }
}

contract ConstructorReverter {
    constructor(string memory message) {
        revert(message);
    }
}

/// Used to ensure that the dummy data from `cheatcodes.expectRevert`
/// is large enough to decode big structs.
///
/// The struct is based on issue #2454
struct LargeDummyStruct {
    address a;
    uint256 b;
    bool c;
    address d;
    address e;
    string f;
    address[8] g;
    address h;
    uint256 i;
}

contract Dummy {
    function callMe() public pure returns (string memory) {
        return "thanks for calling";
    }

    function largeReturnType() public pure returns (LargeDummyStruct memory) {
        revert("reverted with large return type");
    }
}

contract ExpectRevertTest is Test {
    Cheatcodes constant cheatcodes = Cheatcodes(Constants.CHEATCODE_ADDRESS);

    function shouldRevert() internal {
        revert();
    }

    function testExpectRevertString() public {
        Reverter reverter = new Reverter();
        cheatcodes.expectRevert("revert");
        reverter.revertWithMessage("revert");
    }

    function testFailExpectRevertWrongString() public {
        Reverter reverter = new Reverter();
        cheatcodes.expectRevert("my not so cool error");
        reverter.revertWithMessage("my cool error");
    }

    function testFailRevertNotOnImmediateNextCall() public {
        Reverter reverter = new Reverter();
        // expectRevert should only work for the next call. However,
        // we do not immediately revert, so,
        // we fail.
        cheatcodes.expectRevert("revert");
        reverter.doNotRevert();
        reverter.revertWithMessage("revert");
    }

    function testExpectRevertConstructor() public {
        cheatcodes.expectRevert("constructor revert");
        new ConstructorReverter("constructor revert");
    }

    function testExpectRevertBuiltin() public {
        Reverter reverter = new Reverter();
        cheatcodes.expectRevert(abi.encodeWithSignature("Panic(uint256)", 0x11));
        reverter.panic();
    }

    function testExpectRevertCustomError() public {
        Reverter reverter = new Reverter();
        cheatcodes.expectRevert(abi.encodePacked(Reverter.CustomError.selector));
        reverter.revertWithCustomError();
    }

    function testExpectRevertNested() public {
        Reverter reverter = new Reverter();
        Reverter inner = new Reverter();
        cheatcodes.expectRevert("nested revert");
        reverter.nestedRevert(inner, "nested revert");
    }

    function testExpectRevertCallsThenReverts() public {
        Reverter reverter = new Reverter();
        Dummy dummy = new Dummy();
        cheatcodes.expectRevert("called a function and then reverted");
        reverter.callThenRevert(dummy, "called a function and then reverted");
    }

    function testDummyReturnDataForBigType() public {
        Dummy dummy = new Dummy();
        cheatcodes.expectRevert("reverted with large return type");
        dummy.largeReturnType();
    }

    function testFailExpectRevertErrorDoesNotMatch() public {
        Reverter reverter = new Reverter();
        cheatcodes.expectRevert("should revert with this message");
        reverter.revertWithMessage("but reverts with this message");
    }

    function testFailExpectRevertDidNotRevert() public {
        Reverter reverter = new Reverter();
        cheatcodes.expectRevert("does not revert, but we think it should");
        reverter.doNotRevert();
    }

    function testExpectRevertNoReason() public {
        Reverter reverter = new Reverter();
        cheatcodes.expectRevert(bytes(""));
        reverter.revertWithoutReason();
    }

    function testExpectRevertAnyRevert() public {
        cheatcodes.expectRevert();
        new ConstructorReverter("hello this is a revert message");

        Reverter reverter = new Reverter();
        cheatcodes.expectRevert();
        reverter.revertWithMessage("this is also a revert message");

        cheatcodes.expectRevert();
        reverter.panic();

        cheatcodes.expectRevert();
        reverter.revertWithCustomError();

        Reverter reverter2 = new Reverter();
        cheatcodes.expectRevert();
        reverter.nestedRevert(reverter2, "this too is a revert message");

        Dummy dummy = new Dummy();
        cheatcodes.expectRevert();
        reverter.callThenRevert(dummy, "revert message 4 i ran out of synonims for also");

        cheatcodes.expectRevert();
        reverter.revertWithoutReason();
    }

    function testFailExpectRevertAnyRevertDidNotRevert() public {
        Reverter reverter = new Reverter();
        cheatcodes.expectRevert();
        reverter.doNotRevert();
    }

    function testFailExpectRevertDangling() public {
        cheatcodes.expectRevert("dangling");
    }
}
