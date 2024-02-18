// 添加事件到事件队列
pub const EPOLL_CTL_ADD: i32 = 1;
// 对文件句柄上的读取操作感兴趣
pub const EPOLLIN: i32 = 0x1;
// 获取通知的方式为边沿触发模式（edge-triggered）
// 电平触发：只要为高电平，就一直通知读取数据，哪怕正在进行处理，只是因为没有处理完，高电平还没有变成低电平
// 边沿触发：低电平变为高电平是才通知，没有这个特定的变化就不通知。
pub const EPOLLET: i32 = 1 << 31;

#[link(name = "c")]
extern "C" {
    // size 无意义，但要 > 0
    pub fn epoll_create(size: i32) -> i32;
    pub fn close(fd: i32) -> i32;
    // 对传入的 epoll fd （epfd）做一些操作
    // 注册：
    //     op 传递上述 EPOLL_CTL_ADD 操作
    //     event 传入感兴趣的事件
    pub fn epoll_ctl(epfd: i32, op: i32, fd: i32, event: *mut Event) -> i32;
    // 阻塞直到事件已发生或已超时，此时调用本函数时传入的 events 结构体：
    //     events 字段标识发生了什么事件
    //     epoll_data 为传入时相同的数据，用以标识事件的来源，
    //                比如传入时设置为文件标识符，事件发生后就知道哪个文件描述符可以被读了。
    //  maxevents 表示事件队列里最多能放多少个事件（poll 中设置成了 Vec 的 Capacity ）
    //  timeout 为阻塞超时时间
    pub fn epoll_wait(epfd: i32, events: *mut Event, maxevents: i32, timeout: i32) -> i32;
}

// 操作系统以 pack 紧凑方式写数据，
// 如果字段数据不是标识为 pack 的，则会用 0 填充 u32 的后面 32 位（usize 是 64 位的）
// 则操作系统以 pack 方式填数据会把原本属于 epoll_data 的 64 位数据从填充区开头开始填 32 位，
// 并把剩下的 32 位填到 epoll_data 中去，填了脏数据。
#[derive(Debug)]
#[repr(C, packed)]
pub struct Event {
    // 标识事件类型；还可以通过这个字段修改收到通知时的行为和时间。
    pub(crate) events: u32,
    // Token to identify event
    // 传递给操作系统，事件发生的时候被返回。标识事件的来源等信息。
    pub(crate) epoll_data: usize,
}

impl Event {
    pub fn token(&self) -> usize {
        self.epoll_data
    }
}
