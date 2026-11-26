//! The three services `serve` runs (redirector, main Blaze server, QoS), bound
//! together and stopped together.

use crate::event::Event;
use crate::players::{PlayerBook, Roster};
use crate::snapshot::Snapshot;
use crate::{blaze, event, models, qos, redirector};
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio_util::sync::{CancellationToken, DropGuard};

/// Port the game's redirector lookup connects to.
pub const REDIRECTOR_PORT: u16 = 42127;
/// Default port of the main Blaze server handed out in the redirect.
pub const DEFAULT_BLAZE_PORT: u16 = 42128;
/// Default QoS HTTP port; EA's QoS servers use the same one.
pub const DEFAULT_QOS_HTTP_PORT: u16 = 17502;
/// Default QoS UDP probe port; EA's QoS servers use the same one.
pub const DEFAULT_QOS_UDP_PORT: u16 = 17499;

/// Ports of the three services; 0 lets the system pick a free one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ports {
    pub redirector: u16,
    pub blaze: u16,
    pub qos: qos::Ports,
}

impl Ports {
    /// Ports that let the system pick a free one for every service.
    pub const ANY: Ports = Ports {
        redirector: 0,
        blaze: 0,
        qos: qos::Ports { http: 0, udp: 0 },
    };
}

impl Default for Ports {
    /// Returns [`REDIRECTOR_PORT`], [`DEFAULT_BLAZE_PORT`],
    /// [`DEFAULT_QOS_HTTP_PORT`] and [`DEFAULT_QOS_UDP_PORT`].
    fn default() -> Self {
        Ports {
            redirector: REDIRECTOR_PORT,
            blaze: DEFAULT_BLAZE_PORT,
            qos: qos::Ports {
                http: DEFAULT_QOS_HTTP_PORT,
                udp: DEFAULT_QOS_UDP_PORT,
            },
        }
    }
}

/// Settings of a server started with [`Server::start`].
pub struct Config {
    /// Address every listener binds to.
    pub bind: Ipv4Addr,
    /// Address handed to clients as this machine's (redirect, telemetry, QoS).
    pub advertise: Ipv4Addr,
    pub ports: Ports,
    /// Player names and admission rules the server starts with.
    pub players: Roster,
}

/// Entry point that starts the three services.
pub struct Server;

impl Server {
    /// Binds the redirector, Blaze and QoS listeners on `config.bind` and
    /// serves them in a background task.
    ///
    /// `config` holds the addresses, ports and player settings. The redirect,
    /// the Blaze configuration and the QoS XML carry the bound ports, which
    /// differ from `config.ports` where those are 0. Returns a handle to the
    /// running server, or the error of the first listener that cannot be bound.
    pub async fn start(config: Config) -> std::io::Result<ServerHandle> {
        let redirector = TcpListener::bind((config.bind, config.ports.redirector)).await?;
        let blaze = TcpListener::bind((config.bind, config.ports.blaze)).await?;
        let qos_http = TcpListener::bind((config.bind, config.ports.qos.http)).await?;
        let qos_udp = UdpSocket::bind((config.bind, config.ports.qos.udp)).await?;
        let ports = Ports {
            redirector: redirector.local_addr()?.port(),
            blaze: blaze.local_addr()?.port(),
            qos: qos::Ports {
                http: qos_http.local_addr()?.port(),
                udp: qos_udp.local_addr()?.port(),
            },
        };
        let instance = redirector::Instance {
            ip: config.advertise,
            port: ports.blaze,
            secure: false,
        };
        let blaze_config = Arc::new(blaze::Config {
            advertise: config.advertise,
            qos: models::QosHttp {
                host: config.advertise,
                port: ports.qos.http,
            },
            players: Arc::new(PlayerBook::new(config.advertise, config.players)),
            games: Default::default(),
            connections: Default::default(),
        });
        let state = blaze_config.clone();
        let stop = CancellationToken::new();
        let token = stop.clone();
        let advertise = config.advertise;
        let task = tokio::spawn(async move {
            let services = async {
                tokio::try_join!(
                    redirector::serve(redirector, instance, token.clone()),
                    blaze::serve(blaze, blaze_config, token.clone()),
                    qos::serve(qos_http, qos_udp, advertise, token.clone()),
                )
                .map(|_| ())
            };
            let result = token.run_until_cancelled(services).await.unwrap_or(Ok(()));
            // A service that failed ends the sessions of the other two as well.
            token.cancel();
            result
        });
        Ok(ServerHandle {
            ports,
            state,
            stop: stop.clone(),
            task,
            _stop_on_drop: stop.drop_guard(),
        })
    }
}

/// Running server returned by [`Server::start`]; dropping it stops the server.
pub struct ServerHandle {
    ports: Ports,
    /// Configuration and state the Blaze sessions share.
    state: Arc<blaze::Config>,
    stop: CancellationToken,
    task: JoinHandle<std::io::Result<()>>,
    _stop_on_drop: DropGuard,
}

impl ServerHandle {
    /// Returns the ports the listeners are bound to.
    pub fn ports(&self) -> Ports {
        self.ports
    }

    /// Returns the player names and admission rules the server applies; changes
    /// made through it apply to the next connection.
    pub fn players(&self) -> &PlayerBook {
        &self.state.players
    }

    /// Returns the server's sessions, refused addresses, rooms and player book
    /// as they are now.
    pub fn snapshot(&self) -> Snapshot {
        blaze::snapshot(&self.state)
    }

