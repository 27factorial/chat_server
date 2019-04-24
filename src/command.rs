extern crate hashbrown;
use crate::server;

use self::CommandError::*;

use hashbrown::HashMap;

use server::Connection;
use server::Id;
use server::Message;

type CmdResult<T> = Result<T, CommandError>;

pub enum CommandError {
    NoSuchCommand,
}

pub trait Command {
    fn execute(&self, from: &Connection, _: Args) -> CmdResponse;
}

pub struct CommandHandler {
    pub prefix: &'static str,
    commands: HashMap<&'static str, Box<dyn Command>>,
}

impl CommandHandler {
    pub fn new(prefix: &'static str) -> CommandHandler {
        let commands = HashMap::new();

        CommandHandler { prefix, commands }
    }

    pub fn register(&mut self, name: &'static str, command: Box<dyn Command>) {
        self.commands.insert(name, command);
    }

    pub fn exec(&self, msg: &str, connection: &Connection) -> CmdResult<CmdResponse> {
        let name = msg.chars().skip(1).collect::<String>();
        let name = name.as_str();

        let args = Args::new(msg);

        match self.commands.get(name) {
            Some(cmd) => Ok(cmd.execute(connection, args)),
            None => Err(NoSuchCommand),
        }
    }
}

pub struct CmdResponse {
    pub to: Id,
    pub msg: String,
}

impl CmdResponse {
    pub fn new(to: Id, msg: String) -> CmdResponse {
        CmdResponse { to, msg }
    }
}

pub struct Args<'a> {
    args: Vec<&'a str>,
}

impl<'a> Args<'a> {
    fn new(message: &'a str) -> Args<'a> {
        let args = message.split_whitespace().skip(1).collect();

        Args { args }
    }
}
