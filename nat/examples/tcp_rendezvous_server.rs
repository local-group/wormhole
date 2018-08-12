extern crate futures;
extern crate nat;
extern crate serde_json;
extern crate tokio_core;
#[macro_use]
extern crate unwrap;

use futures::{future, Future};
use nat::{TcpRendezvousServer, P2p};

fn main() {
    let mut core = unwrap!(tokio_core::reactor::Core::new());
    let handle = core.handle();
    let mc = P2p::default();
    let res = core.run({
        TcpRendezvousServer::bind_public(&"0.0.0.0:0".parse().unwrap(), &handle, &mc)
            .map_err(|e| panic!("Error binding server publicly: {}", e))
            .and_then(|(server, public_addr)| {
                println!("listening on public socket address {}", public_addr);
                println!(
                    "our public key is: {}",
                    unwrap!(serde_json::to_string(&server.public_key()))
                );

                future::empty().map(|()| drop(server))
            })
    });
    unwrap!(res);
}
