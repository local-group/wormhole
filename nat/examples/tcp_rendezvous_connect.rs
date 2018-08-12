extern crate bytes;
extern crate env_logger;
extern crate future_utils;
extern crate futures;
extern crate nat;
extern crate serde;
extern crate safe_crypto;
extern crate serde_json;
extern crate tokio_core;
extern crate tokio_io;
/// To use this, use a server with publicly accessible ports to act as a relay server between two
/// connecting tcp streams. This relay server will act as the channel when negotiating the
/// rendezvous connect.
///
/// For example, log into a VPS and run:
/// ```
/// $ socat TCP-LISTEN:45666 TCP-LISTEN:45667
/// ```
///
/// The run this example on two machines, on seperate networks, both hidden behind NATs:
/// ```
/// $ cargo run --example tcp_rendezvous_connect -- <address of your vps>:45666 blah blah blah
/// $ cargo run --example tcp_rendezvous_connect -- <address of your vps>:45667 blah blah blah
/// ```
///
/// If successful, the peers should be able to form a TCP connection directly to each other.
#[macro_use]
extern crate unwrap;
extern crate void;
extern crate clap;

use futures::{Async, AsyncSink, Future, Sink, Stream};
use nat::{RemoteTcpRendezvousServer, TcpStreamExt, P2p};
use safe_crypto::PublicId;
use std::net::{Shutdown, SocketAddr};
use std::fmt;
use tokio_core::net::TcpStream;
use tokio_io::codec::length_delimited::Framed;
use void::ResultVoidExt;

// TODO: figure out how to not need this.
struct DummyDebug<S>(S);

impl<S> fmt::Debug for DummyDebug<S> {
    fn fmt(&self, _f: &mut fmt::Formatter) -> fmt::Result {
        Ok(())
    }
}

impl<S: Stream> Stream for DummyDebug<S> {
    type Item = S::Item;
    type Error = S::Error;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        self.0.poll()
    }
}

impl<S: Sink> Sink for DummyDebug<S> {
    type SinkItem = S::SinkItem;
    type SinkError = S::SinkError;

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        self.0.start_send(item)
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        self.0.poll_complete()
    }
}

fn main() {
    unwrap!(env_logger::init());

    let matches = clap::App::new("TCP Rendezvous Connect")
        .arg(
            clap::Arg::with_name("relay-server")
                .short("r")
                .takes_value(true)
                .required(true)
                .help("Relay server address")
        )
        .arg(
            clap::Arg::with_name("disable-igd")
                .help("Disable IGD")
        )
        .arg(
            clap::Arg::with_name("traversal-server")
                .alias("ts")
                .requires("traversal-server-key")
                .help("Traversal server address")
        )
        .arg(
            clap::Arg::with_name("traversal-server-key")
                .alias("ts-key")
                .requires("traversal-server")
                .help("Traversal server public key")
        )
        .arg(
            clap::Arg::with_name("message")
                .index(1)
                .required(true)
                .help("The message to send")
        )
        .get_matches();

    let disable_igd = matches.is_present("disable-igd");
    let relay_server: SocketAddr = matches.value_of("relay-server")
        .map(|s| s.parse().unwrap())
        .unwrap();
    let traversal_server: Option<SocketAddr> = matches.value_of("traversal-server")
        .map(|s| s.parse().unwrap());
    let traversal_server_key: Option<String> = matches.value_of("traversal-server-key")
        .map(String::from);
    let message = matches.value_of("message")
        .map(String::from)
        .unwrap();

    let mc = P2p::default();
    if disable_igd {
        mc.disable_igd();
    }

    if let Some(server_addr) = traversal_server {
        let server_pub_key = unwrap!(
            traversal_server_key,
            "If echo address server is specified, it's public key must be given too.",
        );
        let server_pub_key: PublicId = unwrap!(serde_json::from_str(&server_pub_key));
        let addr_querier = RemoteTcpRendezvousServer::new(server_addr, server_pub_key);
        mc.add_tcp_addr_querier(addr_querier);
    }

    let message: Vec<u8> = message.into_bytes();

    let mut core = unwrap!(tokio_core::reactor::Core::new());
    let handle = core.handle();
    let res = core.run({
        TcpStream::connect(&relay_server, &handle)
            .map_err(|e| panic!("error connecting to relay server: {}", e))
            .and_then(move |relay_stream| {
                let relay_channel =
                    DummyDebug(Framed::new(relay_stream).map(|bytes| bytes.freeze()));
                TcpStream::rendezvous_connect(relay_channel, &handle, &mc)
                    .map_err(|e| panic!("rendezvous connect failed: {}", e))
                    .and_then(|stream| {
                        println!("connected!");
                        tokio_io::io::write_all(stream, message)
                            .map_err(|e| panic!("error writing to tcp stream: {}", e))
                            .and_then(|(stream, _)| {
                                unwrap!(stream.shutdown(Shutdown::Write));
                                tokio_io::io::read_to_end(stream, Vec::new())
                                    .map_err(|e| panic!("error reading from tcp stream: {}", e))
                                    .map(|(_, data)| {
                                        let recv_message = String::from_utf8_lossy(&data);
                                        println!("got message: {}", recv_message);
                                    })
                            })
                    })
            })
    });
    res.void_unwrap()
}
