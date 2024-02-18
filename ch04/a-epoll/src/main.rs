use std::{
    io::{self, Read, Result, Write},
    net::TcpStream,
};

use ffi::Event;
use poll::Poll;

mod ffi;
mod poll;

/// Not the entire url, but everyhing after the domain addr
/// i.e. http://localhost/1000/hello => /1000/hello
fn get_req(path: &str) -> String {
    format!(
        "GET {path} HTTP/1.1\r\n\
             Host: localhost\r\n\
             Connection: close\r\n\
             \r\n"
    )
}

fn handle_events(events: &[Event], streams: &mut [TcpStream]) -> Result<usize> {
    let mut handled_events = 0;
    // 对传入的所有操作系统返回的事件进行循环处理
    for event in events {
        let index = event.token();
        let mut data = vec![0u8; 4096];

        // 需要多次进行 read 操作以确保一定耗尽了流而不是耗尽了此处的 data 缓冲区
        loop {
            // 调用流的 read 方法读取服务器返回的数据
            // 因为当前 event 可能和 streams 的顺序不是按发送的顺序一一对应的，
            // 因此需要把 event 的 token 取出来看看这个 event 对应的是哪个 stream
            match streams[index].read(&mut data) {
                // 读到了这条流的末尾，把处理流的计数+1，退出读取当前流
                Ok(n) if n == 0 => {
                    handled_events += 1;
                    break;
                }
                Ok(n) => {
                    let txt = String::from_utf8_lossy(&data[..n]);

                    println!("RECEIVED: {:?}", event);
                    println!("{txt}\n------\n");
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                // this was not in the book example, but it's a error condition
                // you probably want to handle in some way (either by breaking
                // out of the loop or trying a new read call immidiately)
                Err(e) if e.kind() == io::ErrorKind::Interrupted => break,
                Err(e) => return Err(e),
            }
        }
    }

    Ok(handled_events)
}

fn main() -> Result<()> {
    let mut poll = Poll::new()?;
    let n_events = 5;

    let mut streams = vec![];
    let addr = "localhost:8080";

    for i in 0..n_events {
        // 拼接 HTTP GET 请求字符串
        // 第 1 次循环延迟 5 秒，第 2 次延迟 4 秒 ...
        let delay = (n_events - i) * 1000;
        let url_path = format!("/{delay}/request-{i}");
        let request = get_req(&url_path);
        // 初始化网络连接对象
        let mut stream = std::net::TcpStream::connect(addr)?;
        // 禁用 TcpStream 的 Nagle 算法（把 TCP_NODELAY 标志设置为 true ）
        stream.set_nonblocking(true)?;

        // 发送 GET 请求
        stream.write_all(request.as_bytes())?;
        // NB! Token is equal to index in Vec
        // 传入 stream ，以边沿提醒的方式订阅关于 stream 的读取事件
        // token 也即 epoll_data 设置为循环变量 i ，在 handle_events 方法中读出来该数据，用于索引 streams 向量
        poll.registry()
            .register(&stream, i, ffi::EPOLLIN | ffi::EPOLLET)?;

        streams.push(stream);
    }

    let mut handled_events = 0;
    // 接收 5 次事件提醒
    while handled_events < n_events {
        // 初始化用于接收操作系统返回的事件的队列
        let mut events = Vec::with_capacity(10);
        // 超时时间设置为 None ，表示可无限等待
        poll.poll(&mut events, None)?;

        // SPURIOUS：伪造的
        if events.is_empty() {
            println!("TIMEOUT (OR SPURIOUS EVENT NOTIFICATION)");
            continue;
        }

        // 处理接受到的操作系统返回的 events，
        // handle_events 函数返回本次处理了多少个事件，将其累加到用于循环判断的变量上
        handled_events += handle_events(&events, &mut streams)?;
    }

    println!("FINISHED");
    Ok(())
}
