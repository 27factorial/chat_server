#![allow(unused)]

extern crate hashbrown;
use crate::server;

use self::CommandError::*;

use hashbrown::HashMap;

use server::Connection;
use server::Id;
use server::Message;

#[macro_export]
macro_rules! command {
    ($f:ident() $b:block) => {
        #[allow(non_camel_case_types)]
        struct $f;

        impl crate::command::Command for $f {
            fn execute(
                &self,
                _: &crate::server::Message,
                _: crate::command::Args,
            ) -> crate::command::CmdResponse {
                crate::command::CmdResponse::from_tuple($b)
            }
        }
    };
    ($f:ident($m:ident) $b:block) => {
        #[allow(non_camel_case_types)]
        struct $f;

        impl crate::command::Command for $f {
            fn execute(
                &self,
                $m: &crate::server::Message,
                _: crate::command::Args,
            ) -> crate::command::CmdResponse {
                crate::command::CmdResponse::from_tuple($b)
            }
        }
    };
    ($f:ident($m:ident, $a:ident) $b:block) => {
        #[allow(non_camel_case_types)]
        struct $f;

        impl crate::command::Command for $f {
            fn execute(
                &self,
                $m: &crate::server::Message,
                $a: crate::command::Args,
            ) -> crate::command::CmdResponse {
                crate::command::CmdResponse::from_tuple($b)
            }
        }
    };
}

type CmdResult<T> = Result<T, CommandError>;

pub enum CommandError {
    NoSuchCommand,
}

pub trait Command {
    fn execute(&self, _: &Message, _: Args) -> CmdResponse;
}

pub struct CommandHandler {
    pub prefix: char,
    commands: HashMap<&'static str, Box<dyn Command>>,
}

impl CommandHandler {
    pub fn new(prefix: char) -> CommandHandler {
        let commands = HashMap::new();

        CommandHandler { prefix, commands }
    }

    fn cmd_from_message(message: &Message) -> String {
        message
            .contents
            .chars()
            .skip(1)
            .take_while(|&c| c != ' ')
            .collect::<String>()
    }

    pub fn register(&mut self, name: &'static str, command: Box<dyn Command>) {
        self.commands.insert(name, command);
    }

    pub fn exec(&self, msg: &Message) -> CmdResult<CmdResponse> {
        let name = Self::cmd_from_message(msg);

        let args = Args::new(msg);

        match self.commands.get(name.as_str()) {
            Some(cmd) => Ok(cmd.execute(msg, args)),
            None => Err(NoSuchCommand),
        }
    }
}

pub struct CmdResponse {
    pub to: Id,
    pub msg: String,
}

impl CmdResponse {
    #[allow(unused)]
    pub fn new(to: Id, msg: String) -> CmdResponse {
        CmdResponse { to, msg }
    }

    pub fn from_tuple((to, msg): (Id, String)) -> CmdResponse {
        CmdResponse { to, msg }
    }
}

pub struct Args<'a> {
    pub count: usize,
    args: Vec<&'a str>,
}

impl<'a> Args<'a> {
    fn new(msg: &Message) -> Args<'_> {
        let args: Vec<&'_ str> = msg.contents.split_ascii_whitespace().skip(1).collect();
        let count = args.len();

        Args { count, args }
    }

    pub fn get(&self, i: usize) -> Option<&str> {
        match self.args.get(i) {
            Some(s) => Some(*s),
            None => None
        }
    }
}
