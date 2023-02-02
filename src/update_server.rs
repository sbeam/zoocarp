use async_trait::async_trait;
use ezsockets::Error;
use ezsockets::Socket;
use std::collections::HashMap;
use std::net::SocketAddr;

type SessionID = u16;
type Session = ezsockets::Session<SessionID, ()>;

pub struct UpdateServer {
    pub sessions: HashMap<SessionID, Session>,
    pub handle: ezsockets::Server<Self>,
}

pub struct ServerSession {
    handle: Session,
    id: SessionID,
}

#[derive(Debug)]
pub enum UpdateNotification {
    LotUpdated,
}

#[async_trait]
impl ezsockets::SessionExt for ServerSession {
    type ID = SessionID;
    type Args = ();
    type Params = ();

    fn id(&self) -> &Self::ID {
        &self.id
    }

    async fn text(&mut self, text: String) -> Result<(), Error> {
        self.handle.text(text);
        Ok(())
    }

    async fn binary(&mut self, _bytes: Vec<u8>) -> Result<(), Error> {
        unimplemented!()
    }

    async fn call(&mut self, _params: Self::Params) -> Result<(), Error> {
        Ok(())
    }
}

#[async_trait]
impl ezsockets::ServerExt for UpdateServer {
    type Session = ServerSession;
    type Params = UpdateNotification;

    async fn accept(
        &mut self,
        socket: Socket,
        address: SocketAddr,
        _args: (),
    ) -> Result<Session, Error> {
        let id = address.port();
        let session = Session::create(|handle| ServerSession { handle, id }, id, socket);
        Ok(session)
    }

    async fn disconnected(
        &mut self,
        _id: <Self::Session as ezsockets::SessionExt>::ID,
    ) -> Result<(), Error> {
        Ok(())
    }

    async fn call(&mut self, _params: Self::Params) -> Result<(), Error> {
        Ok(())
    }
}
