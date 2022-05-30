use std::error::Error;
use std::io::BufRead;
use std::io::Read;
use std::str::FromStr;

use crate::sem::*;

pub struct IoSource(pub Box<dyn BufRead>);

impl IoSource {
    pub fn next_atom(self: &mut Self, _ty: &AtomTy) -> Result<Option<i64>, Box<dyn Error>> {
        let val = self.next_token()?;

        Ok(if val.is_empty() {
            None
        } else {
            Some(i64::from_str(&String::from_utf8(val)?)?)
        })
    }

    pub fn next_token(self: &mut Self) -> Result<Vec<u8>, Box<dyn Error>> {
        let val = self
            .0
            .by_ref()
            .bytes()
            .skip_while(|b| match b {
                Ok(b) => b.is_ascii_whitespace(),
                _ => false,
            })
            .take_while(|b| match b {
                Ok(b) => !b.is_ascii_whitespace(),
                _ => false,
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(val)
    }

    pub fn check_eof(self: &mut Self) -> bool {
        self.next_token().unwrap().is_empty()
    }
}
