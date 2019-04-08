use chat_server::ConnectionPool;

use std::net::TcpListener;
use std::process;

fn main() {
    let size = 20;

    let conn_pool = match ConnectionPool::new(size) {
        Ok(pool) => {
            println!("Created connection pool of size {}", size);
            pool
        }
        Err(e) => {
            eprintln!("Error creating connection pool: {}", e);
            process::exit(1);
        }
    };

    let listener = TcpListener::bind("10.0.0.39:7878").expect("Could not bind to address.");
    println!("Server started at [{}]", listener.local_addr().unwrap());

    for stream in listener.incoming() {
        let stream = match stream {
            Ok(stream) => stream,
            Err(e) => {
                eprintln!("Error receiving stream: {}", e);
                continue;
            }
        };

        conn_pool.accept(stream).unwrap_or_else(|e| {
            eprintln!("Error accepting stream: {}", e);
        });
    }
}
