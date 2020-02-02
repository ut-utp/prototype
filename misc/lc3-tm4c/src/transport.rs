enum State {
    WaitingForMessageType,
    ControlMessageLengthUnknown,
    ConsoleInputLengthUnknown,
    ReadingControlMessage(u16),
    ReadingConsoleInput(u16),
}

pub struct Fifo<T> {
    data: [T; Self::CAPACITY], // Pick this so that we can read in the largest message. Also have compile time asserts that check that all the messages fit within a buffer of this size! (TODO) (shouldn't be too bad since we have one message...)
    length: usize,
    starting: Self::Cur,
    ending: Self::Cur,   // Points to the next empty slot.
}

impl<T> Fifo<T> {
    const CAPACITY: usize = 256; // TODO: compile time assert that this is in [1, Self::Cur::MAX].
    type Cur = u16;

    pub const fn new() -> Self {
        Self {
            data: [0; Self::CAPACITY],
            length: 0,
            starting: 0,
            ending: 0,
        }
    }

    pub fn is_empty(&self) -> bool { self.length == 0 }
    pub fn is_full(&self) -> bool { self.length == Self::CAPACITY }

    pub fn length(&self) -> usize { self.length }
    pub fn remaining(&self) -> usize { Self::CAPACITY - self.length }

    // fn increment(pos: Self::Cur) -> Self::Cur {
    //     if pos == ((Self::CAPACITY - 1) as Self::Cur) {
    //         0
    //     } else {
    //         pos + 1
    //     }
    // }

    fn add(pos: Self::Cur, num: Self::Cur) -> Self::Cur {
        (((pos as usize) + (num as usize)) % Self::CAPACITY) as Self::Cur
    }

    pub fn push(&mut self, datum: T) -> Result<(), ()> {
        if self.is_full() {
            Err(())
        } else {
            self.length += 1;
            self.data[self.ending as usize] = datum;
            self.ending = self.add(self.ending, 1);

            Ok(())
        }
    }

    pub fn peek(&self) -> Result<T, ()> {
        if self.is_empty() {
            Err(())
        } else {
            Ok(self.data[self.starting as usize])
        }
    }

    pub fn pop(&mut self) -> Result<T, ()> {
        let datum = self.peek()?;

        self.advance(1);
        Ok(datum)
    }

    pub fn bytes(&self) -> &[T] {
        // starting == ending can either mean a full fifo or an empty one
        if self.is_empty() {
            &[]
        } else {
            if self.ending > self.starting {
                self.data[(self.starting as usize)..(self.ending as usize)]
            } else if self.ending <= self.starting {
                // Gotta do it in two parts then.
                self.data[(self.starting as usize)..]
            }
        }
    }

    fn advance(&mut self, num: Self::Cur) -> Result<(), ()> {
        assert!(num <= self.length);

        self.length -= num;
        self.starting = self.add(self.starting, num);
    }
}

impl Buf for Fifo<u8> {
    fn remaining(&self) -> usize {
        self.remaining()
    }

    fn bytes(&self) -> &[u8] {
        self.bytes()
    }

    fn advance(&mut self, count: usize) {
        use core::convert::TryInto;
        self.advance(count.try_into::<Self::Cur>().unwrap());
    }
}

static RX: Mutex<RefCell<Rx<_>>> = ..;
static RX_STATE: Mutex<RefCell<State>> = Mutex::new(RefCell::new(State::WaitingForMessageType));
static RX_BUFFER: Mutex<RefCell<Fifo>> = Mutex::new(RefCell::new(Fifo::<u8>::new()));

static CONTROL_MESSAGE_PENDING: Mutex<Cell<Option<ControlMessage>>> = Mutex::new(Cell::new(None)); // TODO: should this be more than one element big?
static CONTROL_MESSAGE_FLAG: AtomicBool = AtomicBool::new(false);

const CONTROL_MESSAGE_SLUG: u8 = 0b1000_0000;
const CONSOLE_INPUT_SLUG: u8 = 0b1100_0000;

