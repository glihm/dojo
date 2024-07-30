#![cfg_attr(not(test), warn(unused_crate_dependencies))]

use std::io;
use std::net::SocketAddr;

use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};
use console::Style;
use dojo_metrics::{metrics_process, prometheus_exporter};
use katana_primitives::class::ClassHash;
use katana_primitives::contract::ContractAddress;
use katana_primitives::genesis::allocation::GenesisAccountAlloc;
use katana_primitives::genesis::Genesis;
use tokio::signal::ctrl_c;
use tracing::info;

mod args;
mod utils;

use args::Commands::Completions;
use args::KatanaArgs;

pub(crate) const LOG_TARGET: &str = "katana::cli";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = KatanaArgs::parse();
    args.init_logging()?;

    if let Some(command) = args.command {
        match command {
            Completions { shell } => {
                print_completion(shell);
                return Ok(());
            }
        }
    }

    let server_config = args.server_config();
    let sequencer_config = args.sequencer_config();
    let starknet_config = args.starknet_config()?;

    // TODO: move to katana-node
    if let Some(listen_addr) = args.metrics {
        let prometheus_handle = prometheus_exporter::install_recorder("katana")?;

        info!(target: LOG_TARGET, addr = %listen_addr, "Starting metrics endpoint.");
        prometheus_exporter::serve(
            listen_addr,
            prometheus_handle,
            metrics_process::Collector::default(),
        )
        .await?;
    }

    // build the node and start it
    let (rpc_handle, backend) =
        katana_node::start(server_config, sequencer_config, starknet_config).await?;

    if !args.silent {
        #[allow(deprecated)]
        let genesis = &backend.config.genesis;
        print_intro(&args, genesis, rpc_handle.addr);
    }

    // Wait until Ctrl + C is pressed, then shutdown
    ctrl_c().await?;
    rpc_handle.handle.stop()?;

    Ok(())
}

fn print_completion(shell: Shell) {
    let mut command = KatanaArgs::command();
    let name = command.get_name().to_string();
    generate(shell, &mut command, name, &mut io::stdout());
}

fn print_intro(args: &KatanaArgs, genesis: &Genesis, address: SocketAddr) {
    let mut accounts = genesis.accounts().peekable();
    let account_class_hash = accounts.peek().map(|e| e.1.class_hash());
    let seed = &args.starknet.seed;

    if args.json_log {
        info!(
            target: LOG_TARGET,
            "{}",
            serde_json::json!({
                "accounts": accounts.map(|a| serde_json::json!(a)).collect::<Vec<_>>(),
                "seed": format!("{}", seed),
                "address": format!("{address}"),
            })
        )
    } else {
        println!(
            "{}",
            Style::new().red().apply_to(
                r"


██╗  ██╗ █████╗ ████████╗ █████╗ ███╗   ██╗ █████╗
██║ ██╔╝██╔══██╗╚══██╔══╝██╔══██╗████╗  ██║██╔══██╗
█████╔╝ ███████║   ██║   ███████║██╔██╗ ██║███████║
██╔═██╗ ██╔══██║   ██║   ██╔══██║██║╚██╗██║██╔══██║
██║  ██╗██║  ██║   ██║   ██║  ██║██║ ╚████║██║  ██║
╚═╝  ╚═╝╚═╝  ╚═╝   ╚═╝   ╚═╝  ╚═╝╚═╝  ╚═══╝╚═╝  ╚═╝
"
            )
        );

        print_genesis_contracts(genesis, account_class_hash);
        print_genesis_accounts(accounts);

        println!(
            r"

ACCOUNTS SEED
=============
{seed}
    "
        );

        let addr = format!(
            "🚀 JSON-RPC server started: {}",
            Style::new().red().apply_to(format!("http://{address}"))
        );

        println!("\n{addr}\n\n",);
    }
}

fn print_genesis_contracts(genesis: &Genesis, account_class_hash: Option<ClassHash>) {
    println!(
        r"
PREDEPLOYED CONTRACTS
==================

| Contract        | Fee Token
| Address         | {}
| Class Hash      | {:#064x}",
        genesis.fee_token.address, genesis.fee_token.class_hash,
    );

    if let Some(ref udc) = genesis.universal_deployer {
        println!(
            r"
| Contract        | Universal Deployer
| Address         | {}
| Class Hash      | {:#064x}",
            udc.address, udc.class_hash
        )
    }

    if let Some(hash) = account_class_hash {
        println!(
            r"
| Contract        | Account Contract
| Class Hash      | {hash:#064x}"
        )
    }
}

fn print_genesis_accounts<'a, Accounts>(accounts: Accounts)
where
    Accounts: Iterator<Item = (&'a ContractAddress, &'a GenesisAccountAlloc)>,
{
    println!(
        r"

PREFUNDED ACCOUNTS
=================="
    );

    for (addr, account) in accounts {
        if let Some(pk) = account.private_key() {
            println!(
                r"
| Account address |  {addr}
| Private key     |  {pk:#x}
| Public key      |  {:#x}",
                account.public_key()
            )
        } else {
            println!(
                r"
| Account address |  {addr}
| Public key      |  {:#x}",
                account.public_key()
            )
        }
    }
}
