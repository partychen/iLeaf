use std::{io, pin::Pin, sync::Arc};

use futures::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Mutex as TokioMutex;

use crate::app::dispatcher::Dispatcher;
use crate::app::nat_manager::NatManager;
use crate::common::fake_dns::FakeDns;

use super::stack_impl::NetStackImpl;

pub struct NetStack(Box<NetStackImpl>);

impl NetStack {
    pub fn new(
        dispatcher: Arc<Dispatcher>,
        nat_manager: Arc<NatManager>,
        fakedns: Arc<TokioMutex<FakeDns>>,
    ) -> Self {
        NetStack(NetStackImpl::new(dispatcher, nat_manager, fakedns))
    }
}

impl AsyncRead for NetStack {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        AsyncRead::poll_read(Pin::new(&mut self.0), cx, buf)
    }
}

impl AsyncWrite for NetStack {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        AsyncWrite::poll_write(Pin::new(&mut self.0), cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        AsyncWrite::poll_flush(Pin::new(&mut self.0), cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        AsyncWrite::poll_shutdown(Pin::new(&mut self.0), cx)
    }
}
