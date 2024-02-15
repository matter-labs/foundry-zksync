// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console, Vm} from "../../lib/forge-std/src/Test.sol";
// import "./contracts/utils/Strings.sol";

contract Nautilus {

    uint256 _value = 0;

    function set(uint256 val) public {
        _value = val;
    }

    function get() public returns (uint256) {
        return _value;
    }

    function useCheatcode() public {
        uint256 pk = 77814517325470205911140941194401928579557062014761831930645393041380819009408;
        address expected = 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266;

        Vm vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
        address addr = vm.addr(pk);

        assert(addr == expected);
    }
}

contract MultiVMBasicTest is Test {
    /// USDC TOKEN
    uint256 constant TOKEN_DECIMALS = 6;

    address constant ERA_TOKEN_ADDRESS = 0x3355df6D4c9C3035724Fd0e3914dE96A5a83aaf4;
    uint256 constant ERA_FORK_BLOCK = 19579636;

    address constant ETH_TOKEN_ADDRESS = 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48;
    uint256 constant ETH_FORK_BLOCK = 19225195;

    uint256 forkEra;
    uint256 forkEth;

    // We need a way to persist EVM forks and state from `setUp()`, until then we call `_setUp()` manually
    function _setUp() public {
        forkEra = vm.createFork("mainnet", ERA_FORK_BLOCK);
        forkEth = vm.createFork("https://eth-mainnet.alchemyapi.io/v2/Lc7oIGYeL_QvInzI0Wiu_pOZZDEKBrdf", ETH_FORK_BLOCK);
    }

    function _verifyToken(address tokenAddress) public {
        (bool success, bytes memory data) = tokenAddress.call(abi.encodeWithSignature("decimals()"));
        require(success, "decimals() failed");
        uint256 decimals = uint256(bytes32(data));
        require(decimals == 6, "decimals() not 6");
    }
    
    function testSmoke() public {
        // _setUp();
        // console.log(block.number);

        // console.log("check era");
        // vm.selectFork(forkEra);
        // console.log(block.number, ERA_FORK_BLOCK);
        // _verifyToken(ERA_TOKEN_ADDRESS);

        // console.log("check evm");
        forkEth = vm.createFork("https://eth-mainnet.alchemyapi.io/v2/Lc7oIGYeL_QvInzI0Wiu_pOZZDEKBrdf", ETH_FORK_BLOCK);
        vm.selectFork(forkEth);
        console.log(block.number, ETH_FORK_BLOCK);
        // _verifyToken(ETH_TOKEN_ADDRESS);
    }

    function testDeploy() public {
    //    _smokeSetUp();
       //check that we are on eth mainnet
        vm.selectFork(forkEth);
        Nautilus c = new Nautilus();
        c.set(42);
        uint256 val = c.get();
        assert(val == 42);
    }
    function testFailUseCheatcodesInEVM() public {
    //    _smokeSetUp();
       //check that we are on eth mainnet
        vm.selectFork(forkEth);
        Nautilus c = new Nautilus();
        c.useCheatcode();
    }
}