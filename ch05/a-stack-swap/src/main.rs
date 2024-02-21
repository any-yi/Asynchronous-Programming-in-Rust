use core::arch::asm;

const SSIZE: isize = 48;

#[derive(Debug, Default)]
#[repr(C)]
struct ThreadContext {
    rsp: u64,
}

fn hello() -> ! {
    println!("I LOVE WAKING UP ON A NEW STACK!");
    loop {}
}

unsafe fn gt_switch(new: *const ThreadContext) {
    asm!(
        "mov rsp, [{0} + 0x00]",
        "ret",
        in(reg) new,
    );
}

fn main() {
    let mut ctx = ThreadContext::default();
    let mut stack = vec![0_u8; SSIZE as usize];

    unsafe {
        // 数组末尾是栈底
        let stack_bottom = stack.as_mut_ptr().offset(SSIZE);
        // 关闭了栈底指针的低 4 位，也就是让栈底地址的低 4 位变为 0
        // 因为 x86‑64 上的堆栈对齐是 16 字节，因此这是按 16 字节对齐（因为 2^4=16 ，且单位为 u8 （字节））
        let sb_aligned = (stack_bottom as usize & !15) as *mut u8;
        // 栈底指针往上指一个单位（16字节），在该单位下保存函数指针
        std::ptr::write(sb_aligned.offset(-16) as *mut u64, hello as u64);
        // 保存当前栈顶指针
        // 注意 sb_aligned 本身此时仍是栈底指针
        ctx.rsp = sb_aligned.offset(-16) as u64;
        // 把（在内存中）保存的栈顶指针复制到 rsp 寄存器，切换到我们创建的栈空间，
        // 调用 ret 返回，这会把栈保存的东西（函数指针）弹出栈到 rip 寄存器，之后将执行函数
        gt_switch(&mut ctx);
    }
}
