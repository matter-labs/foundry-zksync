// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {NFT} from "../src/NFT.sol";

contract NFTScript is Script {
    function run() external {
        vm.startBroadcast();

        new NFT("NFT_tutorial", "TUT12", "baseUri");
        
        vm.stopBroadcast();
    }
}
