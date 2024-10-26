//! Remote resources for the world, gathered from events emitted by the world at the given address.
//!
//! A remote resource must be reconstructible form the onchain world without any additional
//! information.
//!
//! Remote resources are coupled to the namespace used to register them. No resource can be
//! registered without a namespace (excepting namespaces themselves).

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use dojo_types::naming;
use starknet::core::types::Felt;

mod events_to_remote;
mod permissions;

use crate::{DojoSelector, Namespace, ResourceType};

/// A remote resource that can be fetched from the world.
#[derive(Debug, Clone)]
pub enum ResourceRemote {
    Namespace(NamespaceRemote),
    Contract(ContractRemote),
    Model(ModelRemote),
    Event(EventRemote),
    // TODO: add starknet contract remote. Sozo needs a way to keep track of the address of this
    // contract once deployed.
}

/// The remote world representation.
#[derive(Debug, Default, Clone)]
pub struct WorldRemote {
    /// The class hashes of the world.
    pub class_hashes: Vec<Felt>,
    /// The resources of the world, by dojo selector.
    pub resources: HashMap<DojoSelector, ResourceRemote>,
}

/// Common information about a world's resource.
#[derive(Debug, Clone)]
pub struct CommonResourceRemoteInfo {
    /// The class hashes of the resource during its lifecycle,
    /// always at least one if the resource has been registered.
    /// Then for each upgrade, a new class hash is appended to the vector.
    pub class_hashes: Vec<Felt>,
    /// The name of the contract.
    pub name: String,
    /// The namespace used to register the resource remotely.
    pub namespace: Namespace,
    /// The address of the contract.
    pub address: Felt,
    /// The contract addresses that have owner permission on the contract.
    pub owners: HashSet<Felt>,
    /// The contract addresses that have writer permission on the contract.
    pub writers: HashSet<Felt>,
}

#[derive(Debug, Clone)]
pub struct ContractRemote {
    /// Common information about the resource.
    pub common: CommonResourceRemoteInfo,
    /// Whether the contract has been initialized.
    pub is_initialized: bool,
}

#[derive(Debug, Clone)]
pub struct ModelRemote {
    /// Common information about the resource.
    pub common: CommonResourceRemoteInfo,
}

#[derive(Debug, Clone)]
pub struct EventRemote {
    /// Common information about the resource.
    pub common: CommonResourceRemoteInfo,
}

#[derive(Debug, Clone)]
pub struct NamespaceRemote {
    pub name: String,
    /// The contract addresses that have owner permission on the contract.
    pub owners: HashSet<Felt>,
    /// The contract addresses that have writer permission on the contract.
    pub writers: HashSet<Felt>,
}

impl NamespaceRemote {
    /// Create a new namespace remote.
    pub fn new(name: String) -> Self {
        Self { name, owners: HashSet::new(), writers: HashSet::new() }
    }
}

impl CommonResourceRemoteInfo {
    /// Create a new common resource remote info.
    pub fn new(original_class_hash: Felt, namespace: &str, name: &str, address: Felt) -> Self {
        Self {
            class_hashes: vec![original_class_hash],
            name: name.to_string(),
            namespace: namespace.to_string(),
            address,
            owners: HashSet::new(),
            writers: HashSet::new(),
        }
    }

    /// The dojo selector of the resource.
    pub fn dojo_selector(&self) -> DojoSelector {
        naming::compute_selector_from_names(&self.namespace, &self.name)
    }

    /// The class hash of the resource after its latest upgrade.
    pub fn current_class_hash(&self) -> Felt {
        *self.class_hashes.last().expect("Remote resources must have at least one class hash.")
    }

    /// The class hash of the resource when it was first registered.
    pub fn original_class_hash(&self) -> Felt {
        *self.class_hashes.first().expect("Remote resources must have at least one class hash.")
    }

    /// Push a new class hash to the resource meaning it has been upgraded.
    pub fn push_class_hash(&mut self, class_hash: Felt) {
        self.class_hashes.push(class_hash);
    }
}

impl ContractRemote {
    /// The dojo selector of the resource.
    pub fn dojo_selector(&self) -> DojoSelector {
        self.common.dojo_selector()
    }
}

