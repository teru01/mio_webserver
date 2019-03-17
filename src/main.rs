extern crate mio;
extern crate regex;

use std::net::SocketAddr;
use std::{ env, str, fs };
use std::io::{ Error, Read, BufReader, Write };
use std::collections::HashMap;

use mio::*;
use mio::tcp::{ TcpListener, TcpStream };
use regex::Regex;


const SERVER: Token = Token(0);
const WEBROOT: &str = "/webroot";

struct TCPServer {
    address: SocketAddr,
    connections: HashMap<usize, TcpStream>,
    next_connection_id: usize
}

impl TCPServer {
    fn new(addr: &str) -> Self {
        let address = addr.parse().unwrap();
        TCPServer {
            address,
            connections: HashMap::new(),
            next_connection_id: 1
        }
    }

    fn run(&mut self) -> Result<(), Error> {
        let server = TcpListener::bind(&self.address).expect("Failed to bind address");
        let poll = Poll::new().unwrap();

        poll.register(&server, SERVER, Ready::readable(), PollOpt::edge()).unwrap();

        let mut events = Events::with_capacity(1024);

        // イベントループ
        loop {
            //現在のスレッドをブロックしてイベントを待つ。
            poll.poll(&mut events, None).unwrap();
            for event in &events {
                match event.token() {
                    SERVER => {
                        let (stream, remote) = server.accept().expect("Failed to accept connection");
                        println!("Connection from {}", &remote);

                        let token = Token(self.next_connection_id);
                        poll.register(&stream, token, Ready::readable(), PollOpt::edge()).unwrap();

                        if let Some(_) = self.connections.insert(self.next_connection_id, stream){
                            // HashMapは既存のキーで値が更新されると更新前の値を返す
                            panic!("Failed to register connection");
                        }
                        self.next_connection_id += 1;
                    }

                    Token(conn_id) => {
                        if let Some(stream) = self.connections.get_mut(&conn_id) {
                            println!("conn_id: {}", conn_id);
                            let mut buffer = [0u8; 512];
                            let nbytes = stream.read(&mut buffer)?;

                            if nbytes != 0 {
                                let response = TCPServer::make_response(&buffer, &nbytes).unwrap();
                                stream.write(&response)?;
                                stream.flush()?;
                            }
                            self.connections.remove(&conn_id);
                        }
                    }
                }
            }
        }
    }

    fn make_response(buffer: &[u8], nbytes: &usize) -> Result<Vec<u8>, Error> {
        let http_pattern = Regex::new(r"(.*) (.*) HTTP/1.([0-1])").unwrap();
        let captures = match http_pattern.captures(str::from_utf8(&buffer[..*nbytes]).unwrap()) {
            Some(cap) => cap,
            None => {
                let mut response = Vec::new();
                response.append(&mut "HTTP/1.0 400 Bad Request\r\n".to_string().into_bytes());
                response.append(&mut "Server: mio webserver\r\n".to_string().into_bytes());
                response.append(&mut "\r\n".to_string().into_bytes());
                return Ok(response);
            }
        };

        let method = captures.get(1).unwrap().as_str();
        let path = &format!("{}{}{}", env::current_dir().unwrap().display(), WEBROOT, captures.get(2).unwrap().as_str());
        let _version = captures.get(3).unwrap().as_str();

        if method == "GET" {
            let file = match fs::File::open(path) {
                Ok(file) => file,
                Err(_) => {
                    // パーミッションエラーなどもここに含まれるが手抜きしてnot foundにしている
                    let mut response = Vec::new();
                    response.append(&mut "HTTP/1.0 404 Not Found\r\n".to_string().into_bytes());
                    response.append(&mut "Server: mio webserver\r\n\r\n".to_string().into_bytes());
                    return Ok(response);
                }
            };
            let mut reader = BufReader::new(file);
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf)?;

            let mut response = Vec::new();
            response.append(&mut "HTTP/1.0 200 OK\r\n".to_string().into_bytes());
            response.append(&mut "Server: mio webserver\r\n".to_string().into_bytes());
            response.append(&mut "\r\n".to_string().into_bytes());
            response.append(&mut buf);
            return Ok(response);
        }
        // サポートしていないHTTPメソッド
        let mut response = Vec::new();
        response.append(&mut "HTTP/1.0 501 Not Implemented\r\n".to_string().into_bytes());
        response.append(&mut "Server: mio webserver\r\n".to_string().into_bytes());
        response.append(&mut "\r\n".to_string().into_bytes());
        return Ok(response);
    }
}

fn main() {
    let args:Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Bad number of argments");
        std::process::exit(1);
    }
    let mut server = TCPServer::new(&args[1]);
    server.run().expect("Internal Server Error.");
}
