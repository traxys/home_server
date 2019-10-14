use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, Mutex};
use tonic::{transport::Server, Request, Response, Status};
use tokio::prelude::*;
use serde::{Serialize, Deserialize};

mod objects;
use objects::{Object, ObjectKind, Protocol};

pub mod home_manager {
    tonic::include_proto!("home_manager");
}

use home_manager::{
    server::{HomeManager, HomeManagerServer},
    ListDeviceReply, ListDeviceRequest,
};

pub struct HomeServer {
    devices: Arc<Mutex<Devices>>,
    actionners: Arc<Mutex<Actionners>>,
}

#[derive(Debug)]
pub enum ServerCreationError {
    Sled(sled::Error),
    Actionners(ActionnerError),
    Devices(DeviceError),
}
impl From<sled::Error> for ServerCreationError {
    fn from(err: sled::Error) -> ServerCreationError {
        ServerCreationError::Sled(err)
    }
}
impl From<ActionnerError> for ServerCreationError {
    fn from(err: ActionnerError) -> ServerCreationError {
        ServerCreationError::Actionners(err)
    }
}
impl From<DeviceError> for ServerCreationError {
    fn from(err: DeviceError) -> ServerCreationError {
        ServerCreationError::Devices(err)
    }
}

impl HomeServer {
    pub async fn open(data_dir: std::path::PathBuf) -> Result<HomeServer, ServerCreationError> {
        let devices = Arc::new(Mutex::new(Devices::open(data_dir.clone())?));
        let actionners = Arc::new(Mutex::new(Actionners::open(data_dir).await?));
        Ok(HomeServer {
            devices,
            actionners,
        })
    }
}
pub struct Devices {
    devices: sled::Db,
    next_id_to_assign: u32,
}

impl Devices {
    pub fn add(&mut self, kind: ObjectKind, protocol: Protocol, name: String) -> Result<u32, DeviceError> {
        let new_obj = Object {
            kind,
            protocol,
            name,
        };
        let id = self.next_id_to_assign;
        self.next_id_to_assign += 1;
        self.devices.insert(bincode::serialize(&id)?, bincode::serialize(&new_obj)?)?;
        Ok(id)
    }
    pub fn ids(&mut self) -> Result<Vec<u32>, DeviceError> {
        let mut ids = Vec::with_capacity(self.devices.len());
        for entry in self.devices.iter() {
            let (id, _) = entry?;
            let id = bincode::deserialize(&id)?;
            ids.push(id)
        }
        Ok(ids)
    }
    pub fn open(mut data_dir: std::path::PathBuf) -> Result<Devices, DeviceError> {
        data_dir.push("devices");
        let mut devices = Devices {
            devices: sled::Db::open(data_dir)?,
            next_id_to_assign: 0,
        };
        if let Some(n) = devices.ids()?.into_iter().max() {
            devices.next_id_to_assign = n + 1;
        }
        Ok(devices)
    }
}
#[derive(Debug)]
pub enum DeviceError {
    Sled(sled::Error),
    Serde(bincode::Error),
}
impl From<sled::Error> for DeviceError {
    fn from(err: sled::Error) -> Self {
        Self::Sled(err)
    }
}
impl From<bincode::Error> for DeviceError {
    fn from(err: bincode::Error) -> Self {
        Self::Serde(err)
    }
}

pub struct Actionners {
    actionner_data: sled::Db,
    actionners: HashMap<u32, Actionner>,
    next_id_to_assign: u32,
}

impl Actionners {
    pub async fn add(&mut self, data: ActionnerData) -> Result<u32, ActionnerError> {
        let id = self.next_id_to_assign;
        self.next_id_to_assign += 1;
        let ser_data = bincode::serialize(&data)?;
        let new_actionner = Actionner {
            name: data.name,
            handler: Handler::new(data.protocol, data.remote).await?,
        };
        self.actionner_data.insert(bincode::serialize(&id)?, ser_data)?;
        self.actionners.insert(id, new_actionner);
        Ok(id)
    }
    pub async fn open(mut data_dir: std::path::PathBuf) -> Result<Actionners, ActionnerError> {
        data_dir.push("actionners");
        let actionner_data = sled::Db::open(data_dir)?;
        let mut actionners = HashMap::with_capacity(actionner_data.len());
        let mut next_id_to_assign = 0;
        for res in actionner_data.iter() {
            let (id, creator) = res?;
            let id: u32 = bincode::deserialize(&id)?;
            let creator: ActionnerData = bincode::deserialize(&creator)?;
            next_id_to_assign = std::cmp::max(next_id_to_assign, id + 1);
            let handler = match Handler::new(creator.protocol, creator.remote).await {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!("Unregistered {} for {:?}", creator.name, e);
                    actionner_data.remove(bincode::serialize(&id)?)?;
                    continue
                }
            };
            actionners.insert(id, Actionner{name: creator.name, handler});
        }
        Ok(Self {
            actionner_data,
            actionners,
            next_id_to_assign,
        })
    }
    pub fn get_list(&self) -> impl Iterator<Item = home_manager::Actionner> + '_ {
        self.actionners.iter().map(|(id, act)| home_manager::Actionner{id: *id, name: act.name.clone(), protocol: act.handler.protocol().name()})
    }
}
#[derive(Serialize, Deserialize)]
pub struct ActionnerData {
    protocol: Protocol,
    remote: String,
    name: String,
}
#[derive(Debug)]
pub enum ActionnerError {
    SerDeError(bincode::Error),
    HandlerCreation(HandlerCreationError),
    Database(sled::Error),
}
impl From<HandlerCreationError> for ActionnerError {
    fn from(err: HandlerCreationError) -> Self {
        Self::HandlerCreation(err)
    }
}
impl From<sled::Error> for ActionnerError {
    fn from(err: sled::Error) -> Self {
        Self::Database(err)
    }
}
impl From<bincode::Error> for ActionnerError {
    fn from(err: bincode::Error) -> Self {
        Self::SerDeError(err)
    }
}
pub struct Actionner {
    handler: Handler,
    name: String,
}

