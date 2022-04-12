use std::net::SocketAddr;
use std::{io, sync::Arc, time};

use async_trait::async_trait;
use futures::future::BoxFuture;
use log::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex as TokioMutex;
use tokio::time::timeout;

use crate::{
    proxy::{ProxyHandler, ProxyStream, ProxyTcpHandler},
    session::{Session, SocksAddr},
};

pub struct Handler {
    pub actors: Vec<Arc<dyn ProxyHandler>>,
    pub fail_timeout: u32,
    pub schedule: Arc<TokioMutex<Vec<usize>>>,
    pub health_check_task: TokioMutex<Option<BoxFuture<'static, ()>>>,
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Measure(usize, u128); // (index, duration in millis)

impl Handler {
    pub fn new(
        actors: Vec<Arc<dyn ProxyHandler>>,
        fail_timeout: u32,
        health_check: bool,
        check_interval: u32,
        failover: bool,
    ) -> Self {
        let mut schedule = Vec::new();
        for i in 0..actors.len() {
            schedule.push(i);
        }
        let schedule = Arc::new(TokioMutex::new(schedule));

        let schedule2 = schedule.clone();
        let actors2 = actors.clone();
        let task = if health_check {
            let health_check_task: BoxFuture<'static, ()> = Box::pin(async move {
                loop {
                    let mut measures: Vec<Measure> = Vec::new();
                    for (i, a) in (&actors2).iter().enumerate() {
                        debug!("health checking tcp for [{}] index [{}]", a.tag(), i);
                        let single_measure = async move {
                            let sess = Session {
                                source: "0.0.0.0:0".parse().unwrap(),
                                destination: SocksAddr::Domain("www.google.com".to_string(), 80),
                            };
                            let start = tokio::time::Instant::now();
                            match a.handle(&sess, None).await {
                                Ok(mut stream) => {
                                    if stream.write_all(b"HEAD / HTTP/1.1\r\n\r\n").await.is_err() {
                                        return Measure(i, u128::MAX - 2); // handshake is ok
                                    }
                                    let mut buf = vec![0u8; 1];
                                    match stream.read_exact(&mut buf).await {
                                        // handshake, write and read are ok
                                        Ok(_) => {
                                            let elapsed =
                                                tokio::time::Instant::now().duration_since(start);
                                            Measure(i, elapsed.as_millis())
                                        }
                                        // handshake and write are ok
                                        Err(_) => Measure(i, u128::MAX - 3),
                                    }
                                }
                                // handshake not ok
                                Err(_) => Measure(i, u128::MAX),
                            }
                        };
                        match timeout(time::Duration::from_secs(10), single_measure).await {
                            Ok(m) => {
                                measures.push(m);
                            }
                            Err(_) => {
                                measures.push(Measure(i, u128::MAX - 1)); // timeout, better than handshake error
                            }
                        }
                    }

                    measures.sort_by(|a, b| a.1.cmp(&b.1));
                    trace!("sorted tcp health check results:\n{:#?}", measures);

                    let priorities: Vec<String> = measures
                        .iter()
                        .map(|m| {
                            // construct tag(millis)
                            let mut repr = actors2[m.0].tag().to_owned();
                            repr.push('(');
                            repr.push_str(m.1.to_string().as_str());
                            repr.push(')');
                            repr
                        })
                        .collect();

                    debug!(
                        "udp priority after health check: {}",
                        priorities.join(" > ")
                    );

                    let mut schedule = schedule2.lock().await;
                    schedule.clear();
                    if !failover {
                        // if failover is disabled, put only 1 actor in schedule
                        schedule.push(measures[0].0);
                        trace!("put {} in schedule", measures[0].0);
                    } else {
                        for m in measures {
                            schedule.push(m.0);
                            trace!("put {} in schedule", m.0);
                        }
                    }

                    drop(schedule); // drop the guard, to release the lock

                    tokio::time::delay_for(time::Duration::from_secs(check_interval as u64)).await;
                }
            });
            Some(health_check_task)
        } else {
            None
        };

        Handler {
            actors,
            fail_timeout,
            schedule,
            health_check_task: TokioMutex::new(task),
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
        if self.health_check_task.lock().await.is_some() {
            if let Some(task) = self.health_check_task.lock().await.take() {
                tokio::spawn(task);
            }
        }

        let schedule = self.schedule.lock().await.clone();

        for i in schedule {
            if i >= self.actors.len() {
                return Err(io::Error::new(io::ErrorKind::Other, "invalid actor index"));
            }

            match timeout(
                time::Duration::from_secs(self.fail_timeout as u64),
                (&self.actors[i]).handle(sess, None),
            )
            .await
            {
                // return before timeout
                Ok(t) => match t {
                    // return ok
                    Ok(v) => return Ok(v),
                    // return err
                    Err(_) => continue,
                },
                // after timeout
                Err(_) => continue,
            }
        }
        Err(io::Error::new(
            io::ErrorKind::Other,
            "all outbound attempts failed",
        ))
    }
}
