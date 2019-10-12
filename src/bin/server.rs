mod rpc_server;
#[macro_use]
extern crate log;

pub enum Action {}

use tokio::sync::mpsc;

struct SshHandler;
struct ArduinoHandler {
    stream: tokio::net::TcpStream,
}
impl ArduinoHandler {
    fn send(&mut self, command: ArduinoCommand) {}
}

enum ArduinoCommand {
    Set { id: i8, state: bool },
    Toggle { id: i8 },
}

impl ArduinoCommand {
    fn repr(&self) -> String {
        match self {
            ArduinoCommand::Set { id, state: true } => format!("on{}\n", id),
            ArduinoCommand::Set { id, state: false } => format!("off{}\n", id),
            ArduinoCommand::Toggle { id } => format!("tog{}\n", id),
        }
    }
}

impl ArduinoHandler {}

enum Handler {
    Arduino(ArduinoHandler),
    SSH(SshHandler),
}

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
