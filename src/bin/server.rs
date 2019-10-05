mod rpc_server;
#[macro_use]
extern crate log;

pub enum Action {}

use tokio::sync::mpsc;

fn main() {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
    )
    .unwrap();
    let (sender, _recv) = mpsc::channel(32);
    tokio::run(rpc_server::RPCServer::server(sender, "0.0.0.0", 1456));
}
