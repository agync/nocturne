//! The Nocturne mind's words and judgement, whoever runs the model: the prompts each job is
//! given, the requests a reading makes, the reading of a reply into memories, the guards a reply
//! must pass, the receipts an answer cites and the template of a question asked online. Nothing
//! here starts a process or reaches a network, so a phone builds it as the desktop does.

pub mod cite;
pub mod decide;
pub mod guard;
pub mod judge;
pub mod online;
pub mod prompts;
pub mod read;
pub mod reading;
pub mod reflect;
pub mod temperature;
pub mod watch;