    /// Returns a receiver of the events sent from now on.
    ///
    /// The channel is the process's own (see [`crate::event`]), so it also
    /// carries the log lines of other servers and tasks in the process.
    pub fn events(&self) -> broadcast::Receiver<Event> {
        event::subscribe()
    }

    /// Closes the listeners and ends every session task.
    pub fn stop(&self) {
        self.stop.cancel();
    }

    /// Waits until the server ends and returns the error of the service that
    /// failed, or `Ok` after [`ServerHandle::stop`].
    pub async fn finished(self) -> std::io::Result<()> {
        self.task.await.map_err(std::io::Error::other)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::authentication as auth;
    use crate::fire::{FireCodec, Frame, FrameType};
    use futures::{SinkExt, StreamExt};
    use std::time::Duration;
    use tdf::prelude::{TdfSerialize, TdfSerializer};
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpStream;
    use tokio_util::codec::Framed;

    /// Empty request body.
    struct Empty;

    impl TdfSerialize for Empty {
        fn serialize<S: TdfSerializer>(&self, _: &mut S) {}
    }

    /// Waits until `server`'s snapshot satisfies `done`, reading it again after
    /// every event; fails after five seconds.
    async fn wait_for(server: &ServerHandle, done: impl Fn(&Snapshot) -> bool) {
        let mut events = server.events();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !done(&server.snapshot()) {
                let _ = events.recv().await;
            }
        })
        .await
        .expect("the snapshot changes in time");
    }

    fn config(ports: Ports) -> Config {
        Config {
            bind: Ipv4Addr::LOCALHOST,
            advertise: Ipv4Addr::LOCALHOST,
            ports,
            players: Roster {
                own_name: "Host".into(),
                ..Default::default()
            },
        }
    }

    #[tokio::test]
    async fn start_reports_the_ports_the_system_picked() {
        let server = Server::start(config(Ports::ANY)).await.unwrap();
        let ports = server.ports();
        for port in [ports.redirector, ports.blaze, ports.qos.http, ports.qos.udp] {
            assert_ne!(port, 0, "{ports:?}");
        }
        TcpStream::connect((Ipv4Addr::LOCALHOST, ports.blaze))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn start_fails_when_a_port_is_taken() {
        let taken = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = taken.local_addr().unwrap().port();
        let ports = Ports {
            blaze: port,
            ..Ports::ANY
        };
        let err = Server::start(config(ports))
            .await
            .err()
            .expect("start fails");
        assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
    }

    #[tokio::test]
    async fn stop_frees_the_ports_and_ends_the_server() {
        let server = Server::start(config(Ports::ANY)).await.unwrap();
        let ports = server.ports();
        server.stop();
        server.finished().await.unwrap();
        TcpListener::bind((Ipv4Addr::LOCALHOST, ports.redirector))
            .await
            .unwrap();
        TcpListener::bind((Ipv4Addr::LOCALHOST, ports.blaze))
            .await
            .unwrap();
        TcpListener::bind((Ipv4Addr::LOCALHOST, ports.qos.http))
            .await
            .unwrap();
        UdpSocket::bind((Ipv4Addr::LOCALHOST, ports.qos.udp))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn stop_closes_an_open_connection() {
        let server = Server::start(config(Ports::ANY)).await.unwrap();
        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, server.ports().blaze))
            .await
            .unwrap();
        // Gives the accept loop time to start the session, so the stop ends a
        // session rather than a connection still waiting in the backlog.
        tokio::time::sleep(Duration::from_millis(200)).await;
        server.stop();
        let mut buffer = [0u8; 16];
        let read = tokio::time::timeout(Duration::from_secs(5), client.read(&mut buffer))
            .await
            .expect("the connection closes");
        assert!(matches!(read, Ok(0) | Err(_)), "{read:?}");
    }

    #[tokio::test]
    async fn stop_closes_a_logged_in_players_connection() {
        let server = Server::start(config(Ports::ANY)).await.unwrap();
        let tcp = TcpStream::connect((Ipv4Addr::LOCALHOST, server.ports().blaze))
            .await
            .unwrap();
        let mut client = Framed::new(tcp, FireCodec::default());
        let login = Frame::request(1, auth::COMPONENT, auth::SILENT_LOGIN, Empty);
        client.send(login).await.unwrap();
        loop {
            let frame = client.next().await.expect("reply").expect("frame");
            if frame.header.ty == FrameType::Response {
                break;
            }
        }
        server.stop();
        let next = tokio::time::timeout(Duration::from_secs(5), client.next())
            .await
            .expect("the connection closes");
        assert!(
            matches!(next, None | Some(Err(_))),
            "a frame arrived after the stop"
        );
    }

    #[tokio::test]
    async fn snapshot_follows_a_connection() {
        let server = Server::start(config(Ports::ANY)).await.unwrap();
        let client = TcpStream::connect((Ipv4Addr::LOCALHOST, server.ports().blaze))
            .await
            .unwrap();
        wait_for(&server, |s| {
            s.sessions.len() == 1 && s.sessions[0].address == Ipv4Addr::LOCALHOST
        })
        .await;
        assert_eq!(server.snapshot().sessions[0].name, "Host");
        drop(client);
        wait_for(&server, |s| s.sessions.is_empty()).await;
    }
}
