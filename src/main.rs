#[macro_use]
extern crate lazy_static;

#[cfg(feature = "node")]
use {
    bazuka::blockchain::KvStoreChain,
    bazuka::db::LevelDbKvStore,
    bazuka::node::{Node, NodeError, PeerAddress},
    bazuka::wallet::Wallet,
    std::path::{Path, PathBuf},
    structopt::StructOpt,
};

#[cfg(not(feature = "node"))]
use {
    bazuka::blockchain::Blockchain, bazuka::blockchain::KvStoreChain, bazuka::core::Address,
    bazuka::db::RamKvStore, bazuka::wallet::Wallet,
};

#[cfg(feature = "node")]
#[derive(Debug, Clone, StructOpt)]
#[structopt(name = "Options", about = "Bazuka node software options")]
struct NodeOptions {
    #[structopt(long)]
    host: Option<String>,
    #[structopt(long)]
    port: Option<u16>,
    #[structopt(long, parse(from_os_str))]
    db: Option<PathBuf>,
}

lazy_static! {
    static ref WALLET: Wallet = Wallet::new(b"random seed".to_vec());
}

#[cfg(feature = "node")]
lazy_static! {
    static ref OPTS: NodeOptions = NodeOptions::from_args();
    static ref NODE: Node<KvStoreChain<LevelDbKvStore>> = {
        let opts = OPTS.clone();
        Node::new(
            PeerAddress(
                opts.host
                    .unwrap_or("127.0.0.1".to_string())
                    .parse()
                    .unwrap(),
                opts.port.unwrap_or(3030),
            ),
            bazuka::config::bootstrap::debug_bootstrap_nodes(),
            KvStoreChain::new(LevelDbKvStore::new(
                &opts
                    .db
                    .unwrap_or(home::home_dir().unwrap().join(Path::new(".bazuka"))),
            ))
            .unwrap(),
            Some(WALLET.clone()),
        )
    };
}

#[cfg(feature = "node")]
#[tokio::main]
async fn main() -> Result<(), NodeError> {
    println!(
        "Public Ip: {:?}",
        bazuka::node::upnp::get_public_ip().await.ok()
    );

    NODE.run().await?;
    Ok(())
}

#[cfg(not(feature = "node"))]
fn main() {
    let chain = KvStoreChain::new(RamKvStore::new()).unwrap();

    println!("Bazuka!");
    println!("Your address is: {}", WALLET.get_address());
    println!(
        "Balance: {:?}",
        chain.get_account(WALLET.get_address()).unwrap()
    );

    let tx = WALLET.create_transaction(Address::Treasury, 123, 0);
    println!("Verify tx signature: {}", tx.verify_signature());
}
