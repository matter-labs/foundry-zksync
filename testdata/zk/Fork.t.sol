// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "ds-test/test.sol";
import "../cheats/Vm.sol";
import {Globals} from "./Globals.sol";

contract Store {
    uint256 a;

    constructor() payable {}

    function set(uint256 _a) public {
        a = _a;
    }

    function get() public view returns (uint256) {
        return a;
    }
}

interface ERC20 {
    function decimals() external returns (uint8);
}

contract ZkForkStorageMigrationTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);
    uint256 constant ETH_FORK_BLOCK = 19225195;
    uint256 constant ERA_FORK_BLOCK = 48517149;

    uint256 forkEth;
    uint256 forkEra;

    // Wrapped native token addresses from https://docs.uniswap.org/contracts/v3/reference/deployments/
    address uniswapEth = 0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2;
    address uniswapEra = 0x5AEa5775959fBC2557Cc8789bC1bf90A239D9a91;

    function setUp() public {
        forkEra = vm.createFork(Globals.ZKSYNC_MAINNET_URL, ERA_FORK_BLOCK);
        forkEth = vm.createFork(Globals.ETHEREUM_MAINNET_URL, ETH_FORK_BLOCK);
    }

    function testForkMigrationExecutesChainNativeCalls() public {
        // assert we have switched to ethereum
        vm.selectFork(forkEth);
        assertEq(18, ERC20(uniswapEth).decimals());

        // assert we have switched to era
        vm.selectFork(forkEra);
        assertEq(18, ERC20(uniswapEra).decimals());
    }

    function testForkMigrationConsistentBalanceAfterForkToZkEvm() public {
        // assert we have switched to ethereum
        vm.selectFork(forkEth);
        assertEq(18, ERC20(uniswapEth).decimals());

        // deploy on EVM
        Store store = new Store{value: 1 ether}();
        store.set(10);
        vm.makePersistent(address(store));

        // assert we have switched to era
        vm.selectFork(forkEra);
        assertEq(18, ERC20(uniswapEra).decimals());

        // assert balance on zkEVM
        assertEq(1 ether, address(store).balance);
    }

    function testForkMigrationConsistentBalanceAfterForkToEvm() public {
        // assert we have switched to era
        vm.selectFork(forkEra);
        assertEq(18, ERC20(uniswapEra).decimals());

        // deploy on zkEVM
        Store store = new Store{value: 1 ether}();
        store.set(10);
        vm.makePersistent(address(store));

        // assert we have switched to ethereum
        vm.selectFork(forkEth);
        assertEq(18, ERC20(uniswapEth).decimals());

        // assert balance on EVM
        assertEq(1 ether, address(store).balance);
    }

    function testForkMigrationConsistentContractCallsAfterForkToZkEvm() public {
        // assert we have switched to ethereum
        vm.selectFork(forkEth);
        assertEq(18, ERC20(uniswapEth).decimals());

        // deploy on EVM
        Store store = new Store{value: 1 ether}();
        store.set(10);
        vm.makePersistent(address(store));

        // assert we have switched to era
        vm.selectFork(forkEra);
        assertEq(18, ERC20(uniswapEra).decimals());

        // assert contract calls on zkEVM
        assertEq(10, store.get());
    }

    function testForkMigrationConsistentContractCallsAfterForkToEvm() public {
        // assert we have switched to era
        vm.selectFork(forkEra);
        assertEq(18, ERC20(uniswapEra).decimals());

        // deploy on zkEVM
        Store store = new Store{value: 1 ether}();
        store.set(10);
        vm.makePersistent(address(store));

        // assert we have switched to ethereum
        vm.selectFork(forkEth);
        assertEq(18, ERC20(uniswapEth).decimals());

        // assert contract calls on EVM
        assertEq(10, store.get());
    }
}
