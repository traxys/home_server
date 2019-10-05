use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(name = "home-cli", about = "A CLI to do some things from your home")]
struct Config {
    #[structopt(
        about = "The address of the home server",
        default_value = "http://localhost:1456"
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
    #[structopt(about = "information on an object")]
    GetInfo {
        #[structopt(help = "the id of the object to query")]
        id: u64,
    },
    #[structopt(about = "change the status of an object")]
    ChangeStatus {
        #[structopt(help = "the id of the object to change")]
        target: u64,
        #[structopt(help = "what to set the object to (on/off)")]
        new_status: Status,
    },
    #[structopt(about = "list objects, optionaly limit to a category")]
    List {
        #[structopt(help = "the optional category to search")]
        category: Option<u64>,
    },
}

fn main() {
    let args = Config::from_args();
    match args.action {
        Action::GetInfo { .. } => println!("Info"),
        Action::ChangeStatus { .. } => println!("ChangeStatus"),
        Action::List { .. } => println!("List"),
    }
}
