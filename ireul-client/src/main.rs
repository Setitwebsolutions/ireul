extern crate ogg;
extern crate ogg_clock;
extern crate ireul_rpc;
extern crate byteorder;

use std::ffi::OsString;
use std::env;
use std::process;

mod enqueue;
mod fastforward;
mod entrypoint;
mod queue;
mod replace_fallback;

use entrypoint::EntryPoint;

static ENTRY_POINT_MAP: &'static [(&'static str, &'static EntryPoint)] = &[
    ("enqueue", &enqueue::EntryPoint),
    ("fast-forward", &fastforward::EntryPoint),
    ("queue", &queue::EntryPoint),
    ("replace-fallback", &replace_fallback::EntryPoint),
];

fn print_usage(args: &[OsString]) {
    for &(_key, val) in ENTRY_POINT_MAP.iter() {
        val.print_usage(args);
    }
}

fn get_entry_point(name: &str) -> Option<&'static EntryPoint> {
    for &(key, val) in ENTRY_POINT_MAP.iter() {
        if key == name {
            return Some(val);
        }
    }
    None
}

fn main() {
    let args: Vec<OsString> = env::args_os().collect();
    let sub_command = args[1].clone().into_string().ok().unwrap();

    let help_args = args.clone();
    if let Some(entry_pt) = get_entry_point(&sub_command) {
        match entry_pt.main(args) {
            Ok(()) => (),
            Err(entrypoint::Error::Unspecified(msg)) => {
                println!("subcommand failed: {}", msg);
                process::exit(1);
            },
            Err(entrypoint::Error::InvalidArguments) => {
                entry_pt.print_usage(&help_args);
                process::exit(1);
            },
        }
        return;
    }

    print_usage(&args);
    process::exit(1);
}
