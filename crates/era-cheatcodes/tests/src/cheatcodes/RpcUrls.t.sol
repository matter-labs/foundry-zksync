// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";
import {Utils} from "./Utils.sol";

contract RpcUrlsTest is Test {
    function testRpcUrl() public view {
        string memory rpc_url = vm.rpcUrl("mainnet");
        require(
            keccak256(bytes(rpc_url)) ==
                keccak256(
                    abi.encodePacked("https://mainnet.era.zksync.io:443")
                ),
            "rpc url retrieved does not match expected value"
        );
    }

    function testRpcUrls() public {
        string[2][] memory rpc_urls = vm.rpcUrls();

        require(
            keccak256(bytes(rpc_urls[0][0])) == keccak256("local"),
            "invalid alias for [0]"
        );
        require(
            keccak256(bytes(rpc_urls[0][1])) ==
                keccak256(
                    bytes(
                        vm.envOr(
                            string("ERA_TEST_NODE_RPC_URL"),
                            string("local")
                        )
                    )
                ),
            "invalid url for [0]"
        );
        require(
            keccak256(bytes(rpc_urls[1][0])) == keccak256("mainnet"),
            "invalid alias for [1]"
        );
        require(
            keccak256(bytes(rpc_urls[1][1])) ==
                keccak256("https://mainnet.era.zksync.io:443"),
            "invalid url for [1]"
        );
        require(
            keccak256(bytes(rpc_urls[2][0])) == keccak256("testnet"),
            "invalid alias for [2]"
        );
        require(
            keccak256(bytes(rpc_urls[2][1])) ==
                keccak256("https://testnet.era.zksync.dev:443"),
            "invalid url for [2]"
        );
    }
}
