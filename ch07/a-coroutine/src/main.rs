use std::{
    thread,
    time::Duration,
};

mod future;
mod http;

use crate::http::Http;
use future::{Future, PollState};

// This state machine would be similar to the one created by:
// async fn async_main() {
//     println!("Program starting");
//     let txt = http::Http::get("/600/HelloAsyncAwait").await;
//     println!("{txt}");
//     let txt = http::Http::get("/400/HelloAsyncAwait").await;
//     println!("{txt}");
// }

/// Coroutine 内部只保存当前状态
///
/// http.rs 中通过保存流（stream）隐式的保存了状态，
/// 只有保存了状态才能利用 poll 方法根据内部的状态来推进状态。
/// 因此只有内部保存了状态的对象才能实现 Future trait 。
struct Coroutine {
    state: State,
}

/// 可以完全看到状态图：开始 -> 状态1 -> 状态2 -> 结束
enum State {
    Start,
    Wait1(Box<dyn Future<Output = String>>),
    Wait2(Box<dyn Future<Output = String>>),
    Resolved,
}

impl Coroutine {
    fn new() -> Self {
        Self {
            state: State::Start,
        }
    }
}

impl Future for Coroutine {
    type Output = ();

    fn poll(&mut self) -> PollState<Self::Output> {
        // 循环不断推进状态直到再也推进不下去，
        // 一次可能推进多个状态，所以用 loop
        loop {
            match self.state {
                State::Start => {
                    println!("Program starting");
                    // 遇到了第 1 个出让点（yield point），
                    // 把当前状态修改成 Wait1，
                    //
                    // Wait1 枚举变量保存一个 future trait 对象的 box 引用
                    // （Http::get 方法返回一个 HttpGetFuture 对象同时也是一个 Future 对象），
                    // 表示该 Wait1 状态的目的是等待这个 Future。
                    //
                    // 这种写法是懒汉式 Future 的写法，编译器总会把 async/await 生成这样的状态机
                    // Box 里的 HTTP 等内容是上述 .await 之前的内容，
                    // 而返回的 HTTP 响应结果要等到下一状态进行处理
                    let fut = Box::new(Http::get("/600/HelloWorld1"));
                    self.state = State::Wait1(fut);
                }

                // 调用传入的 future 对象的 poll 方法推进状态
                State::Wait1(ref mut fut) => match fut.poll() {
                    PollState::Ready(txt) => {
                        // 输出得到的 HTTP 响应
                        println!("{txt}");
                        // 遇到第 2 个出让点
                        let fut2 = Box::new(Http::get("/400/HelloWorld2"));
                        self.state = State::Wait2(fut2);
                    }

                    PollState::NotReady => break PollState::NotReady,
                },

                State::Wait2(ref mut fut2) => match fut2.poll() {
                    PollState::Ready(txt2) => {
                        println!("{txt2}");
                        self.state = State::Resolved;
                        break PollState::Ready(());
                    }

                    PollState::NotReady => break PollState::NotReady,
                },

                State::Resolved => panic!("Polled a resolved future"),
            }
        }
    }
}

fn async_main() -> impl Future<Output = ()> {
    Coroutine::new()
}

fn main() {
    let mut future = async_main();

    loop {
        match future.poll() {
            PollState::NotReady => {
                println!("Schedule other tasks");
            }
            PollState::Ready(_) => break,
        }

        // Since we print every poll, slow down the loop
        // 因为我们每次 poll 的时候都打印输出，所以要减慢循环速度。
        thread::sleep(Duration::from_millis(100));
    }
}
