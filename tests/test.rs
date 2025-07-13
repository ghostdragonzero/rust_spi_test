#![no_std]
#![no_main]
#![feature(used_with_arg)]

extern crate alloc;
extern crate bare_test;


#[bare_test::tests]
mod tests {
    use bare_test::irq::{IrqHandleResult, IrqParam};
    use bare_test::println;
    use bare_test::{
        globals::{PlatformInfoKind, global_val},
        mem::iomap,GetIrqConfig};
    use log::info;
    use pl101::Pl011Uart;
    use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
pub struct Mutex<T> {
    inner:AtomicBool,
    data:UnsafeCell<T>,
}

unsafe impl<T> Sync for Mutex<T> {}
unsafe impl<T> Send for Mutex<T> {}

impl<T> Mutex<T> {
    pub const fn new(data: T) -> Self {
        Self {
            inner: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, T> {
        while self.inner.swap(true, Ordering::Acquire) {}
        MutexGuard { mutex: self }
    }    
    
    pub fn unlock(&self) {
        self.inner.store(false, Ordering::Release);
    }

    #[allow(clippy::mut_from_ref)]
    unsafe fn force_use(&self) -> &mut T {
        unsafe { &mut *self.data.get()}
    }

}

pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

impl<'a, T> core::ops::Deref for MutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<'a, T> core::ops::DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        self.mutex.inner.store(false, Ordering::Release);
    }
}

    static UART:Mutex<Option<Pl011Uart>> = Mutex::new(None);
    //先创建一个为空的锁

    #[test]
    fn it_works() {
        info!("This is a test log message.");
        let a = 2;
        let b = 2;
        assert_eq!(a + b, 4);
        let PlatformInfoKind::DeviceTree(fdt) = &global_val().platform_info;
        let fdt = fdt.get();
        let node = fdt.find_compatible(&["arm,pl011"]).next().unwrap();
        let reg = node.reg().unwrap().next().unwrap();
        let interrupt = node.irq_info().unwrap();
        let cfg = interrupt.cfgs[0].clone();
        println!("PL011 IRQ:{:?}", interrupt);
        let base = reg.address;
        let mut mmio = iomap((base as usize).into(), reg.size.unwrap());
        let mut pl011_1 = Pl011Uart::new(unsafe { mmio.as_mut() });
        {
            let mut uart = UART.lock();
            *uart = Some(pl011_1);
            //初始化mutx 锁
        }
        
        IrqParam{
            intc: interrupt.irq_parent,
            cfg,
        }.register_builder({
            |_irq| {
                unsafe {
                    UART.force_use().as_mut().unwrap().handle_interrupt();
                }
                IrqHandleResult::Handled
                //避免死锁
            }
        }).register();
    
        //这里会自动调用drap 释放锁
        {
            let mut uart = UART.lock();
            let pl011 = uart.as_mut().unwrap();
            //println!("PL011 base address {:p}", pl011.base);
            pl011.init();

            for &c in b"Hello, world!\n".iter() {
                pl011.putchar(c);
            }
        }
        
        println!("test passed!");
    }
}