#[tonic::async_trait]
impl HomeManager for HomeServer {
    async fn list_device(
        &self,
        _request: Request<ListDeviceRequest>,
    ) -> Result<Response<ListDeviceReply>, Status> {
        tracing::info!("Listing devices");
        let ids = match self.devices.lock().await.ids() {
            Ok(i) => i,
            Err(_) => return Err(Status::new(tonic::Code::Internal, "internal error")),
        };
        let reply = ListDeviceReply {
            objects: ids
                .into_iter()
                .map(|id| home_manager::Object { id })
                .collect(),
        };
        Ok(Response::new(reply))
    }
    async fn list_actionner(
        &self,
        _request: Request<home_manager::ListActionnerRequest>
    ) -> Result<Response<home_manager::ListActionnerReply>, Status> {
        Ok(Response::new(home_manager::ListActionnerReply{
         actionners: self.actionners.lock().await.get_list().collect()
        }))
    }

    async fn register_actionner(
        &self,
        request: Request<home_manager::RegisterActionnerRequest>,
    ) -> Result<Response<home_manager::RegisterActionnerReply>, Status> {
        let reg_req = request.into_inner();
        let protocol = match reg_req.protocol.parse() {
            Ok(k) => k,
            Err(_) => {
                return Err(Status::new(
                    tonic::Code::InvalidArgument,
                    "invalid protocol",
                ))
            }
        };
        let data = ActionnerData {
            protocol,
            remote: reg_req.remote,
            name: reg_req.name
        };
        match self.actionners.lock().await.add(data).await {
            Ok(id) => Ok(Response::new(home_manager::RegisterActionnerReply {
                id
            })),
            Err(ActionnerError::HandlerCreation(HandlerCreationError::IoError(e))) => Err(
                tonic::Status::new(tonic::Code::Aborted, format!("could not create handler: {}", e))
            ),
            Err(ActionnerError::HandlerCreation(HandlerCreationError::InvalidAddress)) => Err(
                tonic::Status::new(tonic::Code::InvalidArgument, "invalid address")
            ),
            Err(ActionnerError::HandlerCreation(HandlerCreationError::Internal)) => Err(
                tonic::Status::new(tonic::Code::Internal, "error creating handler")
            ),
            Err(_) => Err(
                tonic::Status::new(tonic::Code::Internal, "internal error")
            ),
        }
    }
}

pub enum Action {}

struct SshHandler;
struct ArduinoHandler {
    address: String,
}
impl ArduinoHandler {
    async fn send(&self, command: ArduinoCommand) -> Result<(), tokio::io::Error> {
        let mut stream = tokio::net::TcpStream::connect(&self.address).await?;
        stream.write_all(command.repr().as_bytes()).await?;
        Ok(())
    }
    async fn check(&self) -> Result<bool, tokio::io::Error> {
        let mut stream = tokio::timer::Timeout::new(tokio::net::TcpStream::connect(&self.address), std::time::Duration::from_millis(100)).await??;
        stream.write_all(ArduinoCommand::Check.repr().as_bytes()).await?;
        let mut buffer = [0; 16];
        stream.read(&mut buffer).await?;
        Ok(&buffer[0..3] == b"yes")
    }
}

enum ArduinoCommand {
    Set { id: i8, state: bool },
    Toggle { id: i8 },
    Check,
}

impl ArduinoCommand {
    fn repr(&self) -> String {
        match self {
            ArduinoCommand::Set { state: true, id } => format!("on {}\n", id),
            ArduinoCommand::Set { state: false, id } => format!("off {}\n", id),
            ArduinoCommand::Toggle { id } => format!("tog {}\n", id),
            ArduinoCommand::Check => format!("ard\n"),
        }
    }
}

enum Handler {
    Arduino(ArduinoHandler),
    SSH(SshHandler),
}

#[derive(Debug)]
pub enum HandlerCreationError {
    InvalidAddress,
    IoError(tokio::io::Error),
    Internal,
}

impl From<tokio::io::Error> for HandlerCreationError {
    fn from(err: tokio::io::Error) -> Self {
        Self::IoError(err)
    }
}

impl Handler {
    fn protocol(&self) -> Protocol {
        match self {
            Handler::Arduino(_) => Protocol::Arduino,
            Handler::SSH(_) => Protocol::SSH,
        }
    }
    async fn new(protocol: Protocol, remote: String) -> Result<Handler, HandlerCreationError> {
        match protocol {
            Protocol::SSH => unimplemented!(),
            Protocol::Arduino => {
                let handler = ArduinoHandler{address: remote};
                if !handler.check().await? {
                    tracing::warn!("Arduino did not respond yes to ard request");
                    return Err(HandlerCreationError::Internal)
                }
                Ok(Handler::Arduino(handler))
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut data_dir = dirs::data_dir().expect("did not find data dir");
    data_dir.push("home_manager");
    let addr = "[::1]:14563".parse().unwrap();

    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt::Subscriber::builder()
            .with_max_level(tracing::Level::DEBUG)
            .with_target(true)
            .inherit_fields(true)
            .finish(),
    )
    .unwrap();

    let server = HomeServer::open(data_dir).await.unwrap();
    Server::builder()
        .serve(addr, HomeManagerServer::new(server))
        .await?;
    Ok(())
}
