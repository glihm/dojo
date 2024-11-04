//! A simple storage abstraction for the world's storage.

use core::panic_with_felt252;
use dojo::world::{IWorldDispatcher, IWorldDispatcherTrait, Resource};
use dojo::model::{Model, ModelIndex, ModelValueKey, ModelValue, ModelStorage, ModelPtr};
use dojo::event::{Event, EventStorage};
use dojo::meta::Layout;
use dojo::utils::{
    entity_id_from_key, entity_id_from_keys, serialize_inline, find_model_field_layout
};
use starknet::{ContractAddress, ClassHash};

#[derive(Drop, Copy)]
pub struct WorldStorage {
    pub dispatcher: IWorldDispatcher,
    pub namespace_hash: felt252,
}

#[generate_trait]
pub impl WorldStorageInternalImpl of WorldStorageTrait {
    fn new(world: IWorldDispatcher, namespace: @ByteArray) -> WorldStorage {
        let namespace_hash = dojo::utils::bytearray_hash(namespace);

        WorldStorage { dispatcher: world, namespace_hash }
    }

    fn set_namespace(ref self: WorldStorage, namespace: @ByteArray) {
        self.namespace_hash = dojo::utils::bytearray_hash(namespace);
    }

    fn dns(self: @WorldStorage, contract_name: @ByteArray) -> Option<(ContractAddress, ClassHash)> {
        match (*self.dispatcher)
            .resource(
                dojo::utils::selector_from_namespace_and_name(*self.namespace_hash, contract_name)
            ) {
            Resource::Contract((
                contract_address, class_hash
            )) => Option::Some((contract_address, class_hash.try_into().unwrap())),
            _ => Option::None
        }
    }

    fn contract_selector(self: @WorldStorage, contract_name: @ByteArray) -> felt252 {
        dojo::utils::selector_from_namespace_and_name(*self.namespace_hash, contract_name)
    }
}

pub impl EventStorageWorldStorageImpl<E, +Event<E>> of EventStorage<WorldStorage, E> {
    fn emit_event(ref self: WorldStorage, event: @E) {
        let events: Array<@E> = array![event];
        Self::emit_events(ref self, events.span());
    }

    fn emit_events(ref self: WorldStorage, events: Span<@E>) {
        let mut all_keys: Array<Span<felt252>> = array![];
        let mut all_values: Array<Span<felt252>> = array![];

        let mut i = 0;
        loop {
            if i >= events.len() {
                break;
            }

            all_keys.append(Event::<E>::keys(*events[i]));
            all_values.append(Event::<E>::values(*events[i]));

            i += 1;
        };

        dojo::world::IWorldDispatcherTrait::emit_events(
            self.dispatcher,
            Event::<E>::selector(self.namespace_hash),
            all_keys.span(),
            all_values.span(),
            Event::<E>::historical()
        );
    }
}

pub impl ModelStorageWorldStorageImpl<M, +Model<M>, +Drop<M>> of ModelStorage<WorldStorage, M> {
    fn read_model<K, +Drop<K>, +Serde<K>>(self: @WorldStorage, key: K) -> M {
        let mut keys = serialize_inline::<K>(@key);
        let values = IWorldDispatcherTrait::entities(
            *self.dispatcher,
            Model::<M>::selector(*self.namespace_hash),
            [ModelIndex::Keys(keys)].span(),
            Model::<M>::layout()
        );

        // Only one model is read, we can use the first span only.
        let mut values_first = *values[0];
        match Model::<M>::from_values(ref keys, ref values_first) {
            Option::Some(model) => model,
            Option::None => {
                panic!(
                    "Model: deserialization failed. Ensure the length of the keys tuple is matching the number of #[key] fields in the model struct."
                )
            }
        }
    }

