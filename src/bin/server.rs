use std::{collections::{HashMap, HashSet}, sync::Arc};
use tokio::sync::{mpsc, Mutex};
use tonic::{transport::Server, Request, Response, Status};
use tokio::prelude::*;
use serde::{Serialize, Deserialize};

mod objects;
mod commands;
use commands::ArduinoCommand;
use objects::{Object, ObjectKind, Protocol, ActionnerId};

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
        let actionners = Actionners::open(data_dir.clone()).await?;
        let known_actionners = actionners.get_known();
        let devices = Arc::new(Mutex::new(Devices::open(data_dir, &known_actionners)?));
        Ok(HomeServer {
            devices,
            actionners: Arc::new(Mutex::new(actionners)),
        })
    }
}
pub struct Devices {
    devices: sled::Db,
    next_id_to_assign: u32,
}

impl Devices {
    pub fn get(&self, id: u32) -> Result<Option<Object>, DeviceError> {
        let data: Object = match self.devices.get(bincode::serialize(&id)?)? {
            Some(d) => bincode::deserialize(&d)?,
            None => return Ok(None),
        };
        Ok(Some(data))
    }
    pub fn add(&mut self, kind: ObjectKind, actionner_id: u32, name: String, id_in_actionner: ActionnerId) -> Result<u32, DeviceError> {
        let new_obj = Object {
            kind,
            actionner_id,
            id_in_actionner,
            name,
        };
        let id = self.next_id_to_assign;
        self.next_id_to_assign += 1;
        self.devices.insert(bincode::serialize(&id)?, bincode::serialize(&new_obj)?)?;
        Ok(id)
    }
    pub fn list(&mut self) -> Result<Vec<(u32, Object)>, DeviceError> {
        let mut list = Vec::with_capacity(self.devices.len());
        for entry in self.devices.iter() {
            let (id, obj) = entry?;
            let id = bincode::deserialize(&id)?;
            let obj = bincode::deserialize(&obj)?;
            list.push((id, obj))
        }
        Ok(list)
    }
    pub fn open(mut data_dir: std::path::PathBuf, known_actionners: &HashSet<u32>) -> Result<Devices, DeviceError> {
        data_dir.push("devices");
        let mut devices = Devices {
            devices: sled::Db::open(data_dir)?,
            next_id_to_assign: 0,
        };
        for res in devices.devices.iter() {
            let (id, data) = res?;
            let d_id: u32 = bincode::deserialize(&id)?;
            let data: Object = bincode::deserialize(&data)?;
            devices.next_id_to_assign = std::cmp::max(devices.next_id_to_assign, d_id + 1);
            if !known_actionners.contains(&data.actionner_id) {
                devices.devices.remove(id)?;
            }
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
    pub fn get_known(&self) -> HashSet<u32> {
        self.get_list().map(|a| a.id).collect()
    }
    pub fn protocol(&self, id: u32) -> Option<Protocol> {
        self.actionners.get(&id).map(|e| e.handler.protocol())
    }
    pub async fn act(&mut self, command: &[u8], object: &Object) -> Result<Option<CommandResult>, HandlerError> {
        match self.actionners.get_mut(&object.actionner_id) {
            Some(hdlr) => Ok(Some(hdlr.handler.command(command, object).await?)),
            None => Ok(None),
        }
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
    Handler(HandlerError),
    Database(sled::Error),
}
impl From<HandlerError> for ActionnerError {
    fn from(err: HandlerError) -> Self {
        Self::Handler(err)
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
        let list = match self.devices.lock().await.list() {
            Ok(i) => i,
            Err(_) => return Err(Status::new(tonic::Code::Internal, "internal error")),
        };
        let reply = ListDeviceReply {
            objects: list
                .into_iter()
                .map(|(id,obj)| home_manager::Object{id, actionner_id: obj.actionner_id, kind: obj.kind.name(), kind_id: obj.kind.id(), name: obj.name, id_in_actionner: obj.id_in_actionner.repr()})
                .collect(),
        };
        Ok(Response::new(reply))
    }
    async fn register_device(&self, request: Request<home_manager::RegisterDeviceRequest>)
        -> Result<Response<home_manager::RegisterDeviceReply>, Status> {
        let request = request.into_inner();
        let kind: ObjectKind = match request.kind.parse() {
            Ok(k) => k,
            Err(_) => {
                return Err(Status::new(tonic::Code::InvalidArgument, "invalid category"))
            }
        };
        let protocol = match self.actionners.lock().await.protocol(request.actionner_id) {
            Some(p) => p,
            None => return Err(Status::new(tonic::Code::NotFound, "actionner not found")),
        };
        let id = match protocol {
            Protocol::Arduino => ActionnerId::Arduino(match request.id_in_actionner.parse() {
                Ok(i) => i,
                Err(_) => return Err(Status::new(tonic::Code::InvalidArgument, "invalid id for protocol"))
            }),
            _ => unimplemented!(),
        };
        match self.devices.lock().await.add(kind, request.actionner_id, request.name, id) {
            Err(e) => {
                tracing::warn!("Internal error adding device: {:?}", e);
                Err(Status::new(tonic::Code::Internal, ""))
            }
            Ok(id) => {
                Ok(Response::new(home_manager::RegisterDeviceReply{id}))
            }
        }
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
            Err(ActionnerError::Handler(HandlerError::IoError(e))) => Err(
                tonic::Status::new(tonic::Code::Aborted, format!("could not create handler: {}", e))
            ),
            Err(ActionnerError::Handler(HandlerError::InvalidAddress)) => Err(
                tonic::Status::new(tonic::Code::InvalidArgument, "invalid address")
            ),
            Err(ActionnerError::Handler(HandlerError::Internal)) => Err(
                tonic::Status::new(tonic::Code::Internal, "error creating handler")
            ),
            Err(_) => Err(
                tonic::Status::new(tonic::Code::Internal, "internal error")
            ),
        }
    }

    async fn command(&self, request: Request<home_manager::CommandRequest>) -> Result<Response<home_manager::CommandReply>, Status> {
        let request = request.into_inner();
        match self.devices.lock().await.get(request.object_id) {
            Err(_) => return Err(tonic::Status::new(tonic::Code::Internal, "")),
            Ok(None) => return Err(tonic::Status::new(tonic::Code::NotFound, "device not found")),
            Ok(Some(obj)) => match self.actionners.lock().await.act(&request.command, &obj).await {
                Ok(_) => (),  // One day
                Err(e) => {
                    tracing::warn!("Error in handler: {:?}", e);
                    return Err(tonic::Status::new(tonic::Code::Internal, ""))
                }
            },
        }
        let response = Response::new(home_manager::CommandReply{reply: String::new()});
        Ok(response)
    }
}

pub enum Action {}

struct SshHandler;
struct ArduinoHandler {
    address: String,
}
impl ArduinoHandler {
    async fn send(&self, command: ArduinoCommand, intern_id: i8) -> Result<(), tokio::io::Error> {
        let mut stream = tokio::net::TcpStream::connect(&self.address).await?;
        stream.write_all(command.repr(intern_id).as_bytes()).await?;
        Ok(())
    }
    async fn check(&self) -> Result<bool, tokio::io::Error> {
        let mut stream = tokio::timer::Timeout::new(tokio::net::TcpStream::connect(&self.address), std::time::Duration::from_millis(100)).await??;
        stream.write_all(ArduinoCommand::Check.repr(0).as_bytes()).await?;
        let mut buffer = [0; 16];
        stream.read(&mut buffer).await?;
        Ok(&buffer[0..3] == b"yes")
    }
}

enum Handler {
    Arduino(ArduinoHandler),
    SSH(SshHandler),
}

#[derive(Debug)]
pub enum HandlerError {
    InvalidAddress,
    IoError(tokio::io::Error),
    Internal,
    InvalidCommand(bincode::Error),
    InvalidId,
}
impl From<tokio::io::Error> for HandlerError {
    fn from(err: tokio::io::Error) -> Self {
        Self::IoError(err)
    }
}
impl From<bincode::Error> for HandlerError {
    fn from(err: bincode::Error) -> Self {
        Self::InvalidCommand(err)
    }
}

type CommandResult = ();

impl Handler {
    fn protocol(&self) -> Protocol {
        match self {
            Handler::Arduino(_) => Protocol::Arduino,
            Handler::SSH(_) => Protocol::SSH,
        }
    }
    async fn new(protocol: Protocol, remote: String) -> Result<Handler, HandlerError> {
        match protocol {
            Protocol::SSH => unimplemented!(),
            Protocol::Arduino => {
                let handler = ArduinoHandler{address: remote};
                if !handler.check().await? {
                    tracing::warn!("Arduino did not respond yes to ard request");
                    return Err(HandlerError::Internal)
                }
                Ok(Handler::Arduino(handler))
            }
        }
    }
    async fn command(&mut self, command: &[u8], object: &Object) -> Result<CommandResult, HandlerError> {
        match self {
            Handler::Arduino(arduino) => {
                match object.id_in_actionner {
                    ActionnerId::Arduino(id) => {
                        let command: ArduinoCommand = bincode::deserialize(command)?;
                        arduino.send(command, id).await?;
                    }
                    _ => return Err(HandlerError::InvalidId)
                }
            }
            _ => unimplemented!(),
        }
        Ok(())
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
