use std::{
    io::{self, Result},
    net::TcpStream,
    os::fd::AsRawFd,
};

use crate::ffi;

type Events = Vec<ffi::Event>;

pub struct Poll {
    registry: Registry,
}

impl Poll {
    pub fn new() -> Result<Self> {
        let res = unsafe { ffi::epoll_create(1) };
        if res < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(Self {
            registry: Registry { raw_fd: res },
        })
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Makes a blocking call to the OS parking the calling thread. It will wake up
    /// when one or more events we've registered interest in have occurred or
    /// the timeout duration has elapsed, whichever occurs first.
    ///
    /// # Note
    /// If the number of events returned is 0, the wakeup was due to an elapsed
    /// timeout
    pub fn poll(&mut self, events: &mut Events, timeout: Option<i32>) -> Result<()> {
        let fd = self.registry.raw_fd;
        // 如果为 None ，则一直等待，没有超时时间的限制
        let timeout = timeout.unwrap_or(-1);
        let max_events = events.capacity() as i32;
        // fd 是从保存的注册器里取出的 epfd ，
        // 而 events（接受到的事件队列） 、max_events 、timeout 都是传入的
        let res = unsafe { ffi::epoll_wait(fd, events.as_mut_ptr(), max_events, timeout) };

        if res < 0 {
            return Err(io::Error::last_os_error());
        };

        // This is safe because epol_wait ensures that `res` events are assigned.
        // 操作系统向传入本 poll 方法的 events 向量写了内容，但没有写其 len 字段
        unsafe { events.set_len(res as usize) };
        Ok(())
    }
}

pub struct Registry {
    raw_fd: i32,
}

impl Registry {
    // NB! Mio inverts this, and `source` owns the register implementation
    pub fn register(&self, source: &TcpStream, token: usize, interests: i32) -> Result<()> {
        // 传入本方法的 token 、interests 都是用来初始化 Event 结构体的
        // token 用于填充 epoll_data
        // interests 用于填充 events
        let mut event = ffi::Event {
            events: interests as u32,
            epoll_data: token,
        };

        let op = ffi::EPOLL_CTL_ADD;
        // 上面都是在初始化 epoll_ctl 需要的变量
        // 其中传入 epoll_ctl 的 epfd 和上面的 poll 方法是同一个值
        // event 用于指示对 source 上的什么操作感兴趣、发生事件后如何提示、接受到事件后如何区分哪一个 source
        //
        // 这里的 event 和 poll 中的 events 中的 event 数据结构是一样的，
        // 只不过一个发送给操作系统，表示预期的事件的信息，一个用于接受实际的事件发生后的信息。
        let res = unsafe { ffi::epoll_ctl(self.raw_fd, op, source.as_raw_fd(), &mut event) };

        if res < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        let res = unsafe { ffi::close(self.raw_fd) };

        if res < 0 {
            // Note! Mio logs the error but does not panic!
            let err = io::Error::last_os_error();
            println!("ERROR: {err:?}");
        }
    }
}