    fn read_models<K, +Drop<K>, +Serde<K>>(self: @WorldStorage, keys: Span<K>) -> Span<M> {
        let mut all_keys: Array<Span<felt252>> = array![];

        let mut i = 0;
        loop {
            if i >= keys.len() {
                break;
            }

            all_keys.append(serialize_inline::<K>(*keys[i]));

            i += 1;
        };

        let all_values = IWorldDispatcherTrait::entities(
            *self.dispatcher,
            Model::<M>::selector(*self.namespace_hash),
            all_keys.span(),
            Model::<M>::layout()
        );

        let mut i = 0;
        let mut models: Array<M> = array![];
        loop {
            if i >= all_values.len() {
                break;
            }

            let mut m_values = *all_values[i];
            let mut m_keys = *all_keys[i];
            match Model::<M>::from_values(ref m_keys, ref m_values) {
                Option::Some(model) => models.append(model),
                Option::None => {
                    panic!(
                        "Model: deserialization failed. Ensure the length of the keys tuple is matching the number of #[key] fields in the model struct."
                    )
                }
            }

            i += 1;
        };

        models.span()
    }

    fn write_model(ref self: WorldStorage, model: @M) {
        let models: Array<@M> = array![model];
        Self::write_models(ref self, models.span());
    }

    fn write_models(ref self: WorldStorage, models: Span<@M>) {
        let mut keys: Array<ModelIndex> = array![];
        let mut values: Array<Span<felt252>> = array![];

        let mut i = 0;
        loop {
            if i >= models.len() {
                break;
            }

            keys.append(ModelIndex::Keys(Model::<M>::keys(*models[i])));
            values.append(Model::<M>::values(*models[i]));

            i += 1;
        };

        IWorldDispatcherTrait::set_entities(
            self.dispatcher,
            Model::<M>::selector(self.namespace_hash),
            keys.span(),
            values.span(),
            Model::<M>::layout()
        );
    }

    fn erase_model(ref self: WorldStorage, model: @M) {
        let models: Array<@M> = array![model];
        Self::erase_models(ref self, models.span());
    }

    fn erase_models(ref self: WorldStorage, models: Span<@M>) {
        let mut keys: Array<ModelIndex> = array![];

        let mut i = 0;
        loop {
            if i >= models.len() {
                break;
            }

            keys.append(ModelIndex::Keys(Model::<M>::keys(*models[i])));

            i += 1;
        };

        IWorldDispatcherTrait::delete_entities(
            self.dispatcher,
            Model::<M>::selector(self.namespace_hash),
            keys.span(),
            Model::<M>::layout()
        );
    }

    fn erase_model_ptr(ref self: WorldStorage, ptr: ModelPtr<M>) {
        let ptrs: Array<ModelPtr<M>> = array![ptr];
        Self::erase_models_ptrs(ref self, ptrs.span());
    }

    fn erase_models_ptrs(ref self: WorldStorage, ptrs: Span<ModelPtr<M>>) {
        let mut ids: Array<ModelIndex> = array![];

        let mut i = 0;
        loop {
            if i >= ptrs.len() {
                break;
            }

            ids
                .append(
                    match ptrs[i] {
                        ModelPtr::Id(id) => ModelIndex::Id(*id),
                        ModelPtr::Keys(keys) => ModelIndex::Id(entity_id_from_keys(*keys)),
                    }
                );

            i += 1;
        };

        IWorldDispatcherTrait::delete_entities(
            self.dispatcher,
            Model::<M>::selector(self.namespace_hash),
            ids.span(),
            Model::<M>::layout()
        );
    }

    fn namespace_hash(self: @WorldStorage) -> felt252 {
        *self.namespace_hash
    }
}

impl ModelValueStorageWorldStorageImpl<
    V, +ModelValue<V>