// TODO: invoked on any new data or FIFO half full?
// any new data for now
#[interrupt]
fn uart_rx_handler() {
    interrupt_free(|cs| {
        let rx_guard = RX.lock(cs);
        let rx = rx_guard.borrow_mut();

        let rx_state_guard = RX_STATE.lock(cs);
        let rx_state = rx_state_guard.borrow_mut();

        let rx_buf_guard = RX_BUFFER.lock(cs);
        let rx_buf = rx_state_guard.borrow_mut();

        use State::*;

        while let Ok(c) = rx.read() {
            rx_state = match (rx_state, c) {
                (WaitingForMessageType, CONTROL_MESSAGE_SLUG) => ControlMessageLengthUnknown,
                (WaitingForMessageType, CONSOLE_INPUT_SLUG) => ConsoleInputLengthUnknown,
                (WaitingForMessageType, _) => panic!("unknown message type"), // TODO: how to actually handle?

                (ConsoleInputLengthUnknown, c) | (ControlMessageLengthUnknown, c) => {
                    rx_buf.push(c).unwrap(); // TODO: don't unwrap here...
                    if let Some(len) = prost::decode_length_delimiter(&mut rx_buf) { // TODO: will this behave correctly with a ref or do we need to extract and call decode_varint?
                        assert!(len <= Fifo::CAPACITY);

                        match rx_state {
                            ConsoleInputLengthUnknown => ReadingConsoleInput(len),
                            ControlMessageLengthUnknown => ReadingControlMessage(len),
                            _ => unreachable!(),
                        }
                    } else {
                        rx_state // Keep reading bytes...
                    }
                },

                (ReadingConsoleInput(1), c) => {
                    rx_buf.push(c).unwrap(); // TODO: don't unwrap...

                    // TODO! actually use the input by feeding it to the input peripheral!
                },

                (ReadingControlMessage(1), c) => {
                    rx_buf.push(c).unwrap(); // TODO: don't unwrap...

                    let m = ControlMessage::decode(&mut rx_buf).unwrap();
                    assert!(rx_buf.length() == 0);

                    let cm = CONTROL_MESSAGE_PENDING.lock(cs);
                    assert_eq!(None, cm.replace(Some(m)));

                    assert_eq!(CONTROL_MESSAGE_FLAG.load(Ordering::SeqCst), false);
                    CONTROL_MESSAGE_FLAG.store(true, Ordering::SeqCst);
                },
            }
        }

        // rx_state = match rx_state {
        //     WaitingForMessageType => {
        //         let ty: u8 = 0;
        //         rx.read(&mut ty).expect("at least one new character...");

        //         match ty {
        //             CONTROL_MESSAGE_SLUG =>
        //         }
        //     }
        // }
    })

    // TODO: acknowledge interrupt?
}

struct UartTransport {
    tx: Tx<_> // TODO
}

impl TransportLayer for UartTransport {
    fn get_message(&mut self) -> Option<ControlMessage> {
        if CONTROL_MESSAGE_FLAG.load(Ordering::SeqCst) {
            let cm = CONTROL_MESSAGE_PENDING.lock();
            let m = cm.take();

            // assert!(m.is_some()); // This invariant should be maintained, but w/e.
            m
        } else {
            None
        }
    }

    fn send_message(&mut self, message: ControlMessage) -> Result<(), ()> {
        let m_len = message.encoded_len();
        let len_len = prost::length_delimiter_len(len);

        let len = m_len + len_len;
        assert!(len <= TX_BUF_CAPACITY);

        const TX_BUF_CAPACITY: usize = 256; // TODO: again, compile time checks, etc.
        let mut buf: [u8; TX_BUF_CAPACITY] = [0; TX_BUF_CAPACITY];

        message.encode_length_delimited(&mut buf).unwrap(); // TODO: don't unwrap...

        // nb::block!(tx.)
        // TODO: maybe use DMA instead of using a blocking write here..
        tx.write(CONTROL_MESSAGE_SLUG);
        tx.write_all(buf[0..len]);
    }
}

fn main() {
    let mut sim = <snipped>;
    let mut control_client: Client<UartTransport> = <snipped>; // got to create the UART pair, split it, give the Tx to a UartTransport, and then give that UartTransport to a Client.

    // Actually we need shared access to a Tx since the Output peripheral will need access to it too.
    // So I guess UartTransport holds no state and we'll need more Mutex<RefCell<_>>s!
    // (TODO)

    loop {
        // Do simulator things, etc, etc.

        // if CONTROL_MESSAGE_FLAG.load(Ordering::SeqCst) {
        //     let cm
        // }

        control_client.step(&mut sim);
    }
}
