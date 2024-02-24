use std::time::Instant;

mod http;
mod future;

use future::*;
use crate::http::Http;

coroutine fn request(i: usize) {
    let path = format!("/{}/HelloWorld{i}", i * 1000);
    let txt = Http::get(&path).wait;
    println!("{txt}");
}

/// 标有 coroutine 的函数的返回类型将被重写为 `impl Future<Output=String>`
coroutine fn async_main() {
    println!("Program starting");
    let mut futures = vec![];

    for i in 0..5 {
        futures.push(request(i));
    }

    // 调用 join_all 函数，返回一个 JoinAll 结构体对象（Future 对象），
    // .wait 关键字调用将使生成的状态机代码中自动 poll 刚刚返回的 Future 对象，
    // 也即调用 JoinAll 结构体对象的 poll 方法。
    //
    // 只有实现 Future trait 的对象才可以使用 .wait 作为后缀
    future::join_all(futures).wait;
}


fn main() {
    let start = Instant::now();
    // 由于 async_main 的 coroutine 标记，将生成一个 future 对象，
    // 在其 poll 方法中调用上述 JoinAll 结构体对象的 poll 方法。
    let mut future = async_main();

    loop {
        match future.poll() {
            PollState::NotReady => (),
            PollState::Ready(_) => break,
        }
    }

    println!("\nELAPSED TIME: {}", start.elapsed().as_secs_f32());
}
