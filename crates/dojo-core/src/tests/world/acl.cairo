use starknet::contract_address_const;

use dojo::model::Model;
use dojo::utils::{bytearray_hash, entity_id_from_keys};
use dojo::world::{IWorldDispatcher, IWorldDispatcherTrait, world};

use dojo::tests::helpers::{
    deploy_world, Foo, foo, foo_setter, IFooSetterDispatcher, IFooSetterDispatcherTrait
};

#[test]
fn test_owner() {
    let world = deploy_world();
    world.register_model(foo::TEST_CLASS_HASH.try_into().unwrap());
    let foo_selector = Model::<Foo>::selector();

    let alice = starknet::contract_address_const::<0xa11ce>();
    let bob = starknet::contract_address_const::<0xb0b>();

    assert(!world.is_owner(0, alice), 'should not be owner');
    assert(!world.is_owner(foo_selector, bob), 'should not be owner');

    world.grant_owner(0, alice);
    assert(world.is_owner(0, alice), 'should be owner');

    world.grant_owner(foo_selector, bob);
    assert(world.is_owner(foo_selector, bob), 'should be owner');

    world.revoke_owner(0, alice);
    assert(!world.is_owner(0, alice), 'should not be owner');

    world.revoke_owner(foo_selector, bob);
    assert(!world.is_owner(foo_selector, bob), 'should not be owner');
}


#[test]
#[should_panic(expected: ("Resource `42` is not registered", 'ENTRYPOINT_FAILED'))]
fn test_grant_owner_not_registered_resource() {
    let world = deploy_world();

    // 42 is not a registered resource ID
    world.grant_owner(42, 69.try_into().unwrap());
}

#[test]
#[should_panic(
    expected: (
        "Caller `57005` is not the owner of the resource `3123252206139358744730647958636922105676576163624049771737508399526017186883`",
        'ENTRYPOINT_FAILED'
    )
)]
fn test_grant_owner_through_malicious_contract() {
    let world = deploy_world();
    world.register_model(foo::TEST_CLASS_HASH.try_into().unwrap());
    let foo_selector = Model::<Foo>::selector();

    let alice = starknet::contract_address_const::<0xa11ce>();
    let bob = starknet::contract_address_const::<0xb0b>();
    let malicious_contract = starknet::contract_address_const::<0xdead>();

    world.grant_owner(foo_selector, alice);

    starknet::testing::set_account_contract_address(alice);
    starknet::testing::set_contract_address(malicious_contract);

    world.grant_owner(foo_selector, bob);
}

#[test]
#[should_panic(
    expected: (
        "Caller `659918` is not the owner of the resource `3123252206139358744730647958636922105676576163624049771737508399526017186883`",
        'ENTRYPOINT_FAILED'
    )
)]
fn test_grant_owner_fails_for_non_owner() {
    let world = deploy_world();
    world.register_model(foo::TEST_CLASS_HASH.try_into().unwrap());
    let foo_selector = Model::<Foo>::selector();

    let alice = starknet::contract_address_const::<0xa11ce>();
    let bob = starknet::contract_address_const::<0xb0b>();

    starknet::testing::set_account_contract_address(alice);
    starknet::testing::set_contract_address(alice);

    world.grant_owner(foo_selector, bob);
}

#[test]
#[should_panic(
    expected: (
        "Caller `57005` is not the owner of the resource `3123252206139358744730647958636922105676576163624049771737508399526017186883`",
        'ENTRYPOINT_FAILED'
    )
)]
fn test_revoke_owner_through_malicious_contract() {
    let world = deploy_world();
    world.register_model(foo::TEST_CLASS_HASH.try_into().unwrap());
    let foo_selector = Model::<Foo>::selector();

    let alice = starknet::contract_address_const::<0xa11ce>();
    let bob = starknet::contract_address_const::<0xb0b>();
    let malicious_contract = starknet::contract_address_const::<0xdead>();

    world.grant_owner(foo_selector, alice);
    world.grant_owner(foo_selector, bob);

    starknet::testing::set_account_contract_address(alice);
    starknet::testing::set_contract_address(malicious_contract);

    world.revoke_owner(foo_selector, bob);
}

#[test]
#[should_panic(
    expected: (
        "Caller `659918` is not the owner of the resource `3123252206139358744730647958636922105676576163624049771737508399526017186883`",
        'ENTRYPOINT_FAILED'
    )
)]
fn test_revoke_owner_fails_for_non_owner() {
    let world = deploy_world();
    world.register_model(foo::TEST_CLASS_HASH.try_into().unwrap());
    let foo_selector = Model::<Foo>::selector();

    let alice = starknet::contract_address_const::<0xa11ce>();
    let bob = starknet::contract_address_const::<0xb0b>();

    world.grant_owner(foo_selector, bob);

    starknet::testing::set_account_contract_address(alice);
    starknet::testing::set_contract_address(alice);

    world.revoke_owner(foo_selector, bob);
}

