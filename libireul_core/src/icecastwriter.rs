use std::net::TcpStream;
use std::io::{self, Write};

use url;
use rustc_serialize::base64::{ToBase64, MIME};

use ogg::OggPage;

// TODO: Move mountpoint information into some serializable thing
// https://gist.github.com/ePirat/adc3b8ba00d85b7e3870#specifying-mountpoint-information
#[derive(RustcDecodable, Clone, Debug)]
pub struct IceCastWriterOptions {
    public: bool,
    name: Option<String>,
    description: Option<String>,
    url: Option<String>,
    genre: Option<String>,
}

impl IceCastWriterOptions {
    pub fn set_name(&mut self, name: &str) -> &mut Self {
        self.name = Some(name.to_string());
        self
    }

    pub fn set_description(&mut self, description: &str) -> &mut Self {
        self.description = Some(description.to_string());
        self
    }

    pub fn set_url(&mut self, url: &str) -> &mut Self {
        self.url = Some(url.to_string());
        self
    }

    pub fn set_genre(&mut self, genre: &str) -> &mut Self {
        self.genre = Some(genre.to_string());
        self
    }
}

impl Default for IceCastWriterOptions {
    fn default() -> IceCastWriterOptions {
        IceCastWriterOptions {
            public: false,
            name: None,
            description: None,
            url: None,
            genre: None,
        }
    }
}

pub struct IceCastWriter {
    stream: TcpStream,
    options: IceCastWriterOptions,
}


impl IceCastWriter {
    #[allow(dead_code)]
    pub fn new(url: &url::Url) -> io::Result<IceCastWriter> {
        IceCastWriter::with_options(url, IceCastWriterOptions::default())
    }

    pub fn with_options(url: &url::Url, opts: IceCastWriterOptions) -> io::Result<IceCastWriter> {
        let endpoint = try!(get_endpoint(url).ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Missing hostname in URL")
        }));

        let stream = try!(TcpStream::connect(&endpoint[..]));
        let mut writer = IceCastWriter {
            stream: stream,
            options: opts
        };
        try!(writer.send_header(url));
        Ok(writer)
    }

    fn send_header(&mut self, url: &url::Url) -> io::Result<()> {
        let options = &self.options;

        let mount = try!(url.serialize_path().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Missing path in URL")
        }));
        try!(write!(self.stream, "SOURCE {} HTTP/1.0\r\n", mount));

        if let Some(userpass) = get_authorization_header(url) {
            try!(write!(self.stream, "{}\r\n", userpass));
        }
        if let Some(host) = get_host_header(url) {
            // TODO: Verify this
            try!(write!(self.stream, "{}\r\n", host));
        }

        try!(write!(self.stream, "Accept: */*"));
        try!(write!(self.stream, "User-Agent: ireul\r\n"));

        if options.public {
            try!(write!(self.stream, "Ice-Public: 1\r\n"));
        }

        if let Some(ref name) = options.name {
            try!(write!(self.stream, "Ice-Name: {}\r\n", name));
        }

        if let Some(ref description) = options.description {
            try!(write!(self.stream, "Ice-Description: {}\r\n", description));
        }

        if let Some(ref url) = options.url {
            try!(write!(self.stream, "Ice-Url: {}\r\n", url));
        }

        if let Some(ref genre) = options.genre {
            try!(write!(self.stream, "Ice-Genre: {}\r\n", genre));
        }

        // Do not reorder Content-Type! Somehow IceCast treated it as audio/mpeg when moved up
        try!(write!(self.stream, "Content-Type: audio/ogg\r\n"));

        // IceCast responds with some headers and a HTTP OK, but they're not parsed yet
        self.stream.write_all(b"\r\n")
    }

    pub fn send_ogg_page(&mut self, page: &OggPage) -> io::Result<()> {
        self.stream.write_all(page.as_u8_slice())
    }
}

fn get_endpoint(url: &url::Url) -> Option<String> {
    match (url.domain(), url.port()) {
        (None, _) => None,
        (Some(domain), None) => {
            Some(format!("{}:8000", domain))
        }
        (Some(domain), Some(port)) => {
            Some(format!("{}:{}", domain, port))
        }
    }
}

fn get_host_header(url: &url::Url) -> Option<String> {
    match get_endpoint(url) {
        None => None,
        Some(endpoint) => Some(format!("Host: {}", endpoint)),
    }
}

fn get_authorization_header(url: &url::Url) -> Option<String> {
    let mut emit = false;
    let mut userpass = String::new();

    if let Some(user) = url.username() {
        emit = true;
        userpass.push_str(user);
    }
    if let Some(password) = url.password() {
        emit = true;
        userpass.push_str(":");
        userpass.push_str(password);
    }

    let mut config = MIME;
    config.line_length = None;
    let userpass_b64 = userpass.as_bytes().to_base64(config);

    match emit {
        true => Some(format!("Authorization: Basic {}", userpass_b64)),
        false => None,
    }
}
