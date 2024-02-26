use crate::future::{Future, PollState};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    sync::{Arc, Mutex},
    thread::{self, Thread},
};

type Task = Box<dyn Future<Output = String>>;

// ExecutorCore 的字段均初始化为默认值，则 next_id 初始值为 0
thread_local! {
    static CURRENT_EXEC: ExecutorCore = ExecutorCore::default();
}

#[derive(Default)]
struct ExecutorCore {
    // 该字段保存执行器在本线程相关的所有顶级 future 及其对应的 id
    //
    // 使用 RefCell 包裹是因为无法修改 static 变量（CURRENT_EXEC 是个不可变的变量）内部的字段，
    // 采用内部可变性则可以修改
    tasks: RefCell<HashMap<usize, Task>>,
    // Ready 队列，一个向量，里面记录 Ready 状态的任务的 id 。
    // 使用 Arc 包裹：可以与 Waker 共享这个堆分配的字段。
    ready_queue: Arc<Mutex<Vec<usize>>>,
    // id 对于任务来讲是独一无二的，不随执行器所在线程的不同而不同，
    // 由于 static 变量相同的原因，且需要是独一无二的（单一的实例），因此使用 Cell 包裹。
    next_id: Cell<usize>,
}

// 'static 生命周期限定意味着传入的必须能活得足够久，直到程序结束，
// 一般传入有所有权的变量就行了，传入引用则一般需要是 'static 生命周期的。
pub fn spawn<F>(future: F)
where
    F: Future<Output = String> + 'static,
{
    CURRENT_EXEC.with(|e| {
        let id = e.next_id.get();
        e.tasks.borrow_mut().insert(id, Box::new(future));
        // 刚创建一个任务就会放进 ready_queue 里先 poll 一次
        e.ready_queue.lock().map(|mut q| q.push(id)).unwrap();
        e.next_id.set(id + 1);
    });
}

pub struct Executor;

impl Executor {
    pub fn new() -> Self {
        Self {}
    }

    fn pop_ready(&self) -> Option<usize> {
        CURRENT_EXEC.with(|q| q.ready_queue.lock().map(|mut q| q.pop()).unwrap())
    }

    /// 将 id 对应的本线程 future 移出任务队列并返回，
    /// 主要是为了获取 Future 的所有权。
    fn get_future(&self, id: usize) -> Option<Task> {
        CURRENT_EXEC.with(|q| q.tasks.borrow_mut().remove(&id))
    }

    /// 通过传入的 id ，新建一个和任务相关联的 waker
    fn get_waker(&self, id: usize) -> Waker {
        Waker {
            id,
            thread: thread::current(),
            ready_queue: CURRENT_EXEC.with(|q| q.ready_queue.clone()),
        }
    }

    /// 本线程任务队列插入一个任务
    fn insert_task(&self, id: usize, task: Task) {
        CURRENT_EXEC.with(|q| q.tasks.borrow_mut().insert(id, task));
    }

    /// 统计本线程任务队列的任务个数
    fn task_count(&self) -> usize {
        CURRENT_EXEC.with(|q| q.tasks.borrow().len())
    }

    pub fn block_on<F>(&mut self, future: F)
    where
        F: Future<Output = String> + 'static,
    {
        // 由于懒汉式设计，Executor 的 new 方法并不会初始化 ExecutorCore，
        // 而是在调用本方法时调用 spawn ，spawn 引用 CURRENT_EXEC 静态变量，
        // 而 CURRENT_EXEC 静态变量通过 thread_local 宏里的 ExecutorCore::default 来自动初始化自身。
        //
        // 利用当前线程执行器派生一个新的任务，
        // 注意刚创建一个任务就会放进 ready_queue 里先 poll 一次
        spawn(future);
        loop {
            // 首先拿出一个 ready 队列中记录的 id
            while let Some(id) = self.pop_ready() {
                // 再拿到 ready 任务 id 对应的 ready 的 future
                let mut future = match self.get_future(id) {
                    Some(f) => f,
                    // guard against false wakeups
                    // 防止已完成的 future 被错误的唤醒（已完成的 future 被错误的插入 ready 队列）
                    None => continue,
                };
                // 再新建一个和这个任务（future）关联的 waker
                let waker = self.get_waker(id);

                // poll 这个 ready 状态的 future ，
                // 如果这个 future 所有的进度走完了，返回 Ready 了，那就 continue ，接着处理下一个 Ready 的任务，
                // 如果这个 future 还没有走完所有的进度，返回 NotReady ，就将其（所有权）插回任务队列。
                match future.poll(&waker) {
                    PollState::NotReady => self.insert_task(id, future),
                    PollState::Ready(_) => continue,
                }
            }

            let task_count = self.task_count();
            let name = thread::current().name().unwrap_or_default().to_string();

            // 能走到这里说明 Ready 队列能往前推进的顶级任务都已经推进完了，
            // 如果此时任务队列里至少还有一个任务，则说明当前正在等待的就是这个任务，那么就暂停执行器线程开始等待，
            // 如果此时任务队列没有任务了，则说明全部的任务都执行完了。
            if task_count > 0 {
                println!("{name}: {task_count} pending tasks. Sleep until notified.");
                thread::park();
            } else {
                println!("{name}: All tasks are finished");
                break;
            }
        }
    }
}

#[derive(Clone)]
pub struct Waker {
    // 保存的是执行器的线程句柄
    thread: Thread,
    // 表示该 Waker 与哪个任务相关联
    id: usize,
    // 和 ExecutorCore 实例、不同 id 的 Waker 实例一起，共享堆分配的 Ready 队列（通过 Arc 类型的引用计数）
    ready_queue: Arc<Mutex<Vec<usize>>>,
}

impl Waker {
    /// wake 过程，先把自己关联的任务 id 推送到 ready_queue 队列里，
    /// 再 unpark 唤醒执行器的线程，让其去 poll 这个 id 标识的任务。
    pub fn wake(&self) {
        self.ready_queue
            .lock()
            .map(|mut q| q.push(self.id))
            .unwrap();
        self.thread.unpark();
    }
}