#[test]
#[available_gas(6000000)]
fn test_writer() {
    let world = deploy_world();
    world.register_model(foo::TEST_CLASS_HASH.try_into().unwrap());
    let foo_selector = Model::<Foo>::selector();

    assert(!world.is_writer(foo_selector, 69.try_into().unwrap()), 'should not be writer');

    world.grant_writer(foo_selector, 69.try_into().unwrap());
    assert(world.is_writer(foo_selector, 69.try_into().unwrap()), 'should be writer');

    world.revoke_writer(foo_selector, 69.try_into().unwrap());
    assert(!world.is_writer(foo_selector, 69.try_into().unwrap()), 'should not be writer');
}

#[test]
fn test_writer_not_registered_resource() {
    let world = deploy_world();

    // 42 is not a registered resource ID
    !world.is_writer(42, 69.try_into().unwrap());
}

#[test]
#[should_panic(
    expected: (
        "Caller `57005` is not the owner of the resource `3123252206139358744730647958636922105676576163624049771737508399526017186883`",
        'ENTRYPOINT_FAILED'
    )
)]
fn test_grant_writer_through_malicious_contract() {
    let world = deploy_world();
    world.register_model(foo::TEST_CLASS_HASH.try_into().unwrap());
    let foo_selector = Model::<Foo>::selector();

    let alice = starknet::contract_address_const::<0xa11ce>();
    let bob = starknet::contract_address_const::<0xb0b>();
    let malicious_contract = starknet::contract_address_const::<0xdead>();

    world.grant_owner(foo_selector, alice);

    starknet::testing::set_account_contract_address(alice);
    starknet::testing::set_contract_address(malicious_contract);

    world.grant_writer(foo_selector, bob);
}

#[test]
#[should_panic(
    expected: (
        "Caller `659918` is not the owner of the resource `3123252206139358744730647958636922105676576163624049771737508399526017186883`",
        'ENTRYPOINT_FAILED'
    )
)]
fn test_grant_writer_fails_for_non_owner() {
    let world = deploy_world();
    world.register_model(foo::TEST_CLASS_HASH.try_into().unwrap());
    let foo_selector = Model::<Foo>::selector();

    let alice = starknet::contract_address_const::<0xa11ce>();
    let bob = starknet::contract_address_const::<0xb0b>();

    starknet::testing::set_account_contract_address(alice);
    starknet::testing::set_contract_address(alice);

    world.grant_writer(foo_selector, bob);
}

#[test]
#[should_panic(
    expected: (
        "Caller `57005` is not the owner of the resource `3123252206139358744730647958636922105676576163624049771737508399526017186883`",
        'ENTRYPOINT_FAILED'
    )
)]
fn test_revoke_writer_through_malicious_contract() {
    let world = deploy_world();
    world.register_model(foo::TEST_CLASS_HASH.try_into().unwrap());
    let foo_selector = Model::<Foo>::selector();

    let alice = starknet::contract_address_const::<0xa11ce>();
    let bob = starknet::contract_address_const::<0xb0b>();
    let malicious_contract = starknet::contract_address_const::<0xdead>();

    world.grant_owner(foo_selector, alice);
    world.grant_writer(foo_selector, bob);

    starknet::testing::set_account_contract_address(alice);
    starknet::testing::set_contract_address(malicious_contract);

    world.revoke_writer(foo_selector, bob);
}

#[test]
#[should_panic(
    expected: (
        "Caller `659918` is not the owner of the resource `3123252206139358744730647958636922105676576163624049771737508399526017186883`",
        'ENTRYPOINT_FAILED'
    )
)]
fn test_revoke_writer_fails_for_non_owner() {
    let world = deploy_world();
    world.register_model(foo::TEST_CLASS_HASH.try_into().unwrap());
    let foo_selector = Model::<Foo>::selector();

    let alice = starknet::contract_address_const::<0xa11ce>();
    let bob = starknet::contract_address_const::<0xb0b>();

    world.grant_writer(foo_selector, bob);

    starknet::testing::set_account_contract_address(alice);
    starknet::testing::set_contract_address(alice);

    world.revoke_writer(foo_selector, bob);
}

