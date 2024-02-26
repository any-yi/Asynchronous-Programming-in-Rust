use crate::future::{Future, PollState};
use mio::{Events, Poll, Registry};
use std::sync::OnceLock;

/// 将 registry 存储在 REGISTRY 全局变量中，
/// 以便稍后可以从 http 模块访问它，而无需引用运行时本身。
static REGISTRY: OnceLock<Registry> = OnceLock::new();

pub fn registry() -> &'static Registry {
    REGISTRY.get().expect("Called outside a runtime context")
}

pub struct Runtime {
    // 第4章创建了类似的 Poll 结构体，保存了一个注册器 Registry。
    // 而 Registry 又封装了一个 epfd（epoll fd）
    poll: Poll,
}

impl Runtime {
    pub fn new() -> Self {
        // Poll 的 new 方法主要是为了拿到 epfd
        let poll = Poll::new().unwrap();
        // 这里获得的是有所有权的空 Registry 。
        //
        // 注册过程要传入订阅的流及其相关感兴趣的事件，还要传入一个标志以便将来识别该流。
        // 传入上述参数的过程在 http.rs 中，实际注册过程发生在第一次 poll 顶层 Future 的时候，
        // 需要和 async_main 块（顶层 Future ）里的代码、以及 http.rs 里的代码结合起来。
        let registry = poll.registry().try_clone().unwrap();
        REGISTRY.set(registry).unwrap();
        Self { poll }
    }

    pub fn block_on<F>(&mut self, future: F)
    where
        F: Future<Output = String>,
    {
        let mut future = future;
        loop {
            match future.poll() {
                PollState::NotReady => {
                    println!("Schedule other tasks\n");
                    // 创建一个事件队列接受来自操作系统的事件
                    let mut events = Events::with_capacity(100);
                    // 这里是一个阻塞调用，超时时间设置为无限。
                    // 如果有事件则此时会返回 Ok ，
                    // 然后就回到循环开头，此时 future 又 poll 一次则应当返回 Ready 。
                    self.poll.poll(&mut events, None).unwrap();
                }

                PollState::Ready(_) => break,
            }
        }
    }
}
