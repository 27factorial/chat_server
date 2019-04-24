extern crate hashbrown;
extern crate rand;

use self::ServerError::*;

use hashbrown::HashMap;

use rand::Rng;

use std::convert::TryFrom;
use std::fmt;
use std::io;
use std::io::prelude::*;
use std::net::TcpListener;
use std::net::TcpStream;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

type Id = usize;
type ServerResult<T> = Result<T, ServerError>;

#[derive(Debug)]
pub enum ServerError {
    InvalidConfig(&'static str),
    IoError(io::Error),
    ServerFull,
}

enum HandlerAsync {
    Working,
    Finished(FinishedStatus),
}

enum FinishedStatus {
    Terminated,
    Panicked,
    TimedOut,
    Errored(io::ErrorKind),
}

pub struct Server {
    size: usize,
    msg_sender: Sender<Message>,
    msg_recver: Receiver<Message>,
    handlers: HashMap<Id, Handler>,
}

impl Server {
    pub fn init(size: usize) -> ServerResult<Server> {
        if size == 0 {
            return Err(InvalidConfig("Server can not have zero connections."));
        }

        let (msg_sender, msg_recver) = mpsc::channel();

        let handlers = HashMap::with_capacity(size);

        Ok(Server {
            size,
            msg_sender,
            msg_recver,
            handlers,
        })
    }

    #[allow(unused)]
    pub fn from_cfg() -> Result<Server, &'static str> {
        unimplemented!();
    }

    #[allow(unused)]
    pub fn cmd(name: &str) {
        unimplemented!();
    }

    pub fn start(mut self, listener: TcpListener) {
        listener
            .set_nonblocking(true)
            .expect("Could not set listener into non-blocking mode!");

        eprintln!("Server started!");

        for stream in listener.incoming() {
            match stream {
                Ok(s) => self
                    .accept(s)
                    .and_then(|id| {
                        println!("Connection {} accepted!", id);
                        Ok(())
                    })
                    .unwrap_or_else(|_err| eprintln!("Error accepting new connection!")),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // No new connections, so let's check if we need to remove
                    // any handlers, then check for messages.
                    self.check_handlers().into_iter().for_each(|id| {
                        self.handlers
                            .remove(&id)
                            .and_then(|handler| handler.thread.join().ok());
                    });
                    self.check_and_broadcast();
                }
                Err(_) => {
                    eprintln!("There was an error receiving the connection!");
                }
            }
        }
    }

    fn accept(&mut self, stream: TcpStream) -> ServerResult<Id> {
        // Do not accept a connection if it would exceed the
        // max connections on the server. Just return an error
        // indicating that the server is full.
        if self.handlers.len() == self.size {
            return Err(ServerFull);
        }

        // We have to make sure that we don't have a duplicate
        // connection id. This is very unlikely to happen, but
        // it can, so I have to check. (Damn you, randomness!)
        let id = {
            let mut rng = rand::thread_rng();
            let mut id: usize = rng.gen();
            while let Some(_) = self.handlers.get(&id) {
                id = rng.gen();
            }
            id
        };

        let msg_sender = self.msg_sender.clone();

        let conn = Connection::new(stream, id).map_err(IoError)?;
        let handler = Handler::accept(conn, msg_sender, Duration::from_secs(120));

        // Don't care about the return type here since it
        // will always return None, due to our id check
        // at the beginning.
        self.handlers.insert(id, handler);

        Ok(id)
    }

    fn check_handlers(&mut self) -> Vec<usize> {
        use self::FinishedStatus::*;
        use self::HandlerAsync::*;

        self.handlers
            .iter()
            .filter(|(id, handler)| {
                if let Finished(status) = handler.check_status() {
                    match status {
                        TimedOut => {
                            eprintln!("Connection {} timed out!", id);
                            return true;
                        }
                        Errored(_) => {
                            eprintln!("Connection {} errored!", id);
                            return true;
                        }
                        Panicked => {
                            eprintln!(
                                "Connection {}'s Handler panicked! This is definitely a bug!",
                                id
                            );
                            return true;
                        }
                        Terminated => unimplemented!(),
                    }
                }
                false
            })
            .map(|(&id, _)| id)
            .collect()

        // TODO: Add a message sender to handlers, make them send message when one
        // is read, then deal with that and broadcast to all connections here.

        // to_be_removed
    }

    fn check_and_broadcast(&mut self) {
        if let Ok(msg) = self.msg_recver.try_recv() {
            if msg.contents != "" {
                let msg_str = msg.to_string();
                println!("{}", msg_str);

                self.handlers.values_mut().for_each(|handler| {
                    let mut conn = handler
                        .connection
                        .lock()
                        .expect("Another thread panicked while holding a conn lock!");

                    conn.write_bytes(msg_str.as_bytes()).unwrap_or_else(|err| {
                        eprintln!(
                            "Could not send message to a Connection! This is most likely a bug. Error: {}",
                            err
                        );
                    });
                });
            }
        }
    }
}

