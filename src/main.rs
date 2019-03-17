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
        // linuxなのでバックエンドにはepollシステムコールを利用
        let poll = Poll::new().unwrap();

        // edgeトリガーモード：監視対象のEventdで変更が発生した場合にイベント配信する
        // レベルトリガー：変更がなくてもイベント？

        // イベントの種類：readyness: どの種類の操作を実行する準備ができているか　readable()は読み取り準備できていることを示す
        // &serverはEventdを実装している。状態変化するとpollの呼び出してredinessイベントを返す
        //TOKENでhandleを識別
        // 第3引数で指定したイベントだけを返す
        poll.register(&server, SERVER, Ready::readable(), PollOpt::edge()).unwrap();

        // イベントを詰め込む
        let mut events = Events::with_capacity(1024);

        loop {
            //現在のスレッドをブロックしてイベントを待つ。
            // 一回のpollに対して複数のeventが発行される場合がある。socketが読み書き可能になると1つの読み書き可能または読み可能と書き可能の2つ
            poll.poll(&mut events, None).unwrap();
            for event in &events {
                match event.token() {
                    SERVER => {
                        let (stream, remote) = server.accept().expect("Failed to accept connection");
                        println!("Connection from {}", &remote);

                        let token = Token(self.next_connection_id);
                        poll.register(&stream, token, Ready::readable(), PollOpt::edge());

                        if let Some(_) = self.connections.insert(self.next_connection_id, stream){
                            // HashMapは既存のキーで値が更新されると更新前の値を返す
                            panic!("Failed to register connection");
                        }
                        self.next_connection_id += 1;
                    }

                    Token(conn_id) => {
                        // let stream = match self.connections.get_mut(&conn_id) {
                        //     Some(stream) => stream,
                        //     _ => return Ok(()),
                        // };
                        if let Some(stream) = self.connections.get_mut(&conn_id) {
                            TCPServer::handle_connection(stream, &conn_id).expect("http server error");  //ここでselfへの&mutが作られるのでだめ
                            self.connections.remove(&conn_id);
                        }
                    }

                    _ => {
                        unreachable!();
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_connection(stream: &mut TcpStream, conn_id: &usize) -> Result<(), Error> {
        println!("conn_id: {}", conn_id);
        let mut buffer = [0u8; 512];
        let nbytes = stream.read(&mut buffer)?;
        if nbytes == 0 {
            return Ok(());
        }

        let http_pattern = Regex::new(r"(.*) (.*) HTTP/1.([0-1])").unwrap();
        let captures = http_pattern.captures(str::from_utf8(&buffer[..nbytes]).unwrap()).ok_or("invalid request").unwrap();

        let method = captures.get(1).unwrap().as_str();
        let path = &format!("{}{}{}", env::current_dir().unwrap().display(), WEBROOT, captures.get(2).unwrap().as_str());
        let _version = captures.get(3).unwrap().as_str();

        if method == "GET" {
            println!("path: {}", path);
            let file = match fs::File::open(path) {
                Ok(file) => file,
                // Error(Err) => {
                //     stream.write("HTTP/1.0 404 NOT FOUND\r\n".as_bytes())?;
                //     stream.write("Server: mio webserver\r\n\r\n".as_bytes())?;
                //     stream.flush()?;
                //     return Ok(());
                // }
                _ => {
                    return Ok(());
                }
            };
            let mut reader = BufReader::new(file);
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf)?;

            stream.write("HTTP/1.0 200 OK\r\n".as_bytes())?;
            stream.write("Server: mio webserver\r\n".as_bytes())?;
            stream.write("\r\n".as_bytes())?;
            stream.write(&buf)?;
            stream.flush()?;
        } else if method == "HEAD" {
        } else {
        }
        Ok(())
    }
}




fn main() {
    let args:Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Bad number of argments");
        std::process::exit(1);
    }
    let mut server = TCPServer::new(&args[1]);
    server.run();
}
