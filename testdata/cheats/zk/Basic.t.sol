// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity 0.8.18;

import "ds-test/test.sol";
import "../Vm.sol";

contract Number {
    function ten() public pure returns (uint8) {
        return 10;
    }
}

contract FixedNumber {
    function five() public pure returns (uint8) {
        return 5;
    }
}

contract CustomNumber {
    uint8 value;

    constructor(uint8 _value) {
        value = _value;
    }
    function number() public view returns (uint8) {
        return value;
    }
}

contract CustomStorage {
    uint8 num;
    string str;

    constructor(string memory _str, uint8 _num) {
        str = _str;
        num = _num;
    }
    function getStr() public view returns (string memory) {
        return str;
    }
    function getNum() public view returns (uint8) {
        return num;
    }
}

contract ZkBasicTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);

    Number number;
    CustomNumber customNumber;

    /// USDC TOKEN
    uint256 constant TOKEN_DECIMALS = 6;

    address constant ERA_TOKEN_ADDRESS =
        0x3355df6D4c9C3035724Fd0e3914dE96A5a83aaf4;
    uint256 constant ERA_FORK_BLOCK = 19579636;
    uint256 constant ERA_FORK_BLOCK_TS = 1700601590;

    address constant ETH_TOKEN_ADDRESS =
        0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48;
    uint256 constant ETH_FORK_BLOCK = 19225195;
    uint256 constant ETH_FORK_BLOCK_TS = 1707901427;

    address constant CONTRACT_ADDRESS =
        0x32400084C286CF3E17e7B677ea9583e60a000324; //zkSync Diamond Proxy
    uint256 constant ERA_BALANCE = 372695034186505563;
    uint256 constant ETH_BALANCE = 153408823439331882193477;

    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;

    uint256 forkEra;
    uint256 forkEth;
    uint256 forkOpt;

    function setUp() public {
        number = new Number();
        customNumber = new CustomNumber(20);
        vm.makePersistent(address(number));
        vm.makePersistent(address(customNumber));

        forkEra = vm.createFork("mainnet", ERA_FORK_BLOCK);
        forkEth = vm.createFork(
            "https://eth-mainnet.alchemyapi.io/v2/Lc7oIGYeL_QvInzI0Wiu_pOZZDEKBrdf",
            ETH_FORK_BLOCK
        );
        forkOpt = vm.createFork(
            "https://mainnet.optimism.io/",
            ETH_FORK_BLOCK
        );
    }

    function testZkBasicBlockNumber() public {
        vm.selectFork(forkEra);
        require(block.number == ERA_FORK_BLOCK, "era block number mismatch");

        vm.selectFork(forkEth);
        require(block.number == ETH_FORK_BLOCK, "eth block number mismatch");
    }

    function testZkBasicBlockTimestamp() public {
        vm.selectFork(forkEra);
        require(
            block.timestamp == ERA_FORK_BLOCK_TS,
            "era block timestamp mismatch"
        );

        vm.selectFork(forkEth);
        require(
            block.timestamp == ETH_FORK_BLOCK_TS,
            "eth block timestamp mismatch"
        );
    }

    function testZkBasicSetUpDeployedContractNoArgs() public {
        require(number.ten() == 10, "base setUp contract value mismatch");

        vm.selectFork(forkEra);
        require(number.ten() == 10, "era setUp contract value mismatch");

        vm.selectFork(forkEth);
        require(number.ten() == 10, "eth setUp contract value mismatch");
    }

    function testZkBasicSetUpDeployedContractArgs() public {
        require(customNumber.number() == 20, "base setUp contract value mismatch");

        vm.selectFork(forkEra);
        require(customNumber.number() == 20, "era setUp contract value mismatch");

        vm.selectFork(forkEth);
        require(customNumber.number() == 20, "eth setUp contract value mismatch");
    }

    function testZkBasicInlineDeployedContractNoArgs() public {
        vm.selectFork(forkEra);
        FixedNumber fixedNumber = new FixedNumber();
        require(fixedNumber.five() == 5, "eera deployed contract value mismatch");
    }

    function testZkBasicAddressBalance() public {
        vm.makePersistent(TEST_ADDRESS);
        vm.deal(TEST_ADDRESS, 100);

        vm.selectFork(forkEra);
        require(
            TEST_ADDRESS.balance == 100,
            "era balance mismatch"
        );

        vm.selectFork(forkEth);
        require(
            TEST_ADDRESS.balance == 100,
            "eth balance mismatch"
        );
    }
}
