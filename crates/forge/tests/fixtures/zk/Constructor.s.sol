// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import {Script} from "forge-std/Script.sol";

contract ConstructorScript is Script {
    function run() external {
        vm.startBroadcast();
        
        // Test constructor without value
        Bank bankNoValue = new Bank();
        assert(bankNoValue.balance() == 0);
        
        // Test constructor with 1 ether
        Bank bankWithEther = new Bank{value: 1 ether}();
        assert(bankWithEther.balance() == 1 ether);
        
        // Test constructor with smaller value
        Bank bankSmallValue = new Bank{value: 0.1 ether}();
        assert(bankSmallValue.balance() == 0.1 ether);
        
        vm.stopBroadcast();
    }
}

contract Bank {
    event Received(address sender, uint256 amount);
    
    function balance() public view returns (uint256) {
        return address(this).balance;
    }
    
    constructor() payable {
    }
}
