use std::io;
use toml::de;

pub type BR<T> = Result<T, BbkarError>;

#[derive(Debug)]
pub enum BbkarError {
    IO(io::Error),
    Toml(de::Error),
}
