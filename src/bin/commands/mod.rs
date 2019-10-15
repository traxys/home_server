use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum ArduinoCommand {
    Set { state: bool },
    Toggle,
    Check,
}

impl ArduinoCommand {
    pub fn repr(&self, id: i8) -> String {
        match self {
            ArduinoCommand::Set { state: true } => format!("on {}\n", id),
            ArduinoCommand::Set { state: false } => format!("off {}\n", id),
            ArduinoCommand::Toggle => format!("tog {}\n", id),
            ArduinoCommand::Check => format!("ard\n"),
        }
    }
}
