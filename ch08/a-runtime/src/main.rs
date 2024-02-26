mod future;
mod http;
mod runtime;

use future::{Future, PollState};
use runtime::Runtime;

fn main() {
    let future = async_main();
    // Runtime 的 new 方法只完成了 epfd 的创建
    let mut runtime = Runtime::new();
    // 事件的注册发生在初次 poll 最上层 future 的时候，最上层 future 是 async_main 代码块的别名，
    // 进入 async_main 代码块后，会去 poll http.rs 中的 HttpGetFuture 对象（Future 对象），
    // 而 HttpGetFuture 的 poll 方法恰好就添加了事件的注册过程。
    //
    // 而第 4 章的 Poll 对象的 poll 方法则封装了 epoll_wait 函数，恰好 block_on 调用的就是该 poll 方法。
    // 调用 epoll_wait 会阻塞直到操作系统到达事件或超时。
    // 此时阻塞的是最上层的 future ，这里的（runtime.rs 里面的）代码代替了 之前不断轮询最上层 future 的代码。
    runtime.block_on(future);
}



// =================================
// We rewrite this:
// =================================

// coroutine fn async_main() {
//     println!("Program starting");
//     
//     let txt = http::Http::get("/600/HelloAsyncAwait").wait;
//     println!("{txt}");
//     let txt = http::Http::get("/400/HelloAsyncAwait").wait;
//     println!("{txt}");

// }

// =================================
// Into this:
// =================================

fn async_main() -> impl Future<Output=String> {
    Coroutine0::new()
}

enum State0 {
    Start,
    Wait1(Box<dyn Future<Output = String>>),
    Wait2(Box<dyn Future<Output = String>>),
    Resolved,
}

struct Coroutine0 {
    state: State0,
}

impl Coroutine0 {
    fn new() -> Self {
        Self { state: State0::Start }
    }
}


impl Future for Coroutine0 {
    type Output = String;

    fn poll(&mut self) -> PollState<Self::Output> {
        loop {
        match self.state {
                State0::Start => {
                    // ---- Code you actually wrote ----
                    println!("Program starting");

                    // ---------------------------------
                    let fut1 = Box::new( http::Http::get("/600/HelloAsyncAwait"));
                    self.state = State0::Wait1(fut1);
                }

                State0::Wait1(ref mut f1) => {
                    match f1.poll() {
                        PollState::Ready(txt) => {
                            // ---- Code you actually wrote ----
                            println!("{txt}");

                            // ---------------------------------
                            let fut2 = Box::new( http::Http::get("/400/HelloAsyncAwait"));
                            self.state = State0::Wait2(fut2);
                        }
                        PollState::NotReady => break PollState::NotReady,
                    }
                }

                State0::Wait2(ref mut f2) => {
                    match f2.poll() {
                        PollState::Ready(txt) => {
                            // ---- Code you actually wrote ----
                            println!("{txt}");

                            // ---------------------------------
                            self.state = State0::Resolved;
                            break PollState::Ready(String::new());
                        }
                        PollState::NotReady => break PollState::NotReady,
                    }
                }

                State0::Resolved => panic!("Polled a resolved future")
            }
        }
    }
}