impl ModelRemote {
    /// The dojo selector of the resource.
    pub fn dojo_selector(&self) -> DojoSelector {
        self.common.dojo_selector()
    }
}

impl EventRemote {
    /// The dojo selector of the resource.
    pub fn dojo_selector(&self) -> DojoSelector {
        self.common.dojo_selector()
    }
}

impl ResourceRemote {
    /// The dojo selector of the resource.
    pub fn dojo_selector(&self) -> DojoSelector {
        match self {
            // The namespace doesn't apply to have the dojo selector of a namespace resource.
            ResourceRemote::Namespace(ns) => naming::compute_bytearray_hash(&ns.name),
            ResourceRemote::Contract(contract) => contract.dojo_selector(),
            ResourceRemote::Model(model) => model.dojo_selector(),
            ResourceRemote::Event(event) => event.dojo_selector(),
        }
    }
    /// The name of the resource.
    pub fn name(&self) -> String {
        match self {
            ResourceRemote::Contract(c) => c.common.name.clone(),
            ResourceRemote::Model(m) => m.common.name.clone(),
            ResourceRemote::Event(e) => e.common.name.clone(),
            ResourceRemote::Namespace(ns) => ns.name.clone(),
        }
    }

    /// The namespace of the resource.
    pub fn namespace(&self) -> String {
        match self {
            ResourceRemote::Contract(c) => c.common.namespace.clone(),
            ResourceRemote::Model(m) => m.common.namespace.clone(),
            ResourceRemote::Event(e) => e.common.namespace.clone(),
            ResourceRemote::Namespace(ns) => ns.name.clone(),
        }
    }

    /// Returns the tag of the resource.
    pub fn tag(&self) -> String {
        naming::get_tag(&self.namespace(), &self.name())
    }

    /// The address of the resource.
    pub fn address(&self) -> Felt {
        match self {
            ResourceRemote::Contract(c) => c.common.address,
            ResourceRemote::Model(m) => m.common.address,
            ResourceRemote::Event(e) => e.common.address,
            ResourceRemote::Namespace(_) => Felt::ZERO,
        }
    }

    /// Push a new class hash to the resource meaning it has been upgraded.
    pub fn push_class_hash(&mut self, class_hash: Felt) {
        match self {
            ResourceRemote::Namespace(_) => {}
            ResourceRemote::Contract(contract) => contract.common.push_class_hash(class_hash),
            ResourceRemote::Model(model) => model.common.push_class_hash(class_hash),
            ResourceRemote::Event(event) => event.common.push_class_hash(class_hash),
        }
    }

    /// The class hash of the resource after its latest upgrade.
    pub fn current_class_hash(&self) -> Felt {
        match self {
            ResourceRemote::Contract(contract) => contract.common.current_class_hash(),
            ResourceRemote::Model(model) => model.common.current_class_hash(),
            ResourceRemote::Event(event) => event.common.current_class_hash(),
            ResourceRemote::Namespace(_) => Felt::ZERO,
        }
    }

    /// Get the writers of the resource and it's dojo selector.
    pub fn get_writers(&self) -> (DojoSelector, HashSet<Felt>) {
        match self {
            ResourceRemote::Contract(contract) => {
                (self.dojo_selector(), contract.common.writers.clone())
            }
            ResourceRemote::Model(model) => (self.dojo_selector(), model.common.writers.clone()),
            ResourceRemote::Event(event) => (self.dojo_selector(), event.common.writers.clone()),
            ResourceRemote::Namespace(ns) => (self.dojo_selector(), ns.writers.clone()),
        }
    }

    /// Get the owners of the resource and it's dojo selector.
    pub fn get_owners(&self) -> (DojoSelector, HashSet<Felt>) {
        match self {
            ResourceRemote::Contract(contract) => {
                (self.dojo_selector(), contract.common.owners.clone())
            }
            ResourceRemote::Model(model) => (self.dojo_selector(), model.common.owners.clone()),
            ResourceRemote::Event(event) => (self.dojo_selector(), event.common.owners.clone()),
            ResourceRemote::Namespace(ns) => (self.dojo_selector(), ns.owners.clone()),
        }
    }

