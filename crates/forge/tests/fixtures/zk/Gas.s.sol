// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Script} from "forge-std/Script.sol";
import {Greeter} from "src/Greeter.sol";

contract GasScript is Script {
    function run() public {
        vm.startBroadcast();
        Greeter greeter = new Greeter();
        vm.stopBroadcast();
    }
}
