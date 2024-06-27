use std::env;
use std::path::PathBuf;

use anyhow::{bail, Context, Ok, Result};
use camino::Utf8PathBuf;
use dojo_lang::compiler::DojoCompiler;
use dojo_lang::plugin::CairoPluginRepository;
use dojo_world::manifest::{DeploymentManifest, MANIFESTS_DIR};
use dojo_world::migration::TxnConfig;
use katana_runner::KatanaRunner;
use scarb::compiler::{CompilerRepository, Profile};
use scarb::core::Config;
use starknet::core::types::FieldElement;

use crate::{CONTRACT_MANIFEST, CONTRACT_MANIFEST_RELATIVE_TO_TESTS};

pub async fn deploy(runner: &KatanaRunner) -> Result<FieldElement> {
    println!("Deploying contract {:?}", runner.log_file_path());
    if let Some(contract) = runner.contract().await {
        return Ok(contract);
    }

    let contract = if PathBuf::from(CONTRACT_MANIFEST).exists() {
        CONTRACT_MANIFEST
    } else {
        if !PathBuf::from(CONTRACT_MANIFEST_RELATIVE_TO_TESTS).exists() {
            bail!("manifest not found")
        }
        // calls in the `tests` dir use paths relative to itselfs
        CONTRACT_MANIFEST_RELATIVE_TO_TESTS
    };

    let address = deploy_contract(runner, contract).await?;
    runner.set_contract(address).await;
    Ok(address)
}

async fn deploy_contract(runner: &KatanaRunner, manifest: &str) -> Result<FieldElement> {
    let contract_address = prepare_migration_args(runner, manifest).await?;

    Ok(contract_address)
}

async fn prepare_migration_args(
    runner: &KatanaRunner,
    manifest_path: &str,
) -> Result<FieldElement> {
    // Preparing config, as in https://github.com/dojoengine/dojo/blob/25fbb7fc973cff4ce1273625c4664545d9b088e9/bin/sozo/src/main.rs#L28-L29
    let mut compilers = CompilerRepository::std();
    let cairo_plugins = CairoPluginRepository::default();
    compilers.add(Box::new(DojoCompiler)).unwrap();

    let manifest_path = Utf8PathBuf::from(manifest_path);
    let manifest_path = scarb::ops::find_manifest_path(Some(&manifest_path))?;

    let config = Config::builder(manifest_path.clone())
        .log_filter_directive(env::var_os("SCARB_LOG"))
        .profile(Profile::DEV)
        .offline(false)
        .cairo_plugins(cairo_plugins.into())
        .compilers(compilers)
        .build()
        .context("failed to build config")?;

    let ws = scarb::ops::read_workspace(config.manifest_path(), &config)?;

    sozo_ops::migration::migrate(
        &ws,
        None,
        runner.endpoint(),
        runner.account(0),
        ws.current_package().expect("Root package to be present").id.name.as_str(),
        false,
        TxnConfig::init_wait(),
        None,
    )
    .await?;

    let manifest_dir = manifest_path.parent().unwrap();

    let manifest = DeploymentManifest::load_from_path(
        &manifest_dir.join(MANIFESTS_DIR).join("dev").join("manifest").with_extension("toml"),
    )
    .expect("failed to load manifest");

    Ok(manifest.contracts[0].inner.address.unwrap())
}
