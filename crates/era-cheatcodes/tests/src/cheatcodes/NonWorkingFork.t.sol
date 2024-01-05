// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract Target {
    function output() public pure returns (uint256) {
        return 1234;
    }
}

contract NonWorkingForkTest is Test {
    uint256 constant FORK_BLOCK = 19579636;

    function testFork() public {
        // Contract gets successfully deployed
        Target target = new Target();
        console.log("target output: ", target.output());

        (bool success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature(
                "createSelectFork(string,uint256)",
                "mainnet",
                FORK_BLOCK
            )
        );
        require(success, "fork failed");

        // After fork, bytecode gets forgotten and contract is not deployed
        Target newTarget = new Target();
        console.log("newTarget output: ", newTarget.output());
    }
}