> of dojo::model::ModelValueStorage<WorldStorage, V> {
    fn read_value<K, +Drop<K>, +Serde<K>, +ModelValueKey<V, K>>(self: @WorldStorage, key: K) -> V {
        Self::read_value_from_id(self, entity_id_from_key(@key))
    }

    fn read_value_from_id(self: @WorldStorage, entity_id: felt252) -> V {
        let mut values = IWorldDispatcherTrait::entity(
            *self.dispatcher,
            ModelValue::<V>::selector(*self.namespace_hash),
            ModelIndex::Id(entity_id),
            ModelValue::<V>::layout()
        );
        match ModelValue::<V>::from_values(entity_id, ref values) {
            Option::Some(entity) => entity,
            Option::None => {
                panic!(
                    "Value: deserialization failed. Ensure the length of the keys tuple is matching the number of #[key] fields in the model struct."
                )
            }
        }
    }

    fn write_value<K, +Drop<K>, +Serde<K>, +ModelValueKey<V, K>>(
        ref self: WorldStorage, key: K, value: @V
    ) {
        let keys: Array<K> = array![key];
        let values: Array<@V> = array![value];
        Self::write_values(ref self, keys.span(), values.span());
    }

    fn write_values<K, +Drop<K>, +Serde<K>, +ModelValueKey<V, K>>(
        ref self: WorldStorage, keys: Span<K>, values: Span<@V>
    ) {
        let mut all_keys: Array<ModelIndex> = array![];
        let mut all_values: Array<Span<felt252>> = array![];

        let mut i = 0;
        loop {
            if i >= keys.len() {
                break;
            }

            all_keys.append(ModelIndex::Id(entity_id_from_keys(serialize_inline::<K>(keys[i]))));
            all_values.append(ModelValue::<V>::values(*values[i]));

            i += 1;
        };

        IWorldDispatcherTrait::set_entities(
            self.dispatcher,
            ModelValue::<V>::selector(self.namespace_hash),
            all_keys.span(),
            all_values.span(),
            ModelValue::<V>::layout()
        );
    }

    fn write_value_from_id(ref self: WorldStorage, entity_id: felt252, value: @V) {
        let entity_ids: Array<felt252> = array![entity_id];
        let values: Array<@V> = array![value];
        Self::write_values_from_ids(ref self, entity_ids.span(), values.span());
    }

    fn write_values_from_ids(ref self: WorldStorage, entity_ids: Span<felt252>, values: Span<@V>) {
        let mut all_keys: Array<ModelIndex> = array![];
        let mut all_values: Array<Span<felt252>> = array![];

        let mut i = 0;
        loop {
            if i >= entity_ids.len() {
                break;
            }

            all_keys.append(ModelIndex::Id(*entity_ids[i]));
            all_values.append(ModelValue::<V>::values(*values[i]));

            i += 1;
        };

        IWorldDispatcherTrait::set_entities(
            self.dispatcher,
            ModelValue::<V>::selector(self.namespace_hash),
            all_keys.span(),
            all_values.span(),
            ModelValue::<V>::layout()
        );
    }
}

#[cfg(target: "test")]
pub impl EventStorageTestWorldStorageImpl<
    E, +Event<E>
> of dojo::event::EventStorageTest<WorldStorage, E> {
    fn emit_event_test(ref self: WorldStorage, event: @E) {
        let events: Array<@E> = array![event];
        Self::emit_events_test(ref self, events.span());
    }

    fn emit_events_test(ref self: WorldStorage, events: Span<@E>) {
        let mut all_keys: Array<Span<felt252>> = array![];
        let mut all_values: Array<Span<felt252>> = array![];

        let mut i = 0;
        loop {
            if i >= events.len() {
                break;
            }

            all_keys.append(Event::<E>::keys(*events[i]));
            all_values.append(Event::<E>::values(*events[i]));

            i += 1;
        };

        let world_test = dojo::world::IWorldTestDispatcher {
            contract_address: self.dispatcher.contract_address
        };

        dojo::world::IWorldTestDispatcherTrait::emit_events_test(
            world_test,
            Event::<E>::selector(self.namespace_hash),
            all_keys.span(),
            all_values.span(),
            Event::<E>::historical()
        );
    }
}

/// Implementation of the `ModelStorageTest` trait for testing purposes, bypassing permission
/// checks.
#[cfg(target: "test")]
pub impl ModelStorageTestWorldStorageImpl<
    M, +Model<M>
