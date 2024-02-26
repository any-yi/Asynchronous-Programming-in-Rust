use crate::runtime::Waker;
use mio::{net::TcpStream, Events, Interest, Poll, Registry, Token};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
};

/// 反应器在内部保存 waker 队列，key 为 stream id（标识），value 为 waker。
type Wakers = Arc<Mutex<HashMap<usize, Waker>>>;

static REACTOR: OnceLock<Reactor> = OnceLock::new();

pub fn reactor() -> &'static Reactor {
    REACTOR.get().expect("Called outside an runtime context")
}

pub struct Reactor {
    // wakers 为 waker 的队列，反应器在内部保存 waker 队列。
    // key 为 stream id（标识），value 为 waker。
    wakers: Wakers,
    // 注册器即为 mio 的注册器（Poll 封装的）。
    registry: Registry,
    // next_id 为反应器保存的 stream 的标识。
    next_id: AtomicUsize,
}

impl Reactor {
    pub fn register(&self, stream: &mut TcpStream, interest: Interest, id: usize) {
        self.registry.register(stream, Token(id), interest).unwrap();
    }

    /// 保存 waker 并设置 waker 和该 stream （id）相关联，
    /// 在 poll 叶子 future 的时候使用
    ///
    /// 在数据没有到达的时候也会调用一次该函数，
    /// 这是因为数据没有到达的时候，有可能多次进行顶层 future 的 poll ，
    /// 而每次 poll 顶层 future 的时候都会创建一个新的 Waker（Waker 中记录的执行器线程有可能不一样）传递到下层 future ,
    /// 这时候就要即使更新 Waker ，以免错误的 wake（unpark 了错误的线程）。
    pub fn set_waker(&self, waker: &Waker, id: usize) {
        let _ = self
            .wakers
            .lock()
            // Must always store the most recent waker
            .map(|mut w| w.insert(id, waker.clone()).is_none())
            .unwrap();
    }

    /// 先删除 waker 及其 stream 关联关系，再实际取消注册器的注册
    pub fn deregister(&self, stream: &mut TcpStream, id: usize) {
        self.wakers.lock().map(|mut w| w.remove(&id)).unwrap();
        self.registry.deregister(stream).unwrap();
    }

    pub fn next_id(&self) -> usize {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

fn event_loop(mut poll: Poll, wakers: Wakers) {
    let mut events = Events::with_capacity(100);
    loop {
        poll.poll(&mut events, None).unwrap();
        // 在这里添加了上一节没有添加的事件处理
        // 根据 token 识别 stream （读出 stream id），再读出 stream id 绑定的 waker，
        // 最后在读出的 waker 上调用 wake 方法将 waker 绑定的 future 加入 ready 队列，并唤醒(unpark)执行器
        for e in events.iter() {
            let Token(id) = e.token();
            let wakers = wakers.lock().unwrap();

            if let Some(waker) = wakers.get(&id) {
                waker.wake();
            }
        }
    }
}

pub fn start() {
    // 导入线程
    use thread::spawn;

    let wakers = Arc::new(Mutex::new(HashMap::new()));
    let poll = Poll::new().unwrap();
    let registry = poll.registry().try_clone().unwrap();
    let next_id = AtomicUsize::new(1);
    // 创建反应器实例
    let reactor = Reactor {
        wakers: wakers.clone(),
        registry,
        next_id,
    };

    REACTOR.set(reactor).ok().expect("Reactor already running");
    // 创建一个线程，该线程专门用来处理事件循环
    spawn(move || event_loop(poll, wakers));
}


