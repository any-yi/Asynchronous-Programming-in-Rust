use std::{
    marker::PhantomPinned,
    pin::{pin, Pin},
};

fn main() {
    heap_pinning();
    //stack_pinning_manual();
    //stack_pinning_manual_problem();
    //stack_pinning_macro();
    //pin_projection();
}

fn heap_pinning() {
    let mut x = Box::pin(MaybeSelfRef::default());
    x.as_mut().init();
    println!("{}", x.as_ref().a);
    *x.as_mut().b().unwrap() = 2;
    println!("{}", x.as_ref().a);
}

fn stack_pinning_manual() {
    let mut x = MaybeSelfRef::default();
    let mut x = unsafe { Pin::new_unchecked(&mut x) };
    x.as_mut().init();
    println!("{}", x.as_ref().a);
    *x.as_mut().b().unwrap() = 2;
    println!("{}", x.as_ref().a);
}

use std::mem::swap;
fn stack_pinning_manual_problem() {
    let mut x = MaybeSelfRef::default();
    let mut y = MaybeSelfRef::default();

    {
        let mut x = unsafe { Pin::new_unchecked(&mut x) };
        x.as_mut().init();
        *x.as_mut().b().unwrap() = 2;
    }
    // 退出作用域，x 不再是被 pin 的，而不会一直延续到程序结束。
    // 且在块里的修改在块外依然有效。
    swap(&mut x, &mut y);
    println!("
     x: {{
  +----->a: {:p},
  |      b: {:?},
  |  }}
  |
  |  y: {{
  |      a: {:p},
  +-----|b: {:?},
     }}",
        &x.a,
        x.b,
        &y.a,
        y.b,
    );
}

fn stack_pinning_macro() {
    let mut x = pin!(MaybeSelfRef::default());
    MaybeSelfRef::init(x.as_mut());
    println!("{}", x.as_ref().a);
    *x.as_mut().b().unwrap() = 2;
    println!("{}", x.as_ref().a);
}

fn pin_projection() {
    #[derive(Default)]
    struct Foo {
        a: MaybeSelfRef,
        b: String,
    }

    impl Foo {
        /// 结构性 Pin 映射：传入一个被 Pin 了的结构体可变引用，返回的字段必须也被 Pin（返回一个同样被 Pin 的可变引用）
        fn a(self: Pin<&mut Self>) -> Pin<&mut MaybeSelfRef> {
            unsafe {
                self.map_unchecked_mut(|s| &mut s.a)
            }
        }

        /// 非结构性 Pin 映射：传入一个被 Pin 了的结构体可变引用，返回的字段可以被 move（返回值的可变引用）
        fn b(self: Pin<&mut Self>) -> &mut String {
            unsafe {
                &mut self.get_unchecked_mut().b
            }
        }
    }
}

// 拥有 PhantomPinned 字段的结构体自动实现 !Unpin trait
#[derive(Default, Debug)]
struct MaybeSelfRef {
    a: usize,
    b: Option<*mut usize>,
    _pin: PhantomPinned,
}

/// 移动就是转移所有权，比如把一个变量赋值给另一个变量，则原先的变量就不可访问了。
/// 想把结构体变量(引用)*移动*到其他地方去，则该变量(引用)必须是可变的（或拥有所有权），
/// 否则新变量连可变性都不能获取到，就更不符合移动是转移所有权的定义。
///
/// Pin 是一个智能指针，包裹一个值，这个 Pin 指针无法 safe 的获取到 Pin 包裹的值的 **可变引用** ，
/// 也即无法拆包 Pin 后得到一个关于原值的可变变量(引用) 。
///
/// 至于为什么无法 safe 的获取到可变解引用（Pin 是智能指针，拆包和解引用是相同的操作），Pin 的 DerefMut 是这么写的：
/// ```rust
/// impl<P: DerefMut<Target: Unpin>> DerefMut for Pin<P>{...}
/// ```
/// 被包裹的值如果是 !Unpin ，不符合这个签名（泛型 P 限定为：解引用 Target 为 Unpin 的可变引用类型，
/// 这个限定如果用在结构体上，就是需要保证整个结构体的所有字段都实现了 Unpin ，
/// 而实参 P 中的 PhantomPinned 字段取消实现了 Unpin ，因此编译器报错），
/// 代码编译不过。
impl MaybeSelfRef {
    // 只有调用者是被 Pin 包裹的 该结构体的实例的可变引用 时，才能调用本方法。
    fn init(self: Pin<&mut Self>) {
        unsafe {
            let Self { a, b, .. } = self.get_unchecked_mut();
            *b = Some(a);
        }
    }

    // 只有调用者是被 Pin 包裹的 该结构体的实例的可变引用 时，才能调用本方法。
    // 通过 b 返回 a 的可变引用
    fn b(self: Pin<&mut Self>) -> Option<&mut usize> {
        unsafe { self.get_unchecked_mut().b.map(|b| &mut *b) }
    }
}
