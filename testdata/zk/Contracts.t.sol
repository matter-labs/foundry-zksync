// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import "ds-test/test.sol";
import "../cheats/Vm.sol";

import {ConstantNumber} from "./ConstantNumber.sol";
import {Globals} from "./Globals.sol";

contract Greeter {
    string name;
    uint256 age;

    event Greet(string greet);

    function greeting(string memory _name) public returns (string memory) {
        name = _name;
        string memory greet = string(abi.encodePacked("Hello ", _name));
        emit Greet(greet);
        return greet;
    }

    function setAge(uint256 _age) public {
        age = _age;
    }

    function getAge() public view returns (uint256) {
        return age;
    }
}

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

contract PayableFixedNumber {
    address sender;
    uint256 value;

    constructor() payable {
        sender = msg.sender;
        value = msg.value;
    }

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

contract ZkContractsTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);

    Number number;
    CustomNumber customNumber;

    uint256 constant ERA_FORK_BLOCK = 19579636;
    uint256 constant ERA_FORK_BLOCK_TS = 1700601590;

    uint256 constant ETH_FORK_BLOCK = 19225195;
    uint256 constant ETH_FORK_BLOCK_TS = 1707901427;

    uint256 forkEra;
    uint256 forkEth;

    function setUp() public {
        number = new Number();
        customNumber = new CustomNumber(20);
        vm.makePersistent(address(number));
        vm.makePersistent(address(customNumber));

        forkEra = vm.createFork(Globals.ZKSYNC_MAINNET_URL, ERA_FORK_BLOCK);
        forkEth = vm.createFork(Globals.ETHEREUM_MAINNET_URL, ETH_FORK_BLOCK);
    }

    function testZkContractsPersistedDeployedContractNoArgs() public {
        require(number.ten() == 10, "base setUp contract value mismatch");

        vm.selectFork(forkEra);
        require(number.ten() == 10, "era setUp contract value mismatch");

        vm.selectFork(forkEth);
        require(number.ten() == 10, "eth setUp contract value mismatch");
    }

    function testZkContractsPersistedDeployedContractArgs() public {
        require(customNumber.number() == 20, "base setUp contract value mismatch");

        vm.selectFork(forkEra);
        require(customNumber.number() == 20, "era setUp contract value mismatch");

        vm.selectFork(forkEth);
        require(customNumber.number() == 20, "eth setUp contract value mismatch");
    }

    function testZkContractsInlineDeployedContractNoArgs() public {
        vm.selectFork(forkEra);
        FixedNumber fixedNumber = new FixedNumber();
        require(fixedNumber.five() == 5, "era deployed contract value mismatch");
    }

    function testZkContractsInlineDeployedContractBalance() public {
        vm.selectFork(forkEra);
        PayableFixedNumber payableFixedNumber = new PayableFixedNumber{value: 10}();
        require(address(payableFixedNumber).balance == 10, "incorrect balance");
    }

    function testZkContractsInlineDeployedContractComplexArgs() public {
        CustomStorage customStorage = new CustomStorage("hello", 10);
        vm.makePersistent(address(customStorage));
        require(
            keccak256(abi.encodePacked(customStorage.getStr())) == keccak256(abi.encodePacked("hello")),
            "base inline contract value mismatch (complex args)"
        );
        require(customStorage.getNum() == 10, "base inline contract value mismatch (complex args)");

        vm.selectFork(forkEra);
        require(
            keccak256(abi.encodePacked(customStorage.getStr())) == keccak256(abi.encodePacked("hello")),
            "era inline contract value mismatch (complex args)"
        );
        require(customStorage.getNum() == 10, "era inline contract value mismatch (complex args)");

        vm.selectFork(forkEth);
        require(
            keccak256(abi.encodePacked(customStorage.getStr())) == keccak256(abi.encodePacked("hello")),
            "eth inline contract value mismatch (complex args)"
        );
        require(customStorage.getNum() == 10, "era inline contract value mismatch (complex args)");
    }

    function testZkContractsCreate2() public {
        vm.selectFork(forkEra);

        string memory artifact = vm.readFile("zk/zkout/ConstantNumber.sol/ConstantNumber.json");
        bytes32 bytecodeHash = vm.parseJsonBytes32(artifact, ".hash");
        address sender = address(0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496);
        bytes32 salt = "12345";
        bytes32 constructorInputHash = keccak256(abi.encode());
        address expectedDeployedAddress =
            _computeCreate2Address(sender, salt, bytes32(bytecodeHash), constructorInputHash);

        // deploy via create2
        address actualDeployedAddress = address(new ConstantNumber{salt: salt}());

        assertEq(expectedDeployedAddress, actualDeployedAddress);
    }

    function testZkContractsMultipleTransactions() external {
        vm.zkVm(true);
        Greeter greeter = new Greeter();
        greeter.setAge(10);
        string memory greeting = greeter.greeting("john");
        assertEq("Hello john", greeting);
        greeter.setAge(60);
        assertEq(60, greeter.getAge());
    }

    function _computeCreate2Address(
        address sender,
        bytes32 salt,
        bytes32 creationCodeHash,
        bytes32 constructorInputHash
    ) private pure returns (address) {
        bytes32 zksync_create2_prefix = keccak256("zksyncCreate2");
        bytes32 address_hash = keccak256(
            bytes.concat(
                zksync_create2_prefix, bytes32(uint256(uint160(sender))), salt, creationCodeHash, constructorInputHash
            )
        );

        return address(uint160(uint256(address_hash)));
    }
}
