use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use smol_str::SmolStr;
use starknet::core::{serde::unsigned_field_element::UfeHex, types::contract::AbiEntry};
use starknet_crypto::FieldElement;

// Collection of different types of `Manifest`'s which are used by dojo compiler/sozo
// For example:
//   - `BaseManifest` is generated by the compiler and wrote to `manifests/base` folder of project
//   - `DeploymentManifest` is generated by sozo which represents the future onchain state after a
//     successful migration
//   - `OverlayManifest` is used by sozo to override values of specific manifest of `BaseManifest`
//     thats generated by compiler

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct BaseManifest {
    pub world: Manifest<Class>,
    pub base: Manifest<Class>,
    pub contracts: Vec<Manifest<DojoContract>>,
    pub models: Vec<Manifest<DojoModel>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct DeploymentManifest {
    pub world: Manifest<Contract>,
    pub base: Manifest<Class>,
    pub contracts: Vec<Manifest<DojoContract>>,
    pub models: Vec<Manifest<DojoModel>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct OverlayManifest {
    pub world: Option<OverlayClass>,
    pub base: Option<OverlayClass>,
    pub contracts: Vec<OverlayDojoContract>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Manifest<T>
where
    T: ManifestMethods,
{
    #[serde(flatten)]
    pub inner: T,
    pub name: SmolStr,
}

// Utility methods thats needs to be implemented by manifest types
pub trait ManifestMethods {
    type OverlayType;
    fn abi(&self) -> Option<&AbiFormat>;
    fn set_abi(&mut self, abi: Option<AbiFormat>);
    fn class_hash(&self) -> &FieldElement;
    fn set_class_hash(&mut self, class_hash: FieldElement);
    fn original_class_hash(&self) -> &FieldElement;

    /// This method is called when during compilation base manifest file already exists.
    /// Manifest generated during compilation won't contains properties manually updated by users
    /// (like calldata) so this method should override those fields
    fn merge(&mut self, old: Self::OverlayType);
}

impl<T> Manifest<T>
where
    T: ManifestMethods,
{
    pub fn new(inner: T, name: SmolStr) -> Self {
        Self { inner, name }
    }
}

#[serde_as]
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(tag = "kind")]
pub struct DojoContract {
    #[serde_as(as = "Option<UfeHex>")]
    pub address: Option<FieldElement>,
    #[serde_as(as = "UfeHex")]
    pub class_hash: FieldElement,
    #[serde_as(as = "UfeHex")]
    pub original_class_hash: FieldElement,
    // base class hash used to deploy the contract
    #[serde_as(as = "UfeHex")]
    pub base_class_hash: FieldElement,
    pub abi: Option<AbiFormat>,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
    pub computed: Vec<ComputedValueEntrypoint>,
}

/// Represents a declaration of a model.
#[serde_as]
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(tag = "kind")]
pub struct DojoModel {
    pub members: Vec<Member>,
    #[serde_as(as = "UfeHex")]
    pub class_hash: FieldElement,
    #[serde_as(as = "UfeHex")]
    pub original_class_hash: FieldElement,
    pub abi: Option<AbiFormat>,
}

#[serde_as]
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(tag = "kind")]
pub struct Contract {
    #[serde_as(as = "UfeHex")]
    pub class_hash: FieldElement,
    #[serde_as(as = "UfeHex")]
    pub original_class_hash: FieldElement,
    pub abi: Option<AbiFormat>,
    #[serde_as(as = "Option<UfeHex>")]
    pub address: Option<FieldElement>,
    #[serde_as(as = "Option<UfeHex>")]
    pub transaction_hash: Option<FieldElement>,
    pub block_number: Option<u64>,
    // used by World contract
    pub seed: Option<String>,
}

#[serde_as]
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(tag = "kind")]
pub struct Class {
    #[serde_as(as = "UfeHex")]
    pub class_hash: FieldElement,
    #[serde_as(as = "UfeHex")]
    pub original_class_hash: FieldElement,
    pub abi: Option<AbiFormat>,
}

#[serde_as]
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct OverlayDojoContract {
    pub name: SmolStr,
    pub original_class_hash: Option<FieldElement>,
    pub reads: Option<Vec<String>>,
    pub writes: Option<Vec<String>>,
}

#[serde_as]
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct OverlayDojoModel {
    pub name: SmolStr,
    pub original_class_hash: Option<FieldElement>,
}

#[serde_as]
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct OverlayContract {
    pub name: SmolStr,
    pub original_class_hash: Option<FieldElement>,
}

#[serde_as]
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct OverlayClass {
    pub name: SmolStr,
    pub original_class_hash: Option<FieldElement>,
}

// Types used by manifest

/// Represents a model member.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Member {
    /// Name of the member.
    pub name: String,
    /// Type of the member.
    #[serde(rename = "type")]
    pub ty: String,
    pub key: bool,
}

#[serde_as]
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct ComputedValueEntrypoint {
    // Name of the contract containing the entrypoint
    pub contract: SmolStr,
    // Name of entrypoint to get computed value
    pub entrypoint: SmolStr,
    // Component to compute for
    pub model: Option<String>,
}

impl From<dojo_types::schema::Member> for Member {
    fn from(m: dojo_types::schema::Member) -> Self {
        Self { name: m.name, ty: m.ty.name(), key: m.key }
    }
}

/// System input ABI.
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct Input {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
}

/// System Output ABI.
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct Output {
    #[serde(rename = "type")]
    pub ty: String,
}

/// Format of the ABI into the manifest.
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AbiFormat {
    /// Only a relative path to the ABI file is stored.
    Path(Utf8PathBuf),
    /// The full ABI is embedded.
    Embed(Vec<AbiEntry>),
}

#[cfg(test)]
impl PartialEq for AbiFormat {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (AbiFormat::Path(p1), AbiFormat::Path(p2)) => p1 == p2,
            (AbiFormat::Embed(e1), AbiFormat::Embed(e2)) => {
                // Currently, [`AbiEntry`] does not implement [`PartialEq`] so we cannot compare
                // them directly.
                let e1_json = serde_json::to_string(e1).expect("valid JSON from ABI");
                let e2_json = serde_json::to_string(e2).expect("valid JSON from ABI");
                e1_json == e2_json
            }
            _ => false,
        }
    }
}

impl AbiFormat {
    pub fn to_path(&self) -> Option<&Utf8PathBuf> {
        match self {
            AbiFormat::Path(p) => Some(p),
            AbiFormat::Embed(_) => None,
        }
    }
}
