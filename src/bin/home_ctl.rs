use structopt::StructOpt;

mod objects;
mod commands;

pub mod home_manager {
    tonic::include_proto!("home_manager");
}

#[derive(StructOpt)]
#[structopt(name = "home-ctl", about = "A CLI to do some things from your home")]
struct Config {
    #[structopt(
        about = "The address of the home server",
        default_value = "http://localhost:14563"
    )]
    address: http::Uri,
    #[structopt(subcommand)]
    action: Action,
}

enum Status {
    On,
    Off,
}

impl std::str::FromStr for Status {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "on" => Ok(Self::On),
            "off" => Ok(Self::Off),
            u => Err(format!("Unknown status: {}", u)),
        }
    }
}

#[derive(StructOpt)]
enum Action {
    #[structopt(about = "register a new device")]
    RegisterDevice {
        #[structopt(help = "the device name", long, short)]
        name: String,
        #[structopt(help = "the category of the device", long, short)]
        kind: objects::ObjectKind, 
        #[structopt(help = "the actionner driving this object", long, short)]
        actionner_id: u32,
        #[structopt(help = "the way to identify the object in the actionner", long, short)]
        id_in_actionner: String,
    },
    #[structopt(about = "list objects, optionaly limit to a category")]
    ListDevice {
        #[structopt(help = "the optional category to search")]
        category: Option<objects::ObjectKind>,
    },
    #[structopt(about = "adds a new actionner")]
    RegisterActionner {
        #[structopt(help = "the remote location of the object (protocol dependent)", long, short)]
        remote: String,
        #[structopt(help = "the protocol used to communicate with the actionner", long, short)]
        protocol: objects::Protocol,
        #[structopt(help = "the actionner name", long, short)]
        name: String,
    },
    #[structopt(about = "lists all actionners")]
    ListActionners,
    #[structopt(about = "issue an arduino command")]
    Arduino {
        #[structopt(help = "the id of the device")]
        id: u32,
        #[structopt(subcommand)]
        command: ArduinoCommand,
    }
}

#[derive(StructOpt)]
pub enum ArduinoCommand {
    On,
    Off,
    Toggle,
}

use home_manager::{client::HomeManagerClient, ListDeviceRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Config::from_args();
    let mut client = HomeManagerClient::connect(args.address)?;
    match args.action {
        Action::Arduino{id: object_id, command} => {
            let command = bincode::serialize(&match command {
                ArduinoCommand::On => commands::ArduinoCommand::Set{state: true},
                ArduinoCommand::Off => commands::ArduinoCommand::Set{state: false},
                ArduinoCommand::Toggle => commands::ArduinoCommand::Toggle,
            })?;
            let request = tonic::Request::new(
                home_manager::CommandRequest {
                    command,
                    object_id,
                }
            );
            let respsonse = client.command(request).await?;
            println!("RESPONSE={:?}", respsonse);
        }
        Action::RegisterDevice{name, actionner_id, id_in_actionner, kind} => {
            let request = tonic::Request::new(
                home_manager::RegisterDeviceRequest {
                    kind: kind.name(),
                    actionner_id,
                    id_in_actionner,
                    name,
                });
            let respsonse = client.register_device(request).await?.into_inner();
            println!("RESPONSE={:?}", respsonse);
        }
        Action::ListDevice { category } => {
            let request = tonic::Request::new(ListDeviceRequest {
                kind_id: category.map(|kind| kind.id()).unwrap_or(0),
            });
            let response = client.list_device(request).await?.into_inner();
            println!("RESPONSE={:?}", response);
        }
        Action::RegisterActionner {
            remote,
            protocol,
            name,
        } => {
            let request = tonic::Request::new(home_manager::RegisterActionnerRequest {
                remote,
                protocol: protocol.name(),
                name,
            });
            let respsonse = client.register_actionner(request).await?.into_inner();
            println!("RESPONSE={:?}", respsonse);
        }
        Action::ListActionners => {
            let request = tonic::Request::new(home_manager::ListActionnerRequest{});
            let respsonse = client.list_actionner(request).await?.into_inner();
            println!("RESPONSE:{:?}", respsonse)
        }
    }
    Ok(())
}
