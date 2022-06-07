use super::*;

mod simulation;
use simulation::*;

use crate::blockchain::BlockchainError;
use crate::config::blockchain;
use crate::core::{ContractId, TransactionAndDelta};
use crate::zk;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;

fn init() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[tokio::test]
async fn test_peers_find_each_other() -> Result<(), NodeError> {
    init();

    let rules = Arc::new(RwLock::new(Vec::new()));
    let conf = blockchain::get_blockchain_config();

    let (node_futs, route_futs, chans) = simulation::test_network(
        Arc::clone(&rules),
        vec![
            NodeOpts {
                config: conf.clone(),
                wallet: None,
                addr: 3030,
                bootstrap: vec![],
                timestamp_offset: 5,
            },
            NodeOpts {
                config: conf.clone(),
                wallet: None,
                addr: 3031,
                bootstrap: vec![3030],
                timestamp_offset: 10,
            },
            NodeOpts {
                config: conf.clone(),
                wallet: None,
                addr: 3032,
                bootstrap: vec![3031],
                timestamp_offset: 15,
            },
        ],
    );
    let test_logic = async {
        sleep(Duration::from_millis(1000)).await;

        for chan in chans.iter() {
            assert_eq!(chan.peers().await?.peers.len(), 2);
        }

        for chan in chans.iter() {
            chan.shutdown().await?;
        }
        Ok::<(), NodeError>(())
    };
    tokio::try_join!(node_futs, route_futs, test_logic)?;
    Ok(())
}

#[tokio::test]
async fn test_timestamps_are_sync() -> Result<(), NodeError> {
    init();

    let rules = Arc::new(RwLock::new(Vec::new()));
    let conf = blockchain::get_blockchain_config();

    let (node_futs, route_futs, chans) = simulation::test_network(
        Arc::clone(&rules),
        vec![
            NodeOpts {
                config: conf.clone(),
                wallet: None,
                addr: 3030,
                bootstrap: vec![],
                timestamp_offset: 5,
            },
            NodeOpts {
                config: conf.clone(),
                wallet: None,
                addr: 3031,
                bootstrap: vec![3030],
                timestamp_offset: 10,
            },
            NodeOpts {
                config: conf.clone(),
                wallet: None,
                addr: 3032,
                bootstrap: vec![3031],
                timestamp_offset: 15,
            },
        ],
    );
    let test_logic = async {
        sleep(Duration::from_millis(1000)).await;

        let mut timestamps = Vec::new();
        for chan in chans.iter() {
            timestamps.push(chan.stats().await?.timestamp);
        }
        let first = timestamps.first().unwrap();
        assert!(timestamps.iter().all(|t| t == first));

        for chan in chans.iter() {
            chan.shutdown().await?;
        }
        Ok::<(), NodeError>(())
    };
    tokio::try_join!(node_futs, route_futs, test_logic)?;
    Ok(())
}

#[tokio::test]
async fn test_blocks_get_synced() -> Result<(), NodeError> {
    init();

    // Allow sync of clocks but no block transfer
    let rules = Arc::new(RwLock::new(vec![]));

    let conf = blockchain::get_test_blockchain_config();

    let (node_futs, route_futs, chans) = simulation::test_network(
        Arc::clone(&rules),
        vec![
            NodeOpts {
                config: conf.clone(),
                wallet: Some(Wallet::new(Vec::from("ABC"))),
                addr: 3030,
                bootstrap: vec![],
                timestamp_offset: 5,
            },
            NodeOpts {
                config: conf.clone(),
                wallet: Some(Wallet::new(Vec::from("CBA"))),
                addr: 3031,
                bootstrap: vec![3030],
                timestamp_offset: 10,
            },
        ],
    );
    let test_logic = async {
        // Wait till clocks sync
        sleep(Duration::from_millis(1000)).await;

        *rules.write().await = vec![Rule::drop_all()];

        chans[0].mine().await?;
        assert_eq!(chans[0].stats().await?.height, 2);
        chans[0].mine().await?;
        assert_eq!(chans[0].stats().await?.height, 3);
        chans[0].mine().await?;
        assert_eq!(chans[0].stats().await?.height, 4);

        chans[1].mine().await?;
        assert_eq!(chans[1].stats().await?.height, 2);
        chans[1].mine().await?;
        assert_eq!(chans[1].stats().await?.height, 3);
        chans[1].mine().await?;
        assert_eq!(chans[1].stats().await?.height, 4);
        chans[1].mine().await?;
        assert_eq!(chans[1].stats().await?.height, 5);
        chans[1].mine().await?;
        assert_eq!(chans[1].stats().await?.height, 6);

        // Still not synced...
        sleep(Duration::from_millis(2000)).await;
        assert_eq!(chans[0].stats().await?.height, 4);
        assert_eq!(chans[1].stats().await?.height, 6);

        // Now we open the connections...
        rules.write().await.clear();
        sleep(Duration::from_millis(3000)).await;
        assert_eq!(chans[0].stats().await?.height, 6);
        assert_eq!(chans[1].stats().await?.height, 6);

        // Now nodes should immediately sync with post_block
        chans[1].mine().await?;
        assert_eq!(chans[0].stats().await?.height, 7);
        assert_eq!(chans[1].stats().await?.height, 7);

        for chan in chans.iter() {
            chan.shutdown().await?;
        }

        Ok::<(), NodeError>(())
    };
    tokio::try_join!(node_futs, route_futs, test_logic)?;
    Ok(())
}