> of dojo::model::ModelStorageTest<WorldStorage, M> {
    fn write_model_test(ref self: WorldStorage, model: @M) {
        let world_test = dojo::world::IWorldTestDispatcher {
            contract_address: self.dispatcher.contract_address
        };
        dojo::world::IWorldTestDispatcherTrait::set_entities_test(
            world_test,
            Model::<M>::selector(self.namespace_hash),
            [ModelIndex::Keys(Model::keys(model))].span(),
            [Model::<M>::values(model)].span(),
            Model::<M>::layout()
        );
    }

    fn write_models_test(ref self: WorldStorage, models: Span<@M>) {
        let mut keys: Array<ModelIndex> = array![];
        let mut values: Array<Span<felt252>> = array![];

        let mut i = 0;
        loop {
            if i >= models.len() {
                break;
            }

            keys.append(ModelIndex::Keys(Model::<M>::keys(*models[i])));
            values.append(Model::<M>::values(*models[i]));

            i += 1;
        };

        let world_test = dojo::world::IWorldTestDispatcher {
            contract_address: self.dispatcher.contract_address
        };

        dojo::world::IWorldTestDispatcherTrait::set_entities_test(
            world_test,
            Model::<M>::selector(self.namespace_hash),
            keys.span(),
            values.span(),
            Model::<M>::layout()
        );
    }

    fn erase_model_test(ref self: WorldStorage, model: @M) {
        let world_test = dojo::world::IWorldTestDispatcher {
            contract_address: self.dispatcher.contract_address
        };

        dojo::world::IWorldTestDispatcherTrait::delete_entities_test(
            world_test,
            Model::<M>::selector(self.namespace_hash),
            [ModelIndex::Keys(Model::keys(model))].span(),
            Model::<M>::layout()
        );
    }

    fn erase_models_test(ref self: WorldStorage, models: Span<@M>) {
        let mut keys: Array<ModelIndex> = array![];

        let mut i = 0;
        loop {
            if i >= models.len() {
                break;
            }

            keys.append(ModelIndex::Keys(Model::<M>::keys(*models[i])));

            i += 1;
        };

        let world_test = dojo::world::IWorldTestDispatcher {
            contract_address: self.dispatcher.contract_address
        };

        dojo::world::IWorldTestDispatcherTrait::delete_entities_test(
            world_test, Model::<M>::selector(self.namespace_hash), keys.span(), Model::<M>::layout()
        );
    }

    fn erase_model_ptr_test(ref self: WorldStorage, ptr: ModelPtr<M>) {
        let entity_id = match ptr {
            ModelPtr::Id(id) => id,
            ModelPtr::Keys(keys) => entity_id_from_keys(keys),
        };

        let world_test = dojo::world::IWorldTestDispatcher {
            contract_address: self.dispatcher.contract_address
        };

        dojo::world::IWorldTestDispatcherTrait::delete_entities_test(
            world_test,
            Model::<M>::selector(self.namespace_hash),
            [ModelIndex::Id(entity_id)].span(),
            Model::<M>::layout()
        );
    }

    fn erase_models_ptrs_test(ref self: WorldStorage, ptrs: Span<ModelPtr<M>>) {
        let mut ids: Array<ModelIndex> = array![];

        let mut i = 0;
        loop {
            if i >= ptrs.len() {
                break;
            }

            ids
                .append(
                    match ptrs[i] {
                        ModelPtr::Id(id) => ModelIndex::Id(*id),
                        ModelPtr::Keys(keys) => ModelIndex::Id(entity_id_from_keys(*keys)),
                    }
                );

            i += 1;
        };

        let world_test = dojo::world::IWorldTestDispatcher {
            contract_address: self.dispatcher.contract_address
        };

        dojo::world::IWorldTestDispatcherTrait::delete_entities_test(
            world_test, Model::<M>::selector(self.namespace_hash), ids.span(), Model::<M>::layout()
        );
    }
}

