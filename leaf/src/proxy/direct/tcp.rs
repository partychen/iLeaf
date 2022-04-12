use std::{io, net::SocketAddr, sync::Arc};

use async_trait::async_trait;

use crate::{
    common::dns_client::DnsClient,
    proxy::{ProxyStream, ProxyTcpHandler},
    session::Session,
};

pub struct Handler {
    bind_addr: SocketAddr,
    dns_client: Arc<DnsClient>,
}

impl Handler {
    pub fn new(bind_addr: SocketAddr, dns_client: Arc<DnsClient>) -> Self {
        Handler {
            bind_addr,
            dns_client,
        }
    }
}

#[async_trait]
impl ProxyTcpHandler for Handler {
    fn name(&self) -> &str {
        super::NAME
    }

    fn tcp_connect_addr(&self) -> Option<(String, u16, SocketAddr)> {
        None
    }

    async fn handle<'a>(
        &'a self,
        sess: &'a Session,
        _stream: Option<Box<dyn ProxyStream>>,
    ) -> io::Result<Box<dyn ProxyStream>> {
        Ok(self
            .dial_tcp_stream(
                self.dns_client.clone(),
                &self.bind_addr,
                &sess.destination.host(),
                &sess.destination.port(),
            )
            .await?)
    }
}
