import "forge-std/Script.sol";
import "../src/Factory.sol";

contract ZkClassicFactoryScript is Script {
    function run() external {
        vm.startBroadcast();
        MyClassicFactory factory = new MyClassicFactory();
        factory.create(42);

        vm.stopBroadcast();
        assert(factory.getNumber() == 42);
    }
}

contract ZkConstructorFactoryScript is Script {
    function run() external {
        vm.startBroadcast();
        MyConstructorFactory factory = new MyConstructorFactory(42);

        vm.stopBroadcast();
        assert(factory.getNumber() == 42);
    }
}

contract ZkNestedFactoryScript is Script {
    function run() external {
        vm.startBroadcast();
        MyNestedFactory factory = new MyNestedFactory();
        factory.create(42);

        vm.stopBroadcast();
        assert(factory.getNumber() == 42);
    }
}

contract ZkNestedConstructorFactoryScript is Script {
    function run() external {
        vm.startBroadcast();
        MyNestedConstructorFactory factory = new MyNestedConstructorFactory(42);

        vm.stopBroadcast();
        assert(factory.getNumber() == 42);
    }
}

contract ZkUserFactoryScript is Script {
    function run() external {
        vm.startBroadcast();
        MyClassicFactory factory = new MyClassicFactory();
        MyUserFactory user = new MyUserFactory();
        user.create(address(factory), 42);

        vm.stopBroadcast();
        assert(user.getNumber(address(factory)) == 42);
    }
}

contract ZkUserConstructorFactoryScript is Script {
    function run() external {
        vm.startBroadcast();
        MyConstructorFactory factory = new MyConstructorFactory(42);
        MyUserFactory user = new MyUserFactory();

        vm.stopBroadcast();
        assert(user.getNumber(address(factory)) == 42);
    }
}
