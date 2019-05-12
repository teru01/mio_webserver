use mio::tcp::{TcpListener, TcpStream};
use mio::*;
use regex::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::process;
use std::{env, str};
#[macro_use]
extern crate log;

const SERVER: Token = Token(0);
const WEBROOT: &str = "/webroot";

struct WebServer {
    server_socket: TcpListener,
    connections: HashMap<usize, TcpStream>,
    next_connection_id: usize,
}

impl WebServer {
    /**
     * サーバの初期化
     */
    fn new(addr: &str) -> Result<Self, failure::Error> {
        let address = addr.parse()?;
        let server_socket = TcpListener::bind(&address)?;
        Ok(WebServer {
            server_socket,
            connections: HashMap::new(),
            next_connection_id: 1,
        })
    }

    /**
     * サーバを起動する。
     */
    fn run(&mut self) -> Result<(), failure::Error> {
        let poll = Poll::new()?;
        // サーバーソケットの状態を監視対象に登録する。
        poll.register(
            &self.server_socket,
            SERVER,
            Ready::readable(),
            PollOpt::edge(),
        )?;

        let mut events = Events::with_capacity(1024);
        let mut response = Vec::new();

        // イベントループ
        loop {
            // 現在のスレッドをブロックしてイベントを待つ。
            poll.poll(&mut events, None).unwrap();
            for event in &events {
                match event.token() {
                    SERVER => {
                        // コネクションの確立要求を処理
                        let (stream, remote) = self.server_socket.accept()?;
                        debug!("Connection from {}", &remote);
                        // コネクションを監視対象に登録
                        self.register_connection(&poll, stream)?;
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
    fn register_connection(
        &mut self,
        poll: &Poll,
        stream: TcpStream,
    ) -> Result<(), failure::Error> {
        let token = Token(self.next_connection_id);
        poll.register(&stream, token, Ready::readable(), PollOpt::edge())?;

        if self.connections.insert(self.next_connection_id, stream).is_some() {
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
                *response = WebServer::make_response(&buffer[..nbytes])?;
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
            Err(failure::err_msg("Undefined event."))
        }
    }

    /**
     * HTTPステータスコードからメッセージを生成する。
     */
    fn create_msg_from_code(
        status_code: u16,
        msg: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, failure::Error> {
        match status_code {
            200 => {
                let mut header = "HTTP/1.0 200 OK\r\n\
                                  Server: mio webserver\r\n\r\n"
                    .to_string()
                    .into_bytes();
                if let Some(mut msg) = msg {
                    header.append(&mut msg);
                }
                Ok(header)
            }
            400 => Ok("HTTP/1.0 400 Bad Request\r\n\
                       Server: mio webserver\r\n\r\n"
                .to_string()
                .into_bytes()),
            404 => Ok("HTTP/1.0 404 Not Found\r\n\
                       Server: mio webserver\r\n\r\n"
                .to_string()
                .into_bytes()),
            501 => Ok("HTTP/1.0 501 Not Implemented\r\n\
                       Server: mio webserver\r\n\r\n"
                .to_string()
                .into_bytes()),
            _ => Err(failure::err_msg("Undefined status code.")),
        }
    }

    /**
     * レスポンスをバイト列で作成して返す
     */
    fn make_response(buffer: &[u8]) -> Result<Vec<u8>, failure::Error> {
        let http_pattern = Regex::new(r"(.*) (.*) HTTP/1.([0-1])\r\n.*")?;
        let captures = match http_pattern.captures(str::from_utf8(buffer)?) {
            Some(cap) => cap,
            None => {
                return WebServer::create_msg_from_code(200, None);
            }
        };

        let method = captures[1].to_string();
        let path = format!(
            "{}{}{}",
            env::current_dir()?.display(),
            WEBROOT,
            &captures[2]
        );
        let _version = captures[3].to_string();

        if method == "GET" {
            let file = match File::open(path) {
                Ok(file) => file,
                Err(_) => {
                    // パーミッションエラーなどもここに含まれるが、
                    // 簡略化のためnot foundにしている
                    return WebServer::create_msg_from_code(404, None);
                }
            };
            let mut reader = BufReader::new(file);
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf)?;
            WebServer::create_msg_from_code(200, Some(buf))
        } else {
            // サポートしていないHTTPメソッド
            WebServer::create_msg_from_code(501, None)
        }
    }
}

fn main() {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        error!("Bad number of argments.");
        process::exit(1);
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