#[test]
#[should_panic(
    expected: (
        "Caller `dojo-foo_setter` has no write access on model (or it's namespace) `dojo-Foo`",
        'ENTRYPOINT_FAILED',
        'ENTRYPOINT_FAILED'
    )
)]
fn test_not_writer_with_known_contract() {
    let world = deploy_world();
    world.register_model(foo::TEST_CLASS_HASH.try_into().unwrap());

    let account = starknet::contract_address_const::<0xb0b>();
    world.grant_owner(bytearray_hash(@"dojo"), account);

    // the account owns the 'test_contract' namespace so it should be able to deploy
    // and register the model.
    starknet::testing::set_account_contract_address(account);
    starknet::testing::set_contract_address(account);

    let contract_address = world
        .deploy_contract('salt1', foo_setter::TEST_CLASS_HASH.try_into().unwrap());
    let d = IFooSetterDispatcher { contract_address };
    d.set_foo(1, 2);
}


// ---------------------------

#[starknet::contract]
pub mod attacker_contract {
    use dojo::world;
    use dojo::world::IWorldDispatcher;
    use dojo::world::IWorldDispatcherTrait;
    use dojo::world::IWorldProvider;
    use dojo::contract::IContract;
    use starknet::storage::{
        StorageMapReadAccess, StorageMapWriteAccess, StoragePointerReadAccess, StoragePointerWriteAccess
    };

    #[storage]
    struct Storage {
        world_dispatcher: IWorldDispatcher,
    }

    #[abi(embed_v0)]
    pub impl ContractImpl of IContract<ContractState> {
        fn contract_name(self: @ContractState) -> ByteArray {
            "test_1"
        }

        fn namespace(self: @ContractState) -> ByteArray {
            "ns1"
        }

        fn tag(self: @ContractState) -> ByteArray {
            "other tag"
        }

        fn name_hash(self: @ContractState) -> felt252 {
            'name hash'
        }

        fn namespace_hash(self: @ContractState) -> felt252 {
            dojo::utils::bytearray_hash(@"atk")
        }

        fn selector(self: @ContractState) -> felt252 {
            selector_from_tag!("dojo-Foo")
        }
    }

    #[abi(embed_v0)]
    impl WorldProviderImpl of IWorldProvider<ContractState> {
        fn world(self: @ContractState) -> IWorldDispatcher {
            self.world_dispatcher.read()
        }
    }
}

#[starknet::contract]
pub mod attacker_model {
    #[storage]
    struct Storage {}

    #[abi(embed_v0)]
    impl DojoModelImpl of dojo::model::IModel<ContractState> {
        fn name(self: @ContractState) -> ByteArray {
            "m1"
        }

        fn namespace(self: @ContractState) -> ByteArray {
            "ns1"
        }

        fn tag(self: @ContractState) -> ByteArray {
            "other tag"
        }

        fn version(self: @ContractState) -> u8 {
            1
        }

        fn selector(self: @ContractState) -> felt252 {
            selector_from_tag!("dojo-Foo")
        }

        fn name_hash(self: @ContractState) -> felt252 {
            'name hash'
        }

        fn namespace_hash(self: @ContractState) -> felt252 {
            // "atk" hash
            dojo::utils::bytearray_hash(@"atk")
        }

        fn unpacked_size(self: @ContractState) -> Option<usize> {
            Option::None
        }

        fn packed_size(self: @ContractState) -> Option<usize> {
            Option::None
        }

        fn layout(self: @ContractState) -> dojo::model::Layout {
            dojo::model::Layout::Fixed([].span())
        }

        fn schema(self: @ContractState) -> dojo::model::introspect::Ty {
            dojo::model::introspect::Ty::Primitive('felt252')
        }
    }
}

#[test]
fn test_attacker_control_hashes() {
    let owner = starknet::contract_address_const::<'owner'>();
    let attacker = starknet::contract_address_const::<'attacker'>();

    starknet::testing::set_account_contract_address(owner);
    starknet::testing::set_contract_address(owner);

    // Owner deploys the world and register Foo model.
    let world = deploy_world();
    world.register_model(foo::TEST_CLASS_HASH.try_into().unwrap());

    let foo_selector = Model::<Foo>::selector();

    assert(world.is_owner(foo_selector, owner), 'should be owner');

    starknet::testing::set_contract_address(attacker);
    starknet::testing::set_account_contract_address(attacker);

    // Attacker has control over the this namespace.
    world.register_namespace("atk");

    // Attacker can't take ownership of the Foo model.
    // world.register_model(attacker_model::TEST_CLASS_HASH.try_into().unwrap());

    // Attacker can however deploy a contract and takes ownership of the Foo model.
    let _contract_address = world.deploy_contract('salt1', attacker_contract::TEST_CLASS_HASH.try_into().unwrap());

    assert(world.is_owner(foo_selector, starknet::get_contract_address()), 'world should be owner');

    assert(!world.is_owner(foo_selector, attacker), 'attacker should not be owner');
}