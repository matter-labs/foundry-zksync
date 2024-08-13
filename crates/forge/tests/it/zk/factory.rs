//! Forge tests for zksync factory contracts.

use super::test_zk;

test_zk!(can_deploy_in_method, "testClassicFactory|testNestedFactory", "ZkFactoryTest");

test_zk!(
    can_deploy_in_constructor,
    "testConstructorFactory|testNestedConstructorFactory",
    "ZkFactoryTest"
);

test_zk!(can_use_predeployed_factory, "testUser.*", "ZkFactoryTest");
