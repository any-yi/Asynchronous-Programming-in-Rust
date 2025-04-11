/// FIX #31:
/// Inline assembly blocks inside naked functions now need to use
/// the `naked_asm` macro instead of the good old `asm` macro.
/// The `noreturn` option is implicitly set by the `naked_asm`
/// macro so there is no need to set that.
///
/// See: https://github.com/PacktPublishing/Asynchronous-Programming-in-Rust/issues/31
/// for more information.
#![feature(naked_functions)]
use std::arch::{asm, naked_asm};

const DEFAULT_STACK_SIZE: usize = 1024 * 1024 * 2;
const MAX_THREADS: usize = 4;
static mut RUNTIME: usize = 0;

pub struct Runtime {
    threads: Vec<Thread>,  // 运行时保存的线程队列
    current: usize,        // 线程队列中当前正在运行的线程下标
}

#[derive(PartialEq, Eq, Debug)]
enum State {
    Available, // 可用/空闲状态，该线程的堆栈中没有任务
    Running,   // 运行状态
    Ready,     // 准备状态，该线程的堆栈中有任务，只是暂停运行，而且随时可以被运行
}

struct Thread {
    // 线程使用的堆栈，业务代码可用，且还可用于记录一些业务代码执行完后的回调函数地址。
    // 用于 Ready->Available 状态的转换
    stack: Vec<u8>,
    // 线程上下文，记录 CPU 实际的寄存器信息，用于暂停/恢复运行（保存/还原现场）。
    // 这不是堆栈的一部分，而是内存中一组固定的空间。这里不用堆栈来保存寄存器。
    // 用于 Ready-Running 状态的转换
    ctx: ThreadContext,
    state: State,
}

#[derive(Debug, Default)]
#[repr(C)]
struct ThreadContext {
    rsp: u64,
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    rbx: u64,
    rbp: u64,
}

impl Thread {
    fn new() -> Self {
        Thread {
            stack: vec![0_u8; DEFAULT_STACK_SIZE],
            ctx: ThreadContext::default(),
            state: State::Available,
        }
    }
}

impl Runtime {
    /// 初始化线程队列，
    /// 创建一个状态为 Running 的基础线程，
    /// 创建 MAX_THREADS 个状态为 Available 的线程，
    /// 并把当前线程设置为下标为 0 的线程，也即上述基础线程
    pub fn new() -> Self {
        // 创建一个状态为 Running 的基础线程加入线程队列
        let base_thread = Thread {
            stack: vec![0_u8; DEFAULT_STACK_SIZE],
            ctx: ThreadContext::default(),
            state: State::Running,
        };

        let mut threads = vec![base_thread];
        // 再创建 MAX_THREADS 个状态为 Available 的线程，加入线程队列
        let mut available_threads: Vec<Thread> = (1..MAX_THREADS).map(|_| Thread::new()).collect();
        threads.append(&mut available_threads);

        // 返回 Runtime ，并把当前线程设置为下标为 0 的线程，也即上述基础线程
        Runtime {
            threads,
            current: 0,
        }
    }

    /// 将全局变量 RUNTIME 指向调用者
    pub fn init(&self) {
        unsafe {
            let r_ptr: *const Runtime = self;
            RUNTIME = r_ptr as usize;
        }
    }

    /// 线程运行时启动
    pub fn run(&mut self) -> ! {
        // 在这里开启循环，切换线程，而不是在 t_yield 中循环
        // 只要 t_yield 返回 true，就永远调度线程，
        // 也即只要线程队列中还有线程的记录，就会一直调度，直到队列为空
        while self.t_yield() {}
        std::process::exit(0);
    }

    /// 只要当前线程不是基础线程，就修改其状态为 Available，再调度/切换线程
    /// 如果是基础线程则什么也不干
    ///
    /// 使用 spawn 方法生成的线程在返回时都会调用本方法，
    /// 从而使线程队列能不断进行 Ready-Available 状态的转换。
    fn t_return(&mut self) {
        if self.current != 0 {
            self.threads[self.current].state = State::Available;
            self.t_yield();
        }
    }

    /// 调度/切换线程，令当前线程之后的第一个状态为 Ready 的线程修改状态为 Running 并实际跑起来
    #[inline(never)]
    fn t_yield(&mut self) -> bool {
        // 从当前线程开始，在线程队列中找一个状态为 Ready 的线程，
        // 如果找到了，pos 是其下标索引
        // 如果找不到，就返回 false
        let mut pos = self.current;
        while self.threads[pos].state != State::Ready {
            pos += 1;
            if pos == self.threads.len() {
                pos = 0;
            }
            // 找了一圈了，还是没找到，说明没有 Ready 状态的线程
            if pos == self.current {
                return false;
            }
        }

        // 如果当前线程的状态不是 Available ，
        // 说明当前线程还没有调用 t_return ，说明当前线程不是运行结束，从而被动出让的 CPU 控制权，
        // 而是主动出让 CPU 控制权，那当前线程就应该还是 Running 状态，则应该将其修改为 Ready 状态。
        if self.threads[self.current].state != State::Available {
            self.threads[self.current].state = State::Ready;
        }

        // 修改我们找到的这个 Ready 线程的状态，准备让它跑起来
        self.threads[pos].state = State::Running;
        let old_pos = self.current;
        // 令即将要跑起来的线程成为当前线程
        self.current = pos;

        // 取出新旧线程的上下文（一组保存在内存中的寄存器），
        // 调用一个 switch 函数，传入新旧线程的上下文，
        // 在 switch 中对上下文进行压栈出栈操作，
        // 最后使用 ret 指令，修改 rip 寄存器为栈顶保存的地址，让 CPU 切换线程
        unsafe {
            let old: *mut ThreadContext = &mut self.threads[old_pos].ctx;
            let new: *const ThreadContext = &self.threads[pos].ctx;
            asm!("call switch", in("rdi") old, in("rsi") new, clobber_abi("C"));
        }
        self.threads.len() > 0
    }

