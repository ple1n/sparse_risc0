use std::{collections::HashMap, env::current_dir, path::PathBuf};

use clap::Parser;
use freenet_ping_types::{Ping, PingContractOptions};
use freenet_stdlib::{
    client_api::{ClientRequest, ContractRequest},
    prelude::*,
};

mod ping_client;
use ping_client::{
    connect_to_host, run_ping_client, wait_for_get_response, wait_for_put_response,
    wait_for_subscribe_response,
};
use tracing::warn;

use rand::rngs::OsRng;
use sha2::{Digest, Sha256};

#[derive(clap::Parser)]
#[command(author, version, about)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Run the default ping client
    Default(DefaultArgs),
    Groth16,
}

#[derive(clap::Args)]
struct DefaultArgs {
    #[clap(long, default_value = "localhost:50509")]
    host: String,
    #[clap(long, default_value = "info")]
    log_level: tracing::level_filters::LevelFilter,
    #[clap(flatten)]
    parameters: PingContractOptions,
    #[clap(long)]
    put_contract: bool,
    #[clap(long)]
    node_id: String,
}

const PACKAGE_DIR: &str = env!("CARGO_MANIFEST_DIR");
const PATH_TO_CONTRACT: &str = "../contracts/ping/build/freenet/freenet_ping_contract";

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let args = Args::parse();

    // Determine log_level for tracing
    let log_level = match &args.command {
        Some(Command::Default(d)) => d.log_level,
        _ => tracing::level_filters::LevelFilter::INFO,
    };
    tracing_subscriber::fmt()
        .with_ansi(true)
        .with_level(true)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_max_level(log_level)
        .with_line_number(true)
        .init();

    match &args.command {
        Some(Command::Groth16) => {

            Ok(())
        }
        _ => {
            // Default command (ping client)
            let default_args = match args.command {
                Some(Command::Default(d)) => d,
                _ => panic!("Default command args missing"),
            };

            // Connect to host using our utility function
            let mut client = connect_to_host(&default_args.host).await?;

            let params = Parameters::from(serde_json::to_vec(&default_args.parameters).unwrap());
            let path_to_code = PathBuf::from(PACKAGE_DIR).join(PATH_TO_CONTRACT);
            tracing::info!(path=%path_to_code.display(), "loading contract code");
            let code = std::fs::read(path_to_code).ok();

            let container = code
                .map(|bytes| ContractContainer::try_from((bytes, &params)))
                .transpose()?;
            let contract_key =
                ContractKey::from_params(default_args.parameters.code_key.clone(), params.clone())?;

            // Step 1: Put the contract or get it, and wait for response
            let mut local_state: Ping;

            if default_args.put_contract {
                // Put contract and wait for response
                let ping = Ping::default();
                let serialized = serde_json::to_vec(&ping)?;
                client
                    .send(ClientRequest::ContractOp(ContractRequest::Put {
                        contract: container.ok_or("contract not found while putting")?,
                        state: WrappedState::new(serialized),
                        related_contracts: RelatedContracts::new(),
                        subscribe: false,
                    }))
                    .await?;

                // Wait for put response
                let key = wait_for_put_response(&mut client, &contract_key).await?;
                tracing::info!(key=%key, "put ping contract successfully!");
                local_state = Ping::default();
            } else {
                // Get contract and wait for response
                client
                    .send(ClientRequest::ContractOp(ContractRequest::Get {
                        key: contract_key,
                        return_contract_code: true,
                        subscribe: false,
                    }))
                    .await?;

                // Wait for get response
                local_state = wait_for_get_response(&mut client, &contract_key).await?;
            }

            // Step 2: Subscribe to the contract and wait for subscription confirmation
            tracing::info!("Subscribing to contract...");
            client
                .send(ClientRequest::ContractOp(ContractRequest::Subscribe {
                    key: contract_key,
                    summary: None,
                }))
                .await?;

            // Wait for subscription response
            wait_for_subscribe_response(&mut client, &contract_key).await?;
            tracing::info!(key=%contract_key, "subscribed successfully!");

            // Run the main ping client logic

            run_ping_client(
                &mut client,
                contract_key,
                default_args.parameters,
                default_args.node_id.clone(),
                &mut local_state,
                None,
                None,
            )
            .await?;

            Ok(())
        }
    }
}
