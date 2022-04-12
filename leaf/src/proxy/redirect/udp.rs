use std::{
    io::Result,
    net::{IpAddr, SocketAddr},
};

use async_trait::async_trait;
use futures::TryFutureExt;
use tokio::net::{
    udp::{RecvHalf, SendHalf},
    UdpSocket,
};

use crate::{
    proxy::{
        ProxyDatagram, ProxyDatagramRecvHalf, ProxyDatagramSendHalf, ProxyStream, ProxyUdpHandler,
        UdpTransportType,
    },
    session::Session,
};

/// Handler with a redirect target address.
pub struct Handler {
    pub address: String,
    pub port: u16,
}

#[async_trait]
impl ProxyUdpHandler for Handler {
    fn name(&self) -> &str {
        super::NAME
    }

    fn udp_connect_addr(&self) -> Option<(String, u16, SocketAddr)> {
        None
    }

    fn udp_transport_type(&self) -> UdpTransportType {
        UdpTransportType::Packet
    }

    async fn connect<'a>(
        &'a self,
        _sess: &'a Session,
        _datagram: Option<Box<dyn ProxyDatagram>>,
        _stream: Option<Box<dyn ProxyStream>>,
    ) -> Result<Box<dyn ProxyDatagram>> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        let (rh, sh) = socket.split();
        let addr = SocketAddr::new(self.address.parse::<IpAddr>().unwrap(), self.port);
        Ok(Box::new(Datagram {
            recv_half: rh,
            send_half: sh,
            target: addr,
        }))
    }
}

pub struct Datagram {
    pub recv_half: RecvHalf,
    pub send_half: SendHalf,
    pub target: SocketAddr,
}

impl ProxyDatagram for Datagram {
    fn split(
        self: Box<Self>,
    ) -> (
        Box<dyn ProxyDatagramRecvHalf>,
        Box<dyn ProxyDatagramSendHalf>,
    ) {
        (
            Box::new(DatagramRecvHalf(self.recv_half, self.target)),
            Box::new(DatagramSendHalf(self.send_half, self.target)),
        )
    }
}

pub struct DatagramRecvHalf(RecvHalf, SocketAddr);

#[async_trait]
impl ProxyDatagramRecvHalf for DatagramRecvHalf {
    async fn recv_from(&mut self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        let addr = self.1;
        self.0.recv_from(buf).map_ok(|(n, _)| (n, addr)).await
    }
}

pub struct DatagramSendHalf(SendHalf, SocketAddr);

#[async_trait]
impl ProxyDatagramSendHalf for DatagramSendHalf {
    async fn send_to(&mut self, buf: &[u8], _target: &SocketAddr) -> Result<usize> {
        self.0.send_to(buf, &self.1).await
    }
}