struct Handler {
    status_recv: Receiver<FinishedStatus>,
    connection: Arc<Mutex<Connection>>,
    thread: thread::JoinHandle<()>,
}

impl Handler {
    fn accept(connection: Connection, msg_sender: Sender<Message>, timeout: Duration) -> Handler {
        let connection = Arc::new(Mutex::new(connection));
        let (status_send, status_recv) = mpsc::sync_channel(0);
        let max_attempts = timeout.as_millis();

        let thread_conn = Arc::clone(&connection);
        let thread = thread::spawn(move || {
            Handler::handle(thread_conn, status_send, msg_sender, max_attempts)
        });

        Handler {
            status_recv,
            connection,
            thread,
        }
    }

    fn handle(
        conn: Arc<Mutex<Connection>>,
        status_sender: SyncSender<FinishedStatus>,
        msg_sender: Sender<Message>,
        max_attempts: u128,
    ) {
        use self::FinishedStatus::*;

        let mut terminated = false;
        let mut attempts = 0u128;
        let mut buf = Vec::with_capacity(1024); // Just a default

        loop {
            thread::sleep(Duration::from_millis(1));
            let mut conn = conn.lock().unwrap_or_else(|err| {
                // Ideally, this should not happen. This is only used to
                // propagate the panic if things do go south.
                status_sender
                    .send(Panicked)
                    .expect("Everything is wrong...");
                panic!(
                    "Another thread panicked while getting conn lock! Error: {}",
                    err
                );
            });

            match conn.read_bytes(&mut buf) {
                Ok(_) => {
                    // The client responded! Reset the attempts.
                    attempts = 0;

                    let msg_contents = bytes_to_string(&buf);
                    let msg = Message::new(msg_contents, conn.id, None);
                    msg_sender.send(msg).expect("Could not send Message!");
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    attempts += 1;
                    if attempts == max_attempts {
                        status_sender
                            .send(TimedOut)
                            .expect("Could not send Timed out signal!");
                        break;
                    }
                }
                Err(e) => {
                    status_sender
                        .send(Errored(e.kind()))
                        .expect("Could not send Errored signal!");
                    break;
                }
            }

            buf.clear();
        }
    }

    fn check_status(&self) -> HandlerAsync {
        use self::HandlerAsync::*;

        match self.status_recv.try_recv() {
            Ok(status) => Finished(status),
            Err(e) if e == mpsc::TryRecvError::Empty => Working,
            Err(_) => panic!("Sender hung up! This should not happen."),
        }
    }
}

struct Connection {
    id: usize,
    stream: TcpStream,
}

impl Connection {
    fn new(stream: TcpStream, id: usize) -> io::Result<Connection> {
        stream.set_nonblocking(true)?;
        Ok(Connection { id, stream })
    }

    fn read_bytes(&mut self, buf: &mut Vec<u8>) -> io::Result<()> {
        // The first two bytes are expected to be the size of the message.
        // This means that a message can be at most 65535 characters long.
        // The most significant byte comes first, and the least significant
        // byte second.
        let mut len_bytes = [0; 2];
        self.stream
            .try_clone()?
            .take(2)
            .read_exact(&mut len_bytes)?;

        let len = ((len_bytes[0] as u16) << 8) + len_bytes[1] as u16;
        let mut msg = vec![0; len as usize].into_boxed_slice();

        self.stream.read(&mut msg)?;

        // To remind myself what this does: We must dereference the Box, to get the
        // [u8] slice, and then reference it again in order to create a &[u8], since
        // Rust's automatic deref coercion rules won't do this for you. The reason
        // we need to do this is because &Box<[u8]> does not implement IntoIterator,
        // but &[u8] does, and Rust won't just deref to some type that implements it.
        buf.extend(&*msg);

        Ok(())
    }

    fn write_bytes(&mut self, buf: &[u8]) -> io::Result<()> {
        // Somewhere to store the length bytes.
        let mut len_bytes = [0; 2];

        // We need to write the length of the message into a variable.
        // Since we know that the buf.len() <= 65535, we can safely cast
        // it to u16. As a sanity check, I'm using try_from() to make sure
        // that it can be safely cast.
        let msg_len =
            u16::try_from(buf.len()).expect("converting to u16 here should always be safe!");

        len_bytes[0] = (msg_len >> 8) as u8;
        len_bytes[1] = (msg_len & 255) as u8;

        let msg = [&len_bytes[..], &buf[..]].concat().into_boxed_slice();

        self.stream.write_all(&msg)?;
        self.stream.flush()?;

        Ok(())
    }
}

struct Message {
    contents: String,
    from: Id,
    to: Option<Id>,
}

impl Message {
    fn new(contents: String, from: Id, to: Option<Id>) -> Message {
        Message { contents, from, to }
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} -> {}", self.from, self.contents)
    }
}

fn bytes_to_string(buf: &[u8]) -> String {
    let s = String::from(String::from_utf8_lossy(buf).trim());
    s
}
