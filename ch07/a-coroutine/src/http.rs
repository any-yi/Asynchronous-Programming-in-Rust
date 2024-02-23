use crate::{future::PollState, Future};
use std::io::{ErrorKind, Read, Write};

fn get_req(path: &str) -> String {
    format!(
        "GET {path} HTTP/1.1\r\n\
             Host: localhost\r\n\
             Connection: close\r\n\
             \r\n"
    )
}

pub struct Http;

impl Http {
    pub fn get(path: &str) -> impl Future<Output = String> {
        // 这一步不但创建了一个 HttpGetFuture 对象，同时也创建了一个 Future 对象
        HttpGetFuture::new(path)
    }
}

struct HttpGetFuture {
    stream: Option<mio::net::TcpStream>,  // 第 4 章中对网络流的读取事件感兴趣，这里就记录一下那个网络流。
    buffer: Vec<u8>,                      // 保存服务器 HTTP 的响应。
    path: String,                         // HTTP GET 请求的路径：服务器延迟多少、回响（echo）的字符串是什么。
}

impl HttpGetFuture {
    fn new(path: &str) -> Self {
        Self {
            stream: None,
            buffer: vec![],
            path: path.to_string(),
        }
    }

    /// 该方法是为了懒汉式 Future 所创建，
    /// 为的是在创建 Future（HttpGetFuture） 对象的时候，还没有实际进行操作（发送 HTTP 请求），
    /// 等到 poll 的时候再进行实际操作。
    ///
    /// 每次遇到协程实现时，您都应该找出它们是懒汉式还是饿汉式，因为这会影响您使用它们进行编程的方式。
    /// 本 http.rs 里的 Future 是叶子 Future ，而非叶子 Future （async/await）总会被重写成懒汉式状态机。
    /// 由于重写方式是固定的，且需要叶子 Future 的配合，因此叶子 Future 也必须是懒汉式的，否则会有副作用。
    fn write_request(&mut self) {
        // 构建网络流对象
        let stream = std::net::TcpStream::connect("127.0.0.1:8080").unwrap();
        stream.set_nonblocking(true).unwrap();
        let mut stream = mio::net::TcpStream::from_std(stream);
        // 发送 HTTP 请求，path 是本对象内部保存的，最初是由 Http 对象的 get 方法传入的。
        stream.write_all(get_req(&self.path).as_bytes()).unwrap();
        // 保存发送了请求的网络流对象到本对象内部
        self.stream = Some(stream);
    }
}

impl Future for HttpGetFuture {
    type Output = String;

    fn poll(&mut self) -> PollState<Self::Output> {
        // 如果 stream 还没有发送请求并被保存
        if self.stream.is_none() {
            println!("FIRST POLL - START OPERATION");
            self.write_request();
            return PollState::NotReady;
        }

        // 保存 HTTP 请求的响应，开始填充本对象内部的 buffer 字段
        let mut buff = vec![0u8; 4096];
        // 循环读取是为了把数据读空而不是用以轮询数据到了没有
        loop {
            match self.stream.as_mut().unwrap().read(&mut buff) {
                // read 返回 0 表示读到了流的末尾
                Ok(0) => {
                    let s = String::from_utf8_lossy(&self.buffer);
                    // 保存 HTTP 响应的字符串并通过 Ready 枚举变量返回给 poll 方法的调用者。
                    // 这里的 break 其实就是 return 的意思。
                    break PollState::Ready(s.to_string());
                }
                Ok(n) => {
                    self.buffer.extend(&buff[0..n]);
                    continue;
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    break PollState::NotReady;
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {
                    continue;
                }
                Err(e) => panic!("{e:?}"),
            }
        }
    }
}