    /// Returns the type of the resource.
    pub fn resource_type(&self) -> ResourceType {
        match self {
            ResourceRemote::Contract(_) => ResourceType::Contract,
            ResourceRemote::Model(_) => ResourceType::Model,
            ResourceRemote::Event(_) => ResourceType::Event,
            ResourceRemote::Namespace(_) => ResourceType::Namespace,
        }
    }

    /// Get the contract remote if the resource is a contract, otherwise return an error.
    pub fn as_contract_mut(&mut self) -> Result<&mut ContractRemote> {
        match self {
            ResourceRemote::Contract(contract) => Ok(contract),
            _ => anyhow::bail!("Resource is expected to be a contract: {:?}.", self),
        }
    }

    /// Get the contract remote if the resource is a contract, otherwise panic.
    pub fn as_contract_or_panic(&self) -> &ContractRemote {
        match self {
            ResourceRemote::Contract(contract) => contract,
            _ => panic!("Resource is expected to be a contract: {:?}.", self),
        }
    }

    /// Get the model remote if the resource is a model, otherwise panic.
    pub fn as_model_or_panic(&self) -> &ModelRemote {
        match self {
            ResourceRemote::Model(model) => model,
            _ => panic!("Resource is expected to be a model: {:?}.", self),
        }
    }

    /// Get the event remote if the resource is an event, otherwise panic.
    pub fn as_event_or_panic(&self) -> &EventRemote {
        match self {
            ResourceRemote::Event(event) => event,
            _ => panic!("Resource is expected to be an event: {:?}.", self),
        }
    }

    /// Get the namespace remote if the resource is a namespace, otherwise panic.
    pub fn as_namespace_or_panic(&self) -> &NamespaceRemote {
        match self {
            ResourceRemote::Namespace(namespace) => namespace,
            _ => panic!("Resource is expected to be a namespace: {:?}.", self),
        }
    }
}

impl WorldRemote {
    /// Adds a resource to the world.
    pub fn add_resource(&mut self, resource: ResourceRemote) {
        self.resources.insert(resource.dojo_selector(), resource);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_contract_resource() {
        let mut world_remote = WorldRemote::default();
        let namespace = "ns".to_string();

        let contract = ContractRemote {
            common: CommonResourceRemoteInfo::new(Felt::ONE, &namespace, "c", Felt::ONE),
            is_initialized: false,
        };
        let resource = ResourceRemote::Contract(contract);

        world_remote.add_resource(resource);

        let selector = naming::compute_selector_from_names("ns", "c");
        assert!(world_remote.contracts.get(&namespace).unwrap().contains(&selector));
        assert!(world_remote.resources.contains_key(&selector));
    }

    #[test]
    fn test_add_model_resource() {
        let mut world_remote = WorldRemote::default();
        let namespace = "ns".to_string();

        let model = ModelRemote {
            common: CommonResourceRemoteInfo::new(Felt::ONE, &namespace, "m", Felt::ONE),
        };
        let resource = ResourceRemote::Model(model);

        world_remote.add_resource(resource);

        let selector = naming::compute_selector_from_names("ns", "m");
        assert!(world_remote.models.get(&namespace).unwrap().contains(&selector));
        assert!(world_remote.resources.contains_key(&selector));
    }

    #[test]
    fn test_add_event_resource() {
        let mut world_remote = WorldRemote::default();
        let namespace = "ns".to_string();

        let event = EventRemote {
            common: CommonResourceRemoteInfo::new(Felt::ONE, &namespace, "e", Felt::ONE),
        };
        let resource = ResourceRemote::Event(event);

        world_remote.add_resource(resource);

        let selector = naming::compute_selector_from_names("ns", "e");
        assert!(world_remote.events.get(&namespace).unwrap().contains(&selector));
        assert!(world_remote.resources.contains_key(&selector));
    }

    #[test]
    fn test_add_namespace_resource() {
        let mut world_remote = WorldRemote::default();
        let namespace = NamespaceRemote::new("ns".to_string());
        let resource = ResourceRemote::Namespace(namespace);

        world_remote.add_resource(resource);

        let selector = naming::compute_bytearray_hash("ns");
        assert!(world_remote.namespaces.contains(&selector));
        assert!(world_remote.resources.contains_key(&selector));
    }
}