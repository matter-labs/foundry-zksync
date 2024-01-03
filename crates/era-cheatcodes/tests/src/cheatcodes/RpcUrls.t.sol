// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";
import {Utils} from "./Utils.sol";

contract RpcUrlsTest is Test {
    function testRpcUrl() public {
        (bool success, bytes memory rawData) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("rpcUrl(string)", "mainnet")
        );

        bytes memory return_data = Utils.trimReturnBytes(rawData);
        string memory rpc_url = string(return_data);
        console.log("rpc_url", rpc_url);
        require(success, "rpcUrl() failed");
        require(
            keccak256(abi.encodePacked(rpc_url)) ==
                keccak256(
                    abi.encodePacked("https://mainnet.era.zksync.io:443")
                ),
            "rpc url retrieved does not match expected value"
        );
    }

    function testRpcUrls() public {
        (bool success, bytes memory rawData2) = Constants
            .CHEATCODE_ADDRESS
            .call(abi.encodeWithSignature("rpcUrls()"));

        bytes memory return_data2 = Utils.trimReturnBytes(rawData2);
        string memory rpc_urls = string(return_data2);

        console.log("rpc_urls", rpc_urls);

        require(success, "rpcUrls() failed");
        require(
            keccak256(abi.encodePacked(rpc_urls)) ==
                keccak256(
                    abi.encodePacked(
                        "local,mainnet:https://mainnet.era.zksync.io:443,testnet:https://testnet.era.zksync.dev:443"
                    )
                ),
            "rpc urls retrieved does not match expected value"
        );
    }
}
