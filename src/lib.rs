use std::error::Error;
use std::fmt;
use std::io;
use std::io::prelude::*;
use std::net::TcpStream;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

extern crate rand;

#[derive(Clone)]
struct Message {
    contents: String,
    from: u64,
    to: Option<u64>,
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}> {}", self.from, self.contents)
    }
}

impl Message {
    fn new(contents: String, from: u64, to: Option<u64>) -> Message {
        Message { contents, from, to }
    }
}

pub struct ConnectionPool {
    handlers: Vec<Handler>,
    conn_sender: mpsc::Sender<Connection>,
    broadcaster: Arc<Mutex<Broadcaster>>,
    broadcast_thread: thread::JoinHandle<()>,
}

impl ConnectionPool {
    pub fn new(size: usize) -> Result<ConnectionPool, &'static str> {
        if size == 0 {
            return Err("Can not create a connection pool with 0 connections.");
        }

        let (msg_sender, msg_receiver) = mpsc::channel();
        let (conn_sender, conn_receiver) = mpsc::channel();
        let conn_receiver = Arc::new(Mutex::new(conn_receiver));

        let broadcaster = Arc::new(Mutex::new(Broadcaster::new()));

        let mut handlers = Vec::with_capacity(size);

        for id in 0..size {
            let conn_receiver = Arc::clone(&conn_receiver);
            let msg_sender = msg_sender.clone();

            handlers.push(Handler::new(id, conn_receiver, msg_sender));
        }

        let mutex_broadcaster = Arc::clone(&broadcaster);

        let broadcast_thread = thread::spawn(move || {
            let broadcaster = mutex_broadcaster;

            loop {
                for msg in msg_receiver.recv() {
                    let mut broadcaster = broadcaster.lock().unwrap();

                    if msg.contents != "" {
                        println!("{}", msg);

                        broadcaster.broadcast(msg);
                    }
                }
            }
        });

        let conn_pool = ConnectionPool {
            handlers,
            conn_sender,
            broadcaster,
            broadcast_thread,
        };

        Ok(conn_pool)
    }

    pub fn accept(&self, raw_stream: TcpStream) -> Result<(), Box<dyn Error + '_>> {
        let mut broadcaster = self.broadcaster.lock()?;

        // TODO: Check this, make sure the connection pool doesn't
        // contain a connection with the same id.
        let id = rand::random::<u64>();

        // raw_stream.set_nonblocking(true)?;

        // TODO: Make this duration a config option.
        let timeout = Some(Duration::from_secs(120));

        broadcaster.add(raw_stream.try_clone()?);

        let connection = Connection::new(id, raw_stream, timeout)?;

        self.conn_sender.send(connection)?;

        Ok(())
    }
}

struct Connection {
    id: u64,
    raw_stream: TcpStream,
}

impl Connection {
    fn new(
        id: u64,
        raw_stream: TcpStream,
        timeout: Option<Duration>,
    ) -> Result<Connection, io::Error> {
        if let Some(_) = timeout {
            raw_stream.set_read_timeout(timeout)?;
        }

        let conn = Connection { id, raw_stream };

        Ok(conn)
    }
}

#[derive(Debug)]
struct Handler {
    id: usize,
    thread: thread::JoinHandle<()>,
}

impl Handler {
    fn new(
        id: usize,
        conn_receiver: Arc<Mutex<mpsc::Receiver<Connection>>>,
        msg_sender: mpsc::Sender<Message>,
    ) -> Handler {
        let thread = thread::spawn(move || loop {
            let mut connection = conn_receiver.lock().unwrap().recv().unwrap();

            println!("Handler {} has received connection {}.", id, connection.id);

            Handler::handle_connection(&mut connection, &msg_sender).unwrap_or_else(|e| {
                eprintln!(
                    "There was an error handling Connection {}: {}",
                    connection.id, e
                );
            });
        });

        Handler { id, thread }
    }

    fn handle_connection(
        connection: &mut Connection,
        msg_sender: &mpsc::Sender<Message>,
    ) -> Result<(), Box<dyn Error>> {
        loop {
            let mut msg_buffer = [b'\n'; 1024];

            connection.raw_stream.read(&mut msg_buffer)?;

            let msg_str = String::from(String::from_utf8_lossy(&msg_buffer).trim());

            // This `None` should be replaced with a `to` recipient if there
            // is one. This will just broadcast for now.
            let msg = Message::new(msg_str, connection.id, None);

            msg_sender.send(msg)?;
        }
    }
}

struct Broadcaster {
    streams: Vec<TcpStream>,
}

impl Broadcaster {
    fn new() -> Broadcaster {
        Broadcaster { streams: vec![] }
    }

    fn broadcast(&mut self, msg: Message) {
        let msg = msg.to_string();

        for stream in &mut self.streams {
            stream.write(msg.as_bytes()).ok();
            stream.flush().ok();
        }
    }

    fn add(&mut self, stream: TcpStream) {
        self.streams.push(stream);
    }
}
