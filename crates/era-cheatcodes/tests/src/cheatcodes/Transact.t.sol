// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract CheatcodeTransactTest is Test {
    /// USDT Token
    bytes32 constant TOKEN_CREATION_TX = 0x058a0e64a30be0aaa9631427ce9fc15b2908847f42f27f2df723b57ca1ae1368;
    uint constant TOKEN_CREATION_BLOCK = 2719164;
    address constant TOKEN_ADDRESS = 0x493257fD37EDB34451f62EDf8D2a0C418852bA4C;

    address constant L1_ADDRESS = 0xdAC17F958D2ee523a2206206994597C13D831ec7;

    function setUp() public {
        vm.createSelectFork("mainnet", TOKEN_CREATION_BLOCK - 10);

        (bool success, bytes memory data) = TOKEN_ADDRESS.call(abi.encodeWithSignature("l1Address()"));
        require(data.length == 0, "contract shouldn't exist yet");
    }

    function testTransact() public {
        vm.transact(TOKEN_CREATION_TX);

        (bool success, bytes memory data) = TOKEN_ADDRESS.call(abi.encodeWithSignature("l1Address()"));
        (address l1_address) = abi.decode(data, (address));

        require(l1_address == L1_ADDRESS, "contract exists now");
    }
}
