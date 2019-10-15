use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum ActionnerId {
    Arduino(i8),
    SSH(String),
}
impl ActionnerId {
    pub fn repr(&self) -> String {
        match &self {
            ActionnerId::Arduino(n) => format!("{}", n),
            ActionnerId::SSH(s) => s.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Object {
    pub actionner_id: u32,
    pub id_in_actionner: ActionnerId,
    pub kind: ObjectKind,
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub enum Protocol {
    Arduino,
    SSH,
}

impl Protocol {
    pub fn name(&self) -> String {
        match self {
            Protocol::Arduino => "Arduino",
            Protocol::SSH => "SSH",
        }
        .to_owned()
    }
}

impl std::str::FromStr for Protocol {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "arduino" => Ok(Self::Arduino),
            "ssh" => Ok(Self::SSH),
            _ => Err("unknown protocol"),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub enum ObjectKind {
    LED,
}

impl ObjectKind {
    pub fn name(&self) -> String {
        match self {
            ObjectKind::LED => "LED".to_owned(),
        }
    }
    pub fn id(&self) -> u32 {
        match self {
            ObjectKind::LED => 1,
        }
    }
}

impl std::str::FromStr for ObjectKind {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "led" => Ok(Self::LED),
            _ => Err("unknown object kind"),
        }
    }
}
