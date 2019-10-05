pub mod home {
    include!(concat!(env!("OUT_DIR"), "/home_server.rs"));
}

use crate::Action;
use futures::{future, prelude::*, Future};
use std::sync::Arc;
use tokio::sync::mpsc;
use tower_grpc::{Request, Response};

#[derive(Clone)]
pub struct RPCServer {
    state: Arc<State>,
}

struct State {
    action_requester: mpsc::Sender<Action>,
}

impl State {
    fn new(action_requester: mpsc::Sender<Action>) -> State {
        State { action_requester }
    }
}

impl RPCServer {
    pub fn server(
        action_requester: mpsc::Sender<Action>,
        bind_address: &str,
        port: u16,
    ) -> impl Future<Item = (), Error = ()> {
        let handler = RPCServer {
            state: Arc::new(State::new(action_requester)),
        };
        let new_service = home::server::HomeServerServer::new(handler);

        let mut server = tower_hyper::server::Server::new(new_service);
        let http = tower_hyper::server::Http::new().http2_only(true).clone();

        let addr = format!("{}:{}", bind_address, port)
            .parse()
            .expect("[gRCP] invalid address");
        let bind = tokio::net::TcpListener::bind(&addr).expect("[gRPC] Failed to bind");

        let serve = bind
            .incoming()
            .for_each(move |sock| {
                if let Err(e) = sock.set_nodelay(true) {
                    return Err(e);
                };
                let serve = server.serve_with(sock, http.clone());
                tokio::spawn(serve.map_err(|e| error!("[gRPC] h2 error: {}", e)));
                Ok(())
            })
            .map_err(|e| error!("[gRPC] accept error: {}", e));
        serve
    }
}

impl home::server::HomeServer for RPCServer {
    type GetInfoFuture = future::FutureResult<Response<home::GetInfoReply>, tower_grpc::Status>;

    fn get_info(&mut self, _request: Request<home::GetInfoRequest>) -> Self::GetInfoFuture {
        debug!("[gRPC] GetInfo");
        // TODO
        let response = Response::new(home::GetInfoReply {
            name: "Nothing".to_owned(),
            kind: "Nothing".to_owned(),
            kind_id: 0,
            status: home::Status::Unknown as i32,
        });
        future::ok(response)
    }

    type ListFuture = future::FutureResult<Response<home::ListReply>, tower_grpc::Status>;
    fn list(&mut self, _request: Request<home::ListRequest>) -> Self::ListFuture {
        debug!("[gRPC] List");
        // TODO
        let response = Response::new(home::ListReply {
            objects: Vec::new(),
        });
        future::ok(response)
    }

    type ChangeStatusFuture =
        future::FutureResult<Response<home::ChangeStatusReply>, tower_grpc::Status>;
    fn change_status(
        &mut self,
        _request: Request<home::ChangeStatusRequest>,
    ) -> Self::ChangeStatusFuture {
        debug!("[gRPC] ChangeStatus");
        // TODO
        let response = Response::new(home::ChangeStatusReply {
            new_status: home::Status::Unknown as i32,
        });
        future::ok(response)
    }
}
