mod future;
mod http;
mod runtime;
use future::{Future, PollState};
use runtime::Waker;
use std::fmt::Write;

fn main() {
    let mut executor = runtime::init();
    executor.block_on(async_main());
}

// =================================
// We rewrite this:
// =================================

// coroutine fn async_main() {
//     let mut buffer = String::from("\nBUFFER:\n----\n");
//     let writer = &mut buffer;
//     println!("Program starting");
//     let txt = http::Http::get("/600/HelloAsyncAwait").wait;
//     writeln!(writer, "{txt}").unwrap();
//     let txt = http::Http::get("/400/HelloAsyncAwait").wait;
//     writeln!(writer, "{txt}").unwrap();

//     println!("{}", buffer);
// }

// =================================
// Into this:
// =================================

fn async_main() -> impl Future<Output = String> {
    Coroutine0::new()
}

enum State0 {
    Start,
    Wait1(Box<dyn Future<Output = String>>),
    Wait2(Box<dyn Future<Output = String>>),
    Resolved,
}

#[derive(Default)]
struct Stack0 {
    buffer: Option<String>,
    // 注意 writer 不是 Option<&mut String> ，因为如果是的话，
    // 则 writer 引用了 buffer ，就变成了一个自引用结构，
    // 无法正确表达生命周期（该引用的生命周期需要不长于结构体对象），
    // 所以只能用裸指针
    writer: Option<*mut String>,
}

struct Coroutine0 {
    stack: Stack0,
    state: State0,
}

impl Coroutine0 {
    fn new() -> Self {
        Self {
            state: State0::Start,
            stack: Stack0::default(),
        }
    }
}

impl Future for Coroutine0 {
    type Output = String;

    fn poll(&mut self, waker: &Waker) -> PollState<Self::Output> {
        loop {
            match self.state {
                State0::Start => {
                    // initialize stack (hoist variables)
                    self.stack.buffer = Some(String::from("\nBUFFER:\n----\n"));
                    self.stack.writer = Some(self.stack.buffer.as_mut().unwrap());
                    // ---- Code you actually wrote ----
                    println!("Program starting");

                    // ---------------------------------
                    let fut1 = Box::new(http::Http::get("/600/HelloAsyncAwait"));
                    self.state = State0::Wait1(fut1);

                    // save stack
                }

                State0::Wait1(ref mut f1) => {
                    match f1.poll(waker) {
                        PollState::Ready(txt) => {
                            // Restore stack
                            // 注意这里使用了 take ，取了 writer 裸指针本身的所有权
                            let writer = unsafe { &mut *self.stack.writer.take().unwrap() };

                            // ---- Code you actually wrote ----
                            writeln!(writer, "{txt}").unwrap();
                            // ---------------------------------
                            let fut2 = Box::new(http::Http::get("/400/HelloAsyncAwait"));
                            self.state = State0::Wait2(fut2);

                            // save stack
                            self.stack.writer = Some(writer);
                        }
                        PollState::NotReady => break PollState::NotReady,
                    }
                }

                State0::Wait2(ref mut f2) => {
                    match f2.poll(waker) {
                        PollState::Ready(txt) => {
                            // Restore stack
                            // 这里取得的是 &String 的所有权，而不是 String 的
                            let buffer = self.stack.buffer.as_ref().take().unwrap();
                            let writer = unsafe { &mut *self.stack.writer.take().unwrap() };

                            // ---- Code you actually wrote ----
                            writeln!(writer, "{txt}").unwrap();

                            println!("{}", buffer);
                            // ---------------------------------
                            self.state = State0::Resolved;

                            // Save stack / free resources
                            // 取掉 String 的所有权
                            let _ = self.stack.buffer.take();

                            break PollState::Ready(String::new());
                        }
                        PollState::NotReady => break PollState::NotReady,
                    }
                }

                State0::Resolved => panic!("Polled a resolved future"),
            }
        }
    }
}
