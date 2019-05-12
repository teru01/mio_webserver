use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::net::SocketAddr;
use std::{env, str};

use mio::tcp::{TcpListener, TcpStream};
use mio::*;
use regex::Regex;
#[macro_use]
extern crate log;

const SERVER: Token = Token(0);
const WEBROOT: &str = "/webroot";

struct WebServer {
    address: SocketAddr,
    connections: HashMap<usize, TcpStream>,
    next_connection_id: usize,
}

impl WebServer {
    /**
     * サーバの初期化
     */
    fn new(addr: &str) -> Result<Self, failure::Error> {
        let address = addr.parse()?;
        Ok(WebServer {
            address,
            connections: HashMap::new(),
            next_connection_id: 1,
        })
    }

    /**
     * サーバを起動する。
     */
    fn run(&mut self) -> Result<(), failure::Error> {
        let server = TcpListener::bind(&self.address)?;
        let poll = Poll::new()?;

        poll.register(&server, SERVER, Ready::readable(), PollOpt::edge())?;

        let mut events = Events::with_capacity(1024);
        let mut response = Vec::new();

        // イベントループ
        loop {
            //現在のスレッドをブロックしてイベントを待つ。
            poll.poll(&mut events, None).unwrap();
            for event in &events {
                match event.token() {
                    SERVER => {
                        // コネクションの確立要求を処理
                        self.connection_handler(&server, &poll)?;
                    }

                    Token(conn_id) => {
                        // コネクションを使ってパケットを送受信する
                        self.http_handler(conn_id, event, &poll, &mut response)?;
                    }
                }
            }
        }
    }

    /**
     * コネクション確立要求の処理
     */
    fn connection_handler(
        &mut self,
        server: &TcpListener,
        poll: &Poll,
    ) -> Result<(), failure::Error> {
        let (stream, remote) = server.accept()?;
        debug!("Connection from {}", &remote);

        let token = Token(self.next_connection_id);
        poll.register(&stream, token, Ready::readable(), PollOpt::edge())?;

        if let Some(_) = self.connections.insert(self.next_connection_id, stream) {
            // HashMapは既存のキーで値が更新されると更新前の値を返す
            panic!("Connection ID is already exist.");
        }
        self.next_connection_id += 1;
        Ok(())
    }

    /**
     * httpリクエスト・レスポンスの処理
     */
    fn http_handler(
        &mut self,
        conn_id: usize,
        event: Event,
        poll: &Poll,
        response: &mut Vec<u8>,
    ) -> Result<(), failure::Error> {
        let stream = self
            .connections
            .get_mut(&conn_id)
            .ok_or_else(|| failure::err_msg("Failed to get connection."))?;
        if event.readiness().is_readable() {
            // ソケットから読み込み可能。
            debug!("conn_id: {}", conn_id);
            let mut buffer = [0u8; 512];
            let nbytes = stream.read(&mut buffer)?;

            if nbytes != 0 {
                *response = WebServer::make_response(&buffer, nbytes).unwrap();
                // 書き込み可能状態を監視対象に入れる
                poll.reregister(stream, Token(conn_id), Ready::writable(), PollOpt::edge())?;
            } else {
                // 通信終了
                self.connections.remove(&conn_id);
            }
            Ok(())
        } else if event.readiness().is_writable() {
            // ソケットに書き込み可能。
            stream.write_all(response)?;
            self.connections.remove(&conn_id);
            Ok(())
        } else {
            Err(failure::err_msg("undefined event."))
        }
    }

    /**
     * レスポンスをバイト列で作成します。
     */
    fn make_response(buffer: &[u8], nbytes: usize) -> Result<Vec<u8>, failure::Error> {
        let http_pattern = Regex::new(r"(.*) (.*) HTTP/1.([0-1])\r\n.*")?;
        let captures = match http_pattern.captures(str::from_utf8(&buffer[..nbytes])?) {
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
        let path = &format!(
            "{}{}{}",
            env::current_dir()?.display(),
            WEBROOT,
            captures.get(2).unwrap().as_str()
        );
        let _version = captures.get(3).unwrap().as_str();

        if method == "GET" {
            let file = match File::open(path) {
                Ok(file) => file,
                Err(_) => {
                    // パーミッションエラーなどもここに含まれるが、
                    // 簡略化のためnot foundにしている
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
        } else {
            // サポートしていないHTTPメソッド
            let mut response = Vec::new();
            response.append(&mut "HTTP/1.0 501 Not Implemented\r\n".to_string().into_bytes());
            response.append(&mut "Server: mio webserver\r\n".to_string().into_bytes());
            response.append(&mut "\r\n".to_string().into_bytes());
            return Ok(response);
        }
    }
}

fn main() {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Bad number of argments");
        std::process::exit(1);
    }
    let mut server = WebServer::new(&args[1]).unwrap_or_else(|e| {
        error!("{}", e);
        panic!();
    });
    server.run().unwrap_or_else(|e| {
        error!("{}", e);
        panic!();
    });
}
