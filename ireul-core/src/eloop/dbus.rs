use std::thread;
use std::fs::File;
use std::sync::{Arc, Mutex};
use std::io::Read;
use std::os::unix::io::FromRawFd;
use std::ffi::CString;

use dbus::{self, Message, Connection, BusType, NameFlag, OwnedFd, MessageItem, FromMessageItem};
use dbus::tree::{Method, MethodFn, Factory};
use time::SteadyTime;

use ogg::{OggTrackBuf};
use ireul_rpc::proxy::track::model::Handle;
use ireul_rpc::proxy::track::{
    EnqueueTrackRequest,
    EnqueueTrackError,
};

use libireul_core::Core;

pub fn start(core: Arc<Mutex<Core>>) {
    thread::spawn(move || start_helper(core));
}

fn start_helper(core: Arc<Mutex<Core>>) {

    let bus = Connection::get_private(BusType::Session).unwrap();
    bus.register_name("org.yasashiisyndicate.ireul", NameFlag::ReplaceExisting as u32).unwrap();

    let f = Factory::new_fn();

    let core_interface = f.interface("org.yasashiisyndicate.ireul_v0.Core")
        .add_m(new_enqueue_file_method(&f, core.clone()));

    let tree = f.tree().add(
        f.object_path("/org/yasashiisyndicate/ireul_v0")
            .introspectable()
            .add(core_interface));

    tree.set_registered(&bus, true).unwrap();
    for _ in tree.run(&bus, bus.iter(1000)) {
        //
    }
}


const DBUS_INVALID_ARGS: &'static str = "org.freedesktop.DBus.Error.InvalidArgs";
const ENQUEUE_TRACK_ERROR_NAME: &'static str = "org.yasashiisyndicate.ireul.EnqueueTrackError";


fn adapt_enqueue_track_error(m: &Message, err: &EnqueueTrackError) -> dbus::Message {
    let err_name = dbus::ErrorName::new(ENQUEUE_TRACK_ERROR_NAME).unwrap();
    let message = CString::new(format!("{:?}", err)).unwrap();

    m.error(&err_name, &message).append1(err.to_u32())
}


fn new_enqueue_file_method(f: &Factory<MethodFn<'static>>, core: Arc<Mutex<Core>>) -> Method<MethodFn<'static>> {
    let in_sig: &[(&str, &str)] = &[
        ("track", "h"),
        ("metadata", "a(ss)"),
    ];
    let out_sig: &[(&str, &str)] = &[
        ("handle", "t"),
    ];

    f.method("EnqueueFile", move |m, _, _| {
        let req_params = m.get_items();
        println!("req_params = {:?}", req_params);

        let enqueue_req = match adapt_enqueue_track_req(&m, &req_params) {
            Ok(req) => req,
            Err(msg) => return Ok(vec![msg]),
        };

        let resp: Result<Handle, _> = {
            let mut exc_core = core.lock().unwrap();
            exc_core.enqueue_track(enqueue_req)
        };
        match resp {
            Ok(res) => {
                Ok(vec![m.method_return().append1(res.0)])
            },
            Err(err) => {
                Ok(vec![adapt_enqueue_track_error(&m, &err)])
            }
        }
    })
    .in_args(in_sig.iter().cloned())
    .out_args(out_sig.iter().cloned())
}


fn new_fast_forward_method(f: &Factory<MethodFn<'static>>, core: Arc<Mutex<Core>>) -> Method<MethodFn<'static>> {
    unimplemented!();
}


fn new_status_method(f: &Factory<MethodFn<'static>>, core: Arc<Mutex<Core>>) -> Method<MethodFn<'static>> {
    unimplemented!();
}


fn new_replace_fallback_method(f: &Factory<MethodFn<'static>>, core: Arc<Mutex<Core>>) -> Method<MethodFn<'static>> {
    unimplemented!();
}


fn adapt_enqueue_track_req(m: &Message, items: &[MessageItem]) -> Result<EnqueueTrackRequest, Message> {
    let arg_err_name = dbus::ErrorName::new(DBUS_INVALID_ARGS).unwrap();

    if items.len() == 0 {
        let msg_text = CString::new("Invalid argument: not enough arguments").unwrap();
        return Err(m.error(&arg_err_name, &msg_text));
    }

    let track_fd: OwnedFd = try!(FromMessageItem::from(&items[0])
        .map(|x: &OwnedFd| x.clone())
        .map_err(|()| {
            let msg_text = CString::new("Invalid argument: first argument must be file").unwrap();
            m.error(&arg_err_name, &msg_text)
        }));

    if 1 < items.len() {
        println!("metadata = {:?}", items[1]);
    }

    let mut file: File = unsafe { File::from_raw_fd(track_fd.into_fd()) };

    let mut buffer: Vec<u8> = Vec::new();
    file.read_to_end(&mut buffer).unwrap();

    let track = OggTrackBuf::new(buffer).unwrap();

    Ok(EnqueueTrackRequest {
        track: track,
        metadata: None,
    })
}

