use core::ptr::NonNull;

use tock_registers::{
    interfaces::{Readable, Writeable},
     register_bitfields, register_structs,
    registers::{ReadOnly, ReadWrite, WriteOnly},
};
use futures::task::AtomicWaker;

register_structs! {
    /// Pl011 registers.
    Pl011UartRegs {
        /// Data Register.
        (0x00 => dr: ReadWrite<u32>),
        (0x04 => _reserved0),
        /// Flag Register.
        (0x18 => fr: ReadOnly<u32>),
        (0x1c => _reserved1),
        /// Control register.
        (0x24 => tibd: ReadWrite<u32>),
        ///
        (0x28 => tfbd: ReadWrite<u32>),
        (0x2c => cr_h: ReadWrite<u32, CONTROLH::Register>),
        (0x30 => cr_l: ReadWrite<u32, CONTROLL::Register>),
        /// Interrupt FIFO Level Select Register.
        (0x34 => ifls: ReadWrite<u32>),
        /// Interrupt Mask Set Clear Register.
        (0x38 => imsc: ReadWrite<u32>),
        /// Raw Interrupt Status Register.
        (0x3c => ris: ReadOnly<u32>),
        /// Masked Interrupt Status Register.
        (0x40 => mis: ReadOnly<u32>),
        /// Interrupt Clear Register.
        (0x44 => icr: WriteOnly<u32>),
        (0x48 => @END),
    }
}
register_bitfields![u32,
    CONTROLH [
        BRK OFFSET(0) NUMBITS(1) [],
        PEN OFFSET(1) NUMBITS(1) [],
        EPS OFFSET(2) NUMBITS(1) [],
        STP2 OFFSET(3) NUMBITS(1) [],
        FEN OFFSET(4) NUMBITS(1) [],
        WLEN OFFSET(5) NUMBITS(2) [
            len5 = 0,
            len6 = 1,
            len7 = 2,
            len8= 3
        ],
        SPS OFFSET(7) NUMBITS(1) [],
    ],
    CONTROLL [
        ENABLE OFFSET(0) NUMBITS(1) [],
        RSV OFFSET(1) NUMBITS(7) [],
        TXE OFFSET(8) NUMBITS(1) [],
        RXE OFFSET(9) NUMBITS(1) [],
    ],
];

/// The Pl011 Uart
///
/// The Pl011 Uart provides a programing interface for:
/// 1. Construct a new Pl011 UART instance
/// 2. Initialize the Pl011 UART
/// 3. Read a char from the UART
/// 4. Write a char to the UART
/// 5. Handle a UART IRQ
pub struct Pl011Uart {
    pub base: NonNull<Pl011UartRegs>,
    waker: AtomicWaker,
    pub irq_count:usize,
}

unsafe impl Send for Pl011Uart {}
unsafe impl Sync for Pl011Uart {}

pub struct WriteFuture<'a> {
    pl011: &'a mut Pl011Uart,
    bytes:&'a [u8],
    n:usize,
}

impl<'a> Future for WriteFuture<'a> {
    type Output = usize;
    fn poll(mut self: core::pin::Pin<&mut Self>, _cx: &mut core::task::Context<'_>) -> core::task::Poll<usize> {
        let this = self.get_mut();
        loop {
            if this.n >= this.bytes.len() {
                return core::task::Poll::Ready(this.n);
            }
            if this.pl011.regs().fr.get() & (1 << 5) != 0 {
                // TXFF满，不能继续发送
                let waker = _cx.waker().clone();
                this.pl011.waker.register(&waker);
                return core::task::Poll::Pending;
            }
            let byte = &this.bytes[this.n];
            this.pl011.putchar(*byte);
            this.n += 1;
        }
    }
}

impl Pl011Uart {
    /// Constrcut a new Pl011 UART instance from the base address.
    pub const fn new(base: *mut u8) -> Self {
        Self {
            base: NonNull::new(base).unwrap().cast(),
            waker:AtomicWaker::new(),
            irq_count:0,
        }
    }

    const fn regs(&self) -> &Pl011UartRegs {
        unsafe { self.base.as_ref() }
    }

    pub fn handle_interrupt(&mut self){
        self.irq_count += 1;

        if self.regs().mis.get() & (1 << 4) != 0{
            self.waker.wake();
        }
        self.ack_interrupts();
    }
    
    pub  fn write_byte<'a>(&'a mut self, bytes:&'a [u8]) -> impl Future<Output = usize> + 'a {
        //usize 写成功了多少字节
        WriteFuture{
            pl011:self,
            bytes,
            n:0,
        }

    }

    /// Initializes the Pl011 UART.
    ///
    /// It clears all irqs, sets fifo trigger level, enables rx interrupt, enables receives
    pub fn init(&mut self) {
        // clear all irqs
        self.regs().cr_l.write(CONTROLL::ENABLE::CLEAR);
        
        self.regs().tibd.set(100_000_000);
        self.regs().tfbd.set(115200);
        self.regs().cr_h.write(CONTROLH::WLEN::len8);
        // set fifo trigger level
        self.regs().ifls.set(0); // 1/8 rxfifo, 1/8 txfifo.

        // enable rx interrupt
        self.regs().imsc.set(0x7ff); // all interrupt

        // enable receive
        self.regs().cr_l.write(CONTROLL::ENABLE::SET + CONTROLL::TXE::SET + CONTROLL::RXE::SET);
    }

    /// Output a char c to data register
    pub fn putchar(&mut self, c: u8) {
        while (self.regs().fr.get() & (1 << 3) != 0 ) {}
        self.regs().dr.set(c as u32);
    }

    /// Return a byte if pl011 has received, or it will return `None`.
    pub fn getchar(&mut self) -> u8 {
        while self.regs().fr.get() & (1 << 4) != 0{}
        (self.regs().dr.get() & 0xff) as u8
    }

    /// Return true if pl011 has received an interrupt
    pub fn is_receive_interrupt(&self) -> bool {
        let pending = self.regs().mis.get();
        pending & (1 << 4) != 0
    }


    /// Clear all interrupts
    pub fn ack_interrupts(&mut self) {
        self.regs().icr.set(0x7ff);
    }
    
}
