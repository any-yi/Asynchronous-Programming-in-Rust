use crate::future::{Future, PollState};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    sync::{Arc, Mutex},
    thread::{self, Thread},
};

type Task = Box<dyn Future<Output = String>>;

thread_local! {
    static CURRENT_EXEC: ExecutorCore = ExecutorCore::default();
}

#[derive(Default)]
struct ExecutorCore {
    tasks: RefCell<HashMap<usize, Task>>,
    ready_queue: Arc<Mutex<Vec<usize>>>,
    next_id: Cell<usize>,
}

pub fn spawn<F>(future: F)
where
    F: Future<Output = String> + 'static,
{
    CURRENT_EXEC.with(|e| {
        let id = e.next_id.get();
        e.tasks.borrow_mut().insert(id, Box::new(future));
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

    fn get_future(&self, id: usize) -> Option<Task> {
        CURRENT_EXEC.with(|q| q.tasks.borrow_mut().remove(&id))
    }

    fn get_waker(&self, id: usize) -> Waker {
        Waker {
            id,
            thread: thread::current(),
            ready_queue: CURRENT_EXEC.with(|q| q.ready_queue.clone()),
        }
    }

    fn insert_task(&self, id: usize, task: Task) {
        CURRENT_EXEC.with(|q| q.tasks.borrow_mut().insert(id, task));
    }

    fn task_count(&self) -> usize {
        CURRENT_EXEC.with(|q| q.tasks.borrow().len())
    }

    pub fn block_on<F>(&mut self, future: F)
    where
        F: Future<Output = String> + 'static,
    {
        // ===== OPTIMIZATION, ASSUME READY
        let waker = self.get_waker(usize::MAX);
        let mut future = future;
        // 这里的 future 代表 main.rs 里面的状态机代码。
        // 其中 Start 状态时先是初始化了 String ，
        // 而该初始化的 String 结构体放置在本 block_on 函数的栈上面一个单位，也即 poll 函数的栈上，
        // 而 stack 的 writer 字段，指向的位置是该 String 位置，该位置所处的栈空间和本 block_on 函数、poll 函数是同一个栈空间。
        match future.poll(&waker) {
            // 如果返回 NotReady，则会执行下方的 `spawn(future)` 。
            PollState::NotReady => (),
            PollState::Ready(_) => return,
        }
        // ===== END

        // 此处的代码执行完， future 所代表的数据被移动到 Box（堆）中，
        // 且 future 变量的所有权被转移到了 HashMap 里，future 变量不再可用。
        //
        // 但是由于 future 的特性，需要在推进到不可能再推进的地方暂停工作，
        // 因此在后面重新恢复 future 工作的时候，并不会再从开头开始工作，
        // 因此已经被初始化的栈不会再初始化一遍，writer 指向的是旧的栈空间地址。
        //
        // 自引用结构在这里表现为：在 future 内部的 writer 指向 future 所处的某一固定位置，
        // 而当 future 移动到别处时，其内部的 writer 并不会也跟着改变其指向。
        spawn(future);

        loop {
            while let Some(id) = self.pop_ready() {
                let mut future = match self.get_future(id) {
                    Some(f) => f,
                    // guard against false wakeups
                    None => continue,
                };
                let waker = self.get_waker(id);

                match future.poll(&waker) {
                    PollState::NotReady => self.insert_task(id, future),
                    PollState::Ready(_) => continue,
                }
            }

            let task_count = self.task_count();
            let name = thread::current().name().unwrap_or_default().to_string();

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
    thread: Thread,
    id: usize,
    ready_queue: Arc<Mutex<Vec<usize>>>,
}

impl Waker {
    pub fn wake(&self) {
        self.ready_queue
            .lock()
            .map(|mut q| q.push(self.id))
            .unwrap();
        self.thread.unpark();
    }
}