/// Implementation of the `ModelValueStorageTest` trait for testing purposes, bypassing permission
/// checks.
#[cfg(target: "test")]
pub impl ModelValueStorageTestWorldStorageImpl<
    V, +ModelValue<V>
> of dojo::model::ModelValueStorageTest<WorldStorage, V> {
    fn write_value_test<K, +Drop<K>, +Serde<K>, +ModelValueKey<V, K>>(
        ref self: WorldStorage, key: K, value: @V
    ) {
        let keys: Array<K> = array![key];
        let values: Array<@V> = array![value];
        Self::write_values_test(ref self, keys.span(), values.span());
    }

    fn write_values_test<K, +Drop<K>, +Serde<K>, +ModelValueKey<V, K>>(
        ref self: WorldStorage, keys: Span<K>, values: Span<@V>
    ) {
        let mut all_keys: Array<ModelIndex> = array![];
        let mut all_values: Array<Span<felt252>> = array![];

        let mut i = 0;
        loop {
            if i >= keys.len() {
                break;
            }

            all_keys.append(ModelIndex::Id(entity_id_from_keys(serialize_inline::<K>(keys[i]))));
            all_values.append(ModelValue::<V>::values(*values[i]));

            i += 1;
        };

        let world_test = dojo::world::IWorldTestDispatcher {
            contract_address: self.dispatcher.contract_address
        };

        dojo::world::IWorldTestDispatcherTrait::set_entities_test(
            world_test,
            ModelValue::<V>::selector(self.namespace_hash),
            all_keys.span(),
            all_values.span(),
            ModelValue::<V>::layout()
        );
    }

    fn write_value_from_id_test(ref self: WorldStorage, entity_id: felt252, value: @V) {
        let entity_ids: Array<felt252> = array![entity_id];
        let values: Array<@V> = array![value];
        Self::write_values_from_ids_test(ref self, entity_ids.span(), values.span());
    }

    fn write_values_from_ids_test(
        ref self: WorldStorage, entity_ids: Span<felt252>, values: Span<@V>
    ) {
        let mut all_keys: Array<ModelIndex> = array![];
        let mut all_values: Array<Span<felt252>> = array![];

        let mut i = 0;
        loop {
            if i >= entity_ids.len() {
                break;
            }

            all_keys.append(ModelIndex::Id(*entity_ids[i]));
            all_values.append(ModelValue::<V>::values(*values[i]));

            i += 1;
        };

        let world_test = dojo::world::IWorldTestDispatcher {
            contract_address: self.dispatcher.contract_address
        };

        dojo::world::IWorldTestDispatcherTrait::set_entities_test(
            world_test,
            ModelValue::<V>::selector(self.namespace_hash),
            all_keys.span(),
            all_values.span(),
            ModelValue::<V>::layout()
        );
    }
}

/// Updates a serialized member of a model.
fn update_serialized_member(
    world: IWorldDispatcher,
    model_id: felt252,
    layout: Layout,
    entity_id: felt252,
    member_id: felt252,
    values: Span<felt252>,
) {
    match find_model_field_layout(layout, member_id) {
        Option::Some(field_layout) => {
            IWorldDispatcherTrait::set_entities(
                world,
                model_id,
                [ModelIndex::MemberId((entity_id, member_id))].span(),
                [values].span(),
                field_layout,
            )
        },
        Option::None => panic_with_felt252('bad member id')
    }
}

/// Retrieves a serialized member of a model.
fn get_serialized_member(
    world: IWorldDispatcher,
    model_id: felt252,
    layout: Layout,
    entity_id: felt252,
    member_id: felt252,
) -> Span<felt252> {
    match find_model_field_layout(layout, member_id) {
        Option::Some(field_layout) => {
            IWorldDispatcherTrait::entity(
                world, model_id, ModelIndex::MemberId((entity_id, member_id)), field_layout
            )
        },
        Option::None => panic_with_felt252('bad member id')
    }
}
