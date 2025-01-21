use crate::Result;
use clap::Parser;
use xshell::Shell;

#[derive(Debug, Parser)]
pub enum Args {
    Guide(crate::guide::Guide),
    Build(crate::build::Build),
    Changelog(crate::changelog::Changelog),
    Checks(crate::checks::Checks),
    Publish(crate::publish::Publish),
    Test(crate::tests::Tests),
}

impl Args {
    pub fn run(&self, sh: &Shell) -> Result {
        match self {
            Args::Guide(args) => args.run(sh),
            Args::Build(args) => args.run(sh).map(|_| ()),
            Args::Changelog(args) => args.run(sh),
            Args::Checks(args) => args.run(sh),
            Args::Publish(args) => args.run(sh),
            Args::Test(args) => args.run(sh),
        }
    }
}

pub trait FlagExt {
    fn is_enabled(&self, default: bool) -> bool;
}

impl FlagExt for Option<bool> {
    fn is_enabled(&self, default: bool) -> bool {
        match self {
            Some(v) => *v,
            None => default,
        }
    }
}

/// Allows for argument flexibility
/// * `duvet` -> default
/// * `duvet --foo` -> true
/// * `duvet --foo=true` -> true
/// * `duvet --foo=false` -> false
impl FlagExt for Option<Option<bool>> {
    fn is_enabled(&self, default: bool) -> bool {
        match self {
            Some(Some(v)) => *v,
            Some(None) => true,
            None => default,
        }
    }
}