fn sample_contract_call() -> TransactionAndDelta {
    let updater = Wallet::new(Vec::from("ABC"));

    let cid =
        ContractId::from_str("ee439600bcd11a41d068c6bc7f5d55aa1cc6a73174b2594ee1e38c54abdf2a31")
            .unwrap();
    let state_model = zk::ZkStateModel::new(1, 10);
    let mut full_state = zk::ZkState::new(
        1,
        state_model,
        [(100, zk::ZkScalar::from(200))].into_iter().collect(),
    );
    let state_delta = zk::ZkStateDelta::new([(123, zk::ZkScalar::from(234))].into_iter().collect());
    full_state.apply_delta(&state_delta);
    updater.call_function(
        cid,
        0,
        state_delta.clone(),
        full_state.compress(),
        zk::ZkProof::Dummy(true),
        0,
        1,
    )
}

#[tokio::test]
async fn test_states_get_synced() -> Result<(), NodeError> {
    init();

    let rules = Arc::new(RwLock::new(vec![Rule::drop_all()]));
    let conf = blockchain::get_test_blockchain_config();

    let (node_futs, route_futs, chans) = simulation::test_network(
        Arc::clone(&rules),
        vec![
            NodeOpts {
                config: conf.clone(),
                wallet: Some(Wallet::new(Vec::from("ABC"))),
                addr: 3030,
                bootstrap: vec![],
                timestamp_offset: 5,
            },
            NodeOpts {
                config: conf.clone(),
                wallet: Some(Wallet::new(Vec::from("CBA"))),
                addr: 3031,
                bootstrap: vec![3030],
                timestamp_offset: 10,
            },
        ],
    );
    let test_logic = async {
        let tx_delta = sample_contract_call();

        chans[0].transact(tx_delta).await?;

        chans[0].mine().await?;
        assert_eq!(chans[0].stats().await?.height, 2);

        assert_eq!(chans[0].outdated_states().await?.outdated_states.len(), 0);

        // Still not synced...
        sleep(Duration::from_millis(1000)).await;
        assert_eq!(chans[0].stats().await?.height, 2);
        assert_eq!(chans[1].stats().await?.height, 1);

        // Now we open the connections but prevent transmission of states...
        *rules.write().await = vec![Rule::drop_url("state")];
        sleep(Duration::from_millis(1000)).await;
        assert_eq!(chans[0].stats().await?.height, 2);
        assert_eq!(chans[1].stats().await?.height, 2);

        assert_eq!(chans[0].outdated_states().await?.outdated_states.len(), 0);
        assert_eq!(chans[1].outdated_states().await?.outdated_states.len(), 1);

        // Now we open transmission of everything
        rules.write().await.clear();
        sleep(Duration::from_millis(1000)).await;
        assert_eq!(chans[0].outdated_states().await?.outdated_states.len(), 0);
        assert_eq!(chans[1].outdated_states().await?.outdated_states.len(), 0);

        for chan in chans.iter() {
            chan.shutdown().await?;
        }

        Ok::<(), NodeError>(())
    };
    tokio::try_join!(node_futs, route_futs, test_logic)?;
    Ok(())
}

#[tokio::test]
async fn test_chain_rolls_back() -> Result<(), NodeError> {
    init();

    let rules = Arc::new(RwLock::new(vec![Rule::drop_all()]));
    let conf = blockchain::get_test_blockchain_config();

    let (node_futs, route_futs, chans) = simulation::test_network(
        Arc::clone(&rules),
        vec![
            NodeOpts {
                config: conf.clone(),
                wallet: Some(Wallet::new(Vec::from("ABC"))),
                addr: 3030,
                bootstrap: vec![],
                timestamp_offset: 5,
            },
            NodeOpts {
                config: conf.clone(),
                wallet: Some(Wallet::new(Vec::from("CBA"))),
                addr: 3031,
                bootstrap: vec![3030],
                timestamp_offset: 10,
            },
        ],
    );
    let test_logic = async {
        let tx_delta = sample_contract_call();

        chans[0].transact(tx_delta).await?;

        chans[0].mine().await?;
        sleep(Duration::from_millis(1000)).await;
        assert_eq!(chans[0].stats().await?.height, 2);

        *rules.write().await = vec![Rule::drop_url("state")];
        sleep(Duration::from_millis(1000)).await;
        assert_eq!(chans[1].stats().await?.height, 2);
        assert_eq!(chans[0].outdated_states().await?.outdated_states.len(), 0);
        assert_eq!(chans[1].outdated_states().await?.outdated_states.len(), 1);

        assert!(matches!(
            chans[1].mine().await,
            Err(NodeError::BlockchainError(BlockchainError::StatesOutdated))
        ));

        sleep(Duration::from_millis(3000)).await;

        assert!(matches!(
            chans[1].mine().await,
            Err(NodeError::BlockchainError(BlockchainError::StatesOutdated))
        ));

        sleep(Duration::from_millis(4000)).await;

        assert_eq!(chans[1].outdated_states().await?.outdated_states.len(), 0);

        for chan in chans.iter() {
            chan.shutdown().await?;
        }

        Ok::<(), NodeError>(())
    };
    tokio::try_join!(node_futs, route_futs, test_logic)?;
    Ok(())
}
