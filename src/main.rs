#[macro_use]
#[macro_escape]
mod command;
mod server;

use server::Server;
use std::net::TcpListener;
use std::process;

fn main() {
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap_or_else(|err| {
        eprintln!("Could not bind to address! Error: {}", err);
        process::exit(1);
    });

    let _ = Server::init(10, '/')
        .unwrap_or_else(|err| {
            eprintln!("Error starting server! Error: {:?}", err);
            process::exit(2);
        })
        .cmd("ping", ping)
        .cmd("add", add)
        .start(listener);
}

command!(ping(m) {
    (m.from, String::from("pong!"))
});

command!(add(msg, args) {
    if args.count < 2 {
        (msg.from, String::from("Not enough arguments!"))
    } else {
        let first = match args.get(0) {
            Some(num) => num.parse::<i64>().unwrap_or(0),
            None => unreachable!(),
        };

        let second = match args.get(1) {
            Some(num) => num.parse::<i64>().unwrap_or(0),
            None => unreachable!(),
        };

        (msg.from, format!("{}", first + second))
    }
});
