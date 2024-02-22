// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";

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

contract MultiVmBridgeTest is Test {
    Number number;

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

    uint256 forkEra;
    uint256 forkEth;

    function setUp() public {
        number = new Number();
        forkEra = vm.createFork("mainnet", ERA_FORK_BLOCK);
        forkEth = vm.createFork(
            "https://eth-mainnet.alchemyapi.io/v2/Lc7oIGYeL_QvInzI0Wiu_pOZZDEKBrdf",
            ETH_FORK_BLOCK
        );
    }

    function _verifyToken(address tokenAddress) public {
        (bool success, bytes memory data) = tokenAddress.call(
            abi.encodeWithSignature("decimals()")
        );
        require(success, "decimals() failed");
        uint256 decimals = uint256(bytes32(data));
        require(decimals == 6, "decimals() not 6");
    }

    function testBridgeSetUpDeployedContract() public {
        vm.selectFork(forkEra);
        require(number.ten() == 10, "era setUp contract value mismatch");

        vm.selectFork(forkEth);
        require(number.ten() == 10, "eth setUp contract value mismatch");

        // vm.selectFork(forkEra);
    }

    function testBridgeInlineDeployedContractNoArgs() public {
        vm.selectFork(forkEra);
        FixedNumber numberEra = new FixedNumber();
        require(
            numberEra.five() == 5,
            "era inline contract value mismatch (no args)"
        );

        vm.selectFork(forkEth);
        FixedNumber numberEth = new FixedNumber();
        require(
            numberEth.five() == 5,
            "eth inline contract value mismatch (no args)"
        );

        vm.selectFork(forkEra);
    }

    function testBridgeInlineDeployedContractSimpleArgs() public {
        vm.selectFork(forkEra);
        CustomNumber customEra = new CustomNumber(10);
        require(
            customEra.number() == 10,
            "era inline contract value mismatch (simple args)"
        );

        vm.selectFork(forkEth);
        CustomNumber customEth = new CustomNumber(10);
        require(
            customEth.number() == 10,
            "era inline contract value mismatch (simple args)"
        );

        vm.selectFork(forkEra);
    }

    function testBridgeInlineDeployedContractComplexArgs() public {
        vm.selectFork(forkEra);
        CustomStorage customEra = new CustomStorage("hello", 10);
        require(
            keccak256(abi.encodePacked(customEra.getStr())) ==
                keccak256(abi.encodePacked("hello")),
            "era inline contract value mismatch (complex args)"
        );
        require(
            customEra.getNum() == 10,
            "era inline contract value mismatch (complex args)"
        );

        vm.selectFork(forkEth);
        CustomStorage customEth = new CustomStorage("hello", 10);
        require(
            keccak256(abi.encodePacked(customEth.getStr())) ==
                keccak256(abi.encodePacked("hello")),
            "eth inline contract value mismatch (complex args)"
        );
        require(
            customEth.getNum() == 10,
            "era inline contract value mismatch (complex args)"
        );

        vm.selectFork(forkEra);
    }

    function testBridgeBlockNumber() public {
        vm.selectFork(forkEra);
        require(block.number == ERA_FORK_BLOCK, "era block number mismatch");

        vm.selectFork(forkEth);
        require(block.number == ETH_FORK_BLOCK, "eth block number mismatch");

        vm.selectFork(forkEra);
    }

    function testBridgeBlockTimestamp() public {
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

        vm.selectFork(forkEra);
    }

    function testBridgeTransitiveVariables() public {
        vm.selectFork(forkEra);
        uint256 blockEra = block.number;

        vm.selectFork(forkEth);
        uint256 blockEth = block.number + 1; // we add 1 so the compiler doesn't optimize this line as a duplicate of the one above

        vm.selectFork(forkEra);
        require(
            blockEra == ERA_FORK_BLOCK,
            "transitive era block number mismatch"
        );
        require(
            blockEth == ETH_FORK_BLOCK + 1,
            "transitive eth block number mismatch"
        );
    }

    function testBridgeExistingContract() public {
        vm.selectFork(forkEra);
        _verifyToken(ERA_TOKEN_ADDRESS);

        vm.selectFork(forkEth);
        _verifyToken(ETH_TOKEN_ADDRESS);

        vm.selectFork(forkEra);
    }

    function testBridgeBalance() public {
        vm.selectFork(forkEra);
        require(
            CONTRACT_ADDRESS.balance == ERA_BALANCE,
            "era balance mismatch"
        );

        vm.selectFork(forkEth);
        require(
            CONTRACT_ADDRESS.balance == ETH_BALANCE,
            "eth balance mismatch"
        );

        vm.selectFork(forkEra);
    }
}
