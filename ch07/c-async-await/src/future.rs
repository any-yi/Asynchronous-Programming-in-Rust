pub trait Future {
    type Output;
    fn poll(&mut self) -> PollState<Self::Output>;
}

pub enum PollState<T> {
    Ready(T),
    NotReady,
}

pub fn join_all<F: Future>(futures: Vec<F>) -> JoinAll<F> {
    let futures = futures.into_iter().map(|f| (false, f)).collect();
    JoinAll {
        futures,
        finished_count: 0,
    }
}

pub struct JoinAll<F: Future> {
    futures: Vec<(bool, F)>,   // bool 记录该 future 是否已完成
    finished_count: usize,
}

impl<F: Future> Future for JoinAll<F> {
    type Output = String;

    fn poll(&mut self) -> PollState<Self::Output> {
        // 把 JoinAll 对象中的 futures 拆包并遍历
        for (finished, fut) in self.futures.iter_mut() {
            if *finished {
                continue;
            }

            match fut.poll() {
                PollState::Ready(_) => {
                    // 如果 Ready 则更新 JoinAll 对象内部的状态
                    *finished = true;
                    self.finished_count += 1;
                }

                // 如果 future 未 Ready 则 continue 而不是 break
                PollState::NotReady => continue,
            }
        }

        // 只有所有 future 都完成了整个 poll 过程才返回 Ready
        if self.finished_count == self.futures.len() {
            PollState::Ready(String::new())
        } else {
            PollState::NotReady
        }
    }
}
