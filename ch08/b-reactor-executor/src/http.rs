use crate::{
    future::PollState,
    runtime::{self, reactor, Waker},
    Future,
};
use mio::Interest;
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
        HttpGetFuture::new(path.to_string())
    }
}
struct HttpGetFuture {
    stream: Option<mio::net::TcpStream>,
    buffer: Vec<u8>,
    path: String,
    // 添加了一个 id 字段，以标识该流。
    // 该字段由反应器的 next_id 字段初始化，next_id 从 1 开始编号
    id: usize,
}

impl HttpGetFuture {
    fn new(path: String) -> Self {
        let id = reactor().next_id();
        Self {
            stream: None,
            buffer: vec![],
            path,
            id,
        }
    }

    fn write_request(&mut self) {
        let stream = std::net::TcpStream::connect("127.0.0.1:8080").unwrap();
        stream.set_nonblocking(true).unwrap();
        let mut stream = mio::net::TcpStream::from_std(stream);
        stream.write_all(get_req(&self.path).as_bytes()).unwrap();
        self.stream = Some(stream);
    }
}

impl Future for HttpGetFuture {
    type Output = String;

    fn poll(&mut self, waker: &Waker) -> PollState<Self::Output> {
        // If this is first time polled, start the operation
        // see: https://users.rust-lang.org/t/is-it-bad-behaviour-for-a-future-or-stream-to-do-something-before-being-polled/61353
        // Avoid dns lookup this time
        if self.stream.is_none() {
            println!("FIRST POLL - START OPERATION");
            self.write_request();
            // CHANGED
            let stream = self.stream.as_mut().unwrap();
            // id 是该流的标识，在反应器中用于注册 epoll ：在 epoll 注册过程中被当作 token 参数传入
            runtime::reactor().register(stream, Interest::READABLE, self.id);
            // 保存 waker 并设置 waker 和该 stream （id）相关联
            runtime::reactor().set_waker(waker, self.id);
            // ============
        }

        let mut buff = vec![0u8; 1024];
        loop {
            match self.stream.as_mut().unwrap().read(&mut buff) {
                Ok(0) => {
                    let s = String::from_utf8_lossy(&self.buffer);
                    // 如果流已经读完了则需要取消注册，以免操作系统错误的又返回了带了 token 的同一事件，
                    // 这样就会错误的执行关联了该 token id 的 waker 的唤醒操作。
                    runtime::reactor().deregister(self.stream.as_mut().unwrap(), self.id);
                    break PollState::Ready(s.to_string());
                }
                Ok(n) => {
                    self.buffer.extend(&buff[0..n]);
                    continue;
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    // always store the last given Waker
                    // 当数据没有到达的时候总是保存最后一次 poll 的时候 future 给的 waker 。
                    // 这是因为 waker 中记录的线程有可能不一样，不更新 waker 可能会 unpark 错误的线程。
                    runtime::reactor().set_waker(waker, self.id);
                    break PollState::NotReady;
                }

                Err(e) => panic!("{e:?}"),
            }
        }
    }
}