    /// 根据传入的闭包（函数指针），在线程队列中修改某个 Available 线程的状态，从而产生一个新的 Ready 状态的线程
    /// 但在本 spawn 方法的代码中并不会实际开始调度该线程
    pub fn spawn(&mut self, f: fn()) {
        // 从线程队列头开始，找到第一个状态为 Available 的线程
        let available = self
            .threads
            .iter_mut()
            .find(|t| t.state == State::Available)
            .expect("no available thread.");

        let size = available.stack.len();

        unsafe {
            // 找到这个线程的栈底，创建栈底指针变量
            let s_ptr = available.stack.as_mut_ptr().offset(size as isize);
            let s_ptr = (s_ptr as usize & !15) as *mut u8;
            // 依次写入堆栈数据：
            //     guard 为 guard 函数，把线程的状态修改为 Available 并调度/切换线程
            //     skip 为 skip 函数，运行 ret 指令
            //     f 为传入本方法的函数指针，这是该线程主要想要运行的业务代码
            // 运行完业务代码 f 后，将借助 skip 的 ret 指令运行 guard 函数，
            // 把线程的状态修改为 Available 并调度/切换线程。
            std::ptr::write(s_ptr.offset(-16) as *mut u64, guard as u64);
            std::ptr::write(s_ptr.offset(-24) as *mut u64, skip as u64);
            std::ptr::write(s_ptr.offset(-32) as *mut u64, f as u64);
            // 令这个 Available 的线程保存新栈顶
            available.ctx.rsp = s_ptr.offset(-32) as u64;
        }
        // 修改 Available 的线程状态为 Ready
        available.state = State::Ready;
    }
} // We close the `impl Runtime` block here

fn guard() {
    unsafe {
        let rt_ptr = RUNTIME as *mut Runtime;
        (*rt_ptr).t_return();
    };
}

#[naked]
unsafe extern "C" fn skip() {
    naked_asm!("ret")
}

pub fn yield_thread() {
    unsafe {
        let rt_ptr = RUNTIME as *mut Runtime;
        (*rt_ptr).t_yield();
    };
}

#[naked]
#[no_mangle]
#[cfg_attr(target_os = "macos", export_name = "\x01switch")] // see: How-to-MacOS-M.md for explanation
unsafe extern "C" fn switch() {
    naked_asm!(
        "mov [rdi + 0x00], rsp",
        "mov [rdi + 0x08], r15",
        "mov [rdi + 0x10], r14",
        "mov [rdi + 0x18], r13",
        "mov [rdi + 0x20], r12",
        "mov [rdi + 0x28], rbx",
        "mov [rdi + 0x30], rbp",
        "mov rsp, [rsi + 0x00]",
        "mov r15, [rsi + 0x08]",
        "mov r14, [rsi + 0x10]",
        "mov r13, [rsi + 0x18]",
        "mov r12, [rsi + 0x20]",
        "mov rbx, [rsi + 0x28]",
        "mov rbp, [rsi + 0x30]",
        "ret"
    );
}

fn main() {
    let mut runtime = Runtime::new();
    runtime.init();

    // spawn 生成的线程状态为 Ready 状态，但此时还未显式让运行时进行调度。
    //
    // 因为闭包里的 yield_thread 是实际运行闭包时才会开始调度，
    // 而 spawn 这个过程既不会运行闭包代码，也不会显示调用 runtime 的 t_yield 方法调度线程。
    //
    // 因此 spawn 只是把闭包代码加入线程队列中的某一个线程，然后等待某一个线程（主线程）开始调度。
    runtime.spawn(|| {
        println!("THREAD 1 STARTING");
        let id = 1;
        for i in 0..10 {
            println!("thread: {} counter: {}", id, i);
            // 本闭包里的代码运行到一半的时候需要主动出让 CPU 控制权，让线程运行时进行调度
            yield_thread();
        }
        println!("THREAD 1 FINISHED");
    });

    runtime.spawn(|| {
        println!("THREAD 2 STARTING");
        let id = 2;
        for i in 0..15 {
            println!("thread: {} counter: {}", id, i);
            yield_thread();
        }
        println!("THREAD 2 FINISHED");
    });
    // 主线程出让 CPU 控制权，让线程运行时进行调度
    runtime.run();
}
