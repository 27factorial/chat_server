mod server;

use std::net::TcpListener;
use server::Server;
use std::process;

fn main() {
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap_or_else(|err| {
        eprintln!("Could not bind to address! Error: {}", err);
        process::exit(1);
    });

    let _ = Server::init(10).unwrap_or_else(|err| {
        eprintln!("Error starting server! Error: {:?}", err);
        process::exit(2);
    }).start(listener);
}
