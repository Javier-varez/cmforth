use super::ExitReason;
use super::ForthContext;
use crate::types::{Address, Word};

pub struct HostSaveContext();

impl HostSaveContext {
    pub fn new() -> Self {
        Self()
    }
}

const MAX_NAME_SIZE: usize = 32;
const MAX_WORDS: usize =
    2 + (MAX_NAME_SIZE + core::mem::size_of::<Address>() - 1) / core::mem::size_of::<Address>();

type Handler = fn(&mut ForthContext);

struct BuiltinWordDef {
    data: [Word; MAX_WORDS],
}

impl BuiltinWordDef {
    fn new(prev: Address, name: &str, target: Handler) -> Self {
        let mut data: [Word; MAX_WORDS] = [0; MAX_WORDS];
        let mut idx = 0;
        data[idx] = prev;
        idx += 1;

        const WORD_SIZE: usize = core::mem::size_of::<Word>();
        name.as_bytes().chunks(WORD_SIZE).for_each(|chunk| {
            let word_bytes: heapless::Vec<u8, WORD_SIZE> = chunk
                .iter()
                .cloned()
                .chain(core::iter::repeat(0u8))
                .take(WORD_SIZE)
                .collect();
            let word_bytes: [u8; WORD_SIZE] = word_bytes[..]
                .try_into()
                .expect("Exactly WORD_SIZE bytes were selected");
            let word = Word::from_ne_bytes(word_bytes);
            data[idx] = word;
            idx += 1;
        });

        data[idx] = target as *const () as Address;
        Self { data }
    }
}

pub fn initial_latest() -> Address {
    todo!()
}

pub fn exit_fn() -> Address {
    todo!()
}

pub fn initial_lr() -> Address {
    0
}

pub unsafe fn enter_forth(ctx: &mut ForthContext) -> ExitReason {
    todo!();
}

pub fn forth_lit() -> Address {
    todo!();
}
