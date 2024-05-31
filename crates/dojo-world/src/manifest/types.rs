use std::fs;

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use smol_str::SmolStr;
use starknet::core::serde::unsigned_field_element::UfeHex;
use starknet::core::types::contract::AbiEntry;
use starknet_crypto::FieldElement;

use crate::manifest::AbstractManifestError;

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
    pub world: Manifest<WorldContract>,
    pub base: Manifest<Class>,
    pub contracts: Vec<Manifest<DojoContract>>,
    pub models: Vec<Manifest<DojoModel>>,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct OverlayManifest {
    pub world: Option<OverlayClass>,
    pub base: Option<OverlayClass>,
    pub contracts: Vec<OverlayDojoContract>,
    pub models: Vec<OverlayDojoModel>,
}

#[derive(Clone, Serialize, Default, Deserialize, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Manifest<T>
where
    T: ManifestMethods,
{
    #[serde(flatten)]
    pub inner: T,

    // name of the manifest which is used as filename
    pub manifest_name: String,

    // artifact name which is used to be able to match manifests
    // with artifacts produced during the compilation.
    pub artifact_name: String,
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
    pub fn new(inner: T, manifest_name: String, artifact_name: String) -> Self {
        Self { inner, manifest_name, artifact_name }
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
    #[serde(default)]
    pub reads: Vec<String>,
    #[serde(default)]
    pub writes: Vec<String>,
    #[serde(default)]
    pub computed: Vec<ComputedValueEntrypoint>,
    #[serde(default)]
    pub init_calldata: Vec<String>,
    pub name: String,
    pub namespace: String,
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
    pub name: String,
    pub namespace: String,
}

#[serde_as]
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(tag = "kind")]
pub struct WorldContract {
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
    pub seed: String,
    pub metadata: Option<WorldMetadata>,
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
    pub namespace: String,
    pub original_class_hash: Option<FieldElement>,
    pub reads: Option<Vec<String>>,
    pub writes: Option<Vec<String>>,
    pub init_calldata: Option<Vec<String>>,
}

#[serde_as]
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct OverlayDojoModel {
    pub name: SmolStr,
    pub namespace: String,
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
    pub namespace: Option<String>,
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

impl AbiFormat {
    /// Get the [`Utf8PathBuf`] if the ABI is stored as a path.
    pub fn to_path(&self) -> Option<&Utf8PathBuf> {
        match self {
            AbiFormat::Path(p) => Some(p),
            AbiFormat::Embed(_) => None,
        }
    }

    /// Loads an ABI from the path or embedded entries.
    ///
    /// # Arguments
    ///
    /// * `root_dir` - The root directory of the ABI file.
    pub fn load_abi_string(&self, root_dir: &Utf8PathBuf) -> Result<String, AbstractManifestError> {
        match self {
            AbiFormat::Path(abi_path) => Ok(fs::read_to_string(root_dir.join(abi_path))?),
            AbiFormat::Embed(abi) => Ok(serde_json::to_string(&abi)?),
        }
    }

    /// Convert to embed variant.
    ///
    /// # Arguments
    ///
    /// * `root_dir` - The root directory for the abi file resolution.
    pub fn to_embed(&self, root_dir: &Utf8PathBuf) -> Result<AbiFormat, AbstractManifestError> {
        if let AbiFormat::Path(abi_path) = self {
            let mut abi_file = std::fs::File::open(root_dir.join(abi_path))?;
            Ok(serde_json::from_reader(&mut abi_file)?)
        } else {
            Ok(self.clone())
        }
    }
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

#[serde_as]
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct WorldMetadata {
    pub profile_name: String,
    pub rpc_url: String,
}
