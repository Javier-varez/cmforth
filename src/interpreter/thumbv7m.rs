use crate::{
    interpreter::ForthContext,
    types::{Address, Word},
};

unsafe extern "C" {
    /// This function executes the forth interpreter, written in assembly
    pub fn enter_forth(host_context: *mut ForthContext) -> super::ExitReason;

    #[link_name = "initial_latest"]
    static asm_initial_latest: Address;

    #[link_name = "forth_exit_fn"]
    static asm_exit_fn: Word;

    #[link_name = "do_word"]
    static asm_do_word: Word;

    #[link_name = "forth_lit"]
    static asm_forth_lit: Word;
}

pub fn initial_latest() -> Address {
    unsafe { asm_initial_latest }
}

pub fn exit_fn() -> Address {
    (unsafe { &asm_exit_fn }) as *const _ as Address
}

pub fn initial_lr() -> Address {
    unsafe { &asm_do_word as *const _ as Address }
}

pub fn forth_lit() -> Address {
    unsafe { &asm_forth_lit as *const _ as Address }
}

#[repr(C)]
pub struct HostSaveContext {
    pub sp: u32,
}

impl HostSaveContext {
    pub fn new() -> Self {
        Self { sp: 0 }
    }
}

const FORTH_CONTEXT_DSP: usize = core::mem::offset_of!(ForthContext, dsp.ptr);
const FORTH_CONTEXT_RSP: usize = core::mem::offset_of!(ForthContext, rsp.ptr);
const FORTH_CONTEXT_IP: usize = core::mem::offset_of!(ForthContext, ip);
const FORTH_CONTEXT_LR: usize = core::mem::offset_of!(ForthContext, lr);
const FORTH_CONTEXT_HERE: usize = core::mem::offset_of!(ForthContext, cpa.ptr);
const FORTH_CONTEXT_STATE: usize = core::mem::offset_of!(ForthContext, variables.state);
const FORTH_CONTEXT_LATEST: usize = core::mem::offset_of!(ForthContext, variables.latest);
const FORTH_CONTEXT_BASE: usize = core::mem::offset_of!(ForthContext, variables.base);
const FORTH_CONTEXT_S0: usize = core::mem::offset_of!(ForthContext, dsp.top);
const FORTH_CONTEXT_R0: usize = core::mem::offset_of!(ForthContext, rsp.top);
const HOST_SAVE_CONTEXT_SP: usize = core::mem::offset_of!(ForthContext, host_ctx.sp);

// Register allocation
// - sp - data stack pointer
// - r4 - instruction pointer
// - r5 - return stack pointer
core::arch::global_asm! {
    r#"
    .macro next
        ldr r6, [r4], #4
        ldr pc, [r6]
    .endm

    .macro pushrsp reg
        str \reg, [r5, #-4]!
    .endm

    .macro poprsp reg
        ldr \reg, [r5], #4
    .endm

        .text
        .align 2
        .thumb_func
        .global docol
    docol:
        pushrsp r4
        add r4, r6, #4
        next

    .set f_immed,0x80
    .set f_hidden,0x20
    .set f_lenmask,0x1f

    .macro exit_forth constant
        mov r0, r11

        str sp, [r0, ${FORTH_CONTEXT_DSP}]
        str r5, [r0, ${FORTH_CONTEXT_RSP}]
        str r4, [r0, ${FORTH_CONTEXT_IP}]
        str r6, [r0, ${FORTH_CONTEXT_LR}]

        ldr sp, [r0, ${HOST_SAVE_CONTEXT_SP}]
        pop {{ r4, r5, r6, r7, r8, r10, r11, lr }}
        mov r0, \constant
        bx lr
    .endm

    .macro syscall syscall_name, constant
        ldr r6, =syscall_ret_\syscall_name
        exit_forth \constant

        .thumb_func
        .type syscall_ret_\syscall_name , %function
    syscall_ret_\syscall_name :
        // Code following the syscall
    .endm

    .set link, 0

    .macro defheader name, namelen, label, flags=0
        .text
        .align 2
        .global word_forth_\label
    word_forth_\label :
        .4byte link  // Setting the link to the previously-defined word. This is done at compile-time, as link is redefined
        .set link, word_forth_\label
        .byte \flags + \namelen
        .ascii "\name"
        .align 2
        .global forth_\label
    forth_\label :
    .endm

    .macro defword name, namelen, label, flags=0
        defheader "\name", \namelen, \label, \flags
        .4byte docol // Interpreter for the word
        // Actual code of the word
    .endm

    .macro defcode name, namelen, label, flags=0
        defheader "\name", \namelen, \label, \flags
        .4byte code_forth_\label // No interpreter, jump to asm code

        .thumb_func
        .global code_forth_\label
        .type code_forth_\label , %function
    code_forth_\label :
        // Actual asm code of the word
    .endm

    .macro defconst name, namelen, label, value, flags=0
        defcode \name, \namelen, \label, \flags
        ldr r0, =\value
        push {{ r0 }}
        next
    .endm

    .macro defvar name, namelen, label, flags=0, offset
        defcode \name, \namelen, \label, \flags
        add r0, r11, #\offset
        push {{ r0 }}
        next
    .endm

    // Define all words here

    defcode "DROP", 4, drop
        pop {{ r0 }}
        next

    defcode "SWAP", 4, swap
        pop {{ r1, r2 }}
        mov r0, r2
        push {{ r0, r1 }}
        next

    defcode "DUP", 3, dup
        ldr r0, [sp]
        push {{ r0 }}
        next

    defcode "OVER", 4, over
        ldr r0, [sp, #4]
        push {{ r0 }}
        next

    defcode "ROT", 3, rot
        pop {{ r1, r2, r3 }}
        mov r0, r3
        push {{ r0, r1, r2 }}
        next

    defcode "-ROT", 4, neg_rot
        pop {{ r0, r1, r2 }}
        mov r3, r0
        push {{ r1, r2, r3 }}
        next

    defcode "2DROP", 5, two_drop
        pop {{ r0 }}
        pop {{ r0 }}
        next

    defcode "2DUP", 4, two_dup
        ldr r0, [sp]
        ldr r1, [sp, #4]
        push {{ r0, r1 }}
        next

    defcode "2SWAP", 5, two_swap
        pop {{ r0, r1, r2, r3 }}
        push {{ r0, r1 }}
        push {{ r2, r3 }}
        next

    defcode "?DUP", 4, qdup
        ldr r0, [sp]
        cmp r0, #0
        it ne
        pushne {{ r0 }}
        next

    defcode "1+", 2, incr
        ldr r0, [sp]
        add r0, #1
        str r0, [sp]
        next

    defcode "1-", 2, decr
        ldr r0, [sp]
        sub r0, #1
        str r0, [sp]
        next

    defcode "4+", 2, incr4
        ldr r0, [sp]
        add r0, #4
        str r0, [sp]
        next

    defcode "4-", 2, decr4
        ldr r0, [sp]
        sub r0, #4
        str r0, [sp]
        next

    defcode "+", 1, add
        pop {{ r0, r1 }}
        add r0, r1
        push {{ r0 }}
        next

    defcode "-", 1, sub
        pop {{ r0, r1 }}
        sub r1, r0
        push {{ r1 }}
        next

    defcode "*", 1, mult
        pop {{ r0, r1 }}
        mul r0, r1
        push {{ r0 }}
        next

    defcode "/MOD", 4, divmod
        pop {{ r1, r2 }}
        sdiv r0, r2, r1
        mls r1, r0, r1, r2
        push {{ r0, r1 }}
        next

    defcode "=", 1, equ
        pop {{ r0, r1 }}
        sub r1, r0, r1
        negs r0, r1
        adc r0, r0, r1
        push {{ r0 }}
        next

    defcode "<>", 2, nequ
        pop {{ r0, r1 }}
        subs r0, r1
        it ne
        movne r0, #1
        push {{ r0 }}
        next

    defcode "<", 1, less_than
        pop {{ r0, r1 }}
        subs r0, r1
        ite gt
        movgt r0, #1
        movle r0, #0
        push {{ r0 }}
        next

    defcode ">", 1, greater_than
        pop {{ r0, r1 }}
        subs r0, r1
        ite lt
        movlt r0, #1
        movge r0, #0
        push {{ r0 }}
        next

    defcode "<=", 2, less_eq
        pop {{ r0, r1 }}
        subs r0, r1
        ite ge
        movge r0, #1
        movlt r0, #0
        push {{ r0 }}
        next

    defcode ">=", 2, greater_eq
        pop {{ r0, r1 }}
        subs r0, r1
        ite le
        movle r0, #1
        movgt r0, #0
        push {{ r0 }}
        next

    defcode "0=", 2, zequ
        pop {{ r0 }}
        // (clz(r0) >> 5), is only 1 for r0 == 0
        clz r0, r0
        lsr r0, r0, #5
        push {{ r0 }}
        next

    defcode "0<>", 3, znequ
        pop {{ r0 }}
        cmp r0, #0
        it ne
        movne r0, #1
        push {{ r0 }}
        next

    defcode "0<", 2, zless_than
        pop {{ r0 }}
        cmp r0, #0
        ite lt
        movlt r0, #1
        movge r0, #0
        push {{ r0 }}
        next

    defcode "0>", 2, zgreater_than
        pop {{ r0 }}
        cmp r0, #0
        ite gt
        movgt r0, #1
        movle r0, #0
        push {{ r0 }}
        next

    defcode "0<=", 3, zless_eq
        pop {{ r0 }}
        cmp r0, #0
        ite le
        movle r0, #1
        movgt r0, #0
        push {{ r0 }}
        next

    defcode "0>=", 3, zgreater_eq
        pop {{ r0 }}
        cmp r0, #0
        ite ge
        movge r0, #1
        movlt r0, #0
        push {{ r0 }}
        next

    defcode "AND", 3, and
        pop {{ r0, r1 }}
        and r0, r1
        push {{ r0 }}
        next

    defcode "OR", 2, or
        pop {{ r0, r1 }}
        orr r0, r1
        push {{ r0 }}
        next

    defcode "XOR", 3, xor
        pop {{ r0, r1 }}
        eor r0, r1
        push {{ r0 }}
        next

    defcode "INVERT", 6, invert
        pop {{ r0 }}
        mvn r0, r0
        push {{ r0 }}
        next

    defcode "!", 1, store
        pop {{ r0, r1 }}
        str r1, [r0]
        next

    defcode "@", 1, fetch
        pop {{ r0 }}
        ldr r0, [r0]
        push {{ r0 }}
        next

    defcode "+!", 2, addstore
        pop {{ r0, r1 }}
        ldr r2, [r0]
        add r2, r1
        str r2, [r0]
        next

    defcode "-!", 2, substore
        pop {{ r0, r1 }}
        ldr r2, [r0]
        sub r2, r1
        str r2, [r0]
        next

    defcode "C!", 2, storebyte
        pop {{ r0, r1 }}
        strb r1, [r0]
        next

    defcode "C@", 2, fetchbyte
        pop {{ r0 }}
        ldrb r0, [r0]
        push {{ r0 }}
        next

    defcode "C@C", 3, ccopy
        pop {{ r0, r1 }}
        ldrb r2, [r1], #1
        strb r2, [r0], #1
        push {{ r0, r1 }}
        next

    defcode "EXIT", 4, exit
        poprsp r4
        next

    defcode "LIT", 3, lit
        ldr r0, [r4], #4
        push {{ r0 }}
        next

    defcode "KEY", 3, key
        syscall key_op, {EXIT_REASON_KEY_OP}
        next

    defcode "EMIT", 4, emit
        syscall emit, {EXIT_REASON_EMIT_OP}
        next

    defcode "WORD", 4, word
        syscall word_op, {EXIT_REASON_WORD_OP}
        next

    defcode "CHAR", 4, char
        syscall char_op, {EXIT_REASON_WORD_OP}
        pop {{ r0, r1 }}
        ldrb r1, [r1]
        push {{ r1 }}
        next

    defcode ">CFA", 4, cfa
        pop {{ r0 }}
        add r0, r0, #4
        ldrb r1, [r0]
        and r1, f_lenmask
        // align up to 4-byte boundary and skip len + flags byte
        add r1, #4
        and r1, #~3
        add r0, r1
        push {{ r0 }}
        next

    defcode ">DFA", 4, dfa
        pop {{ r0 }}
        add r0, r0, #4
        ldrb r1, [r0]
        and r1, f_lenmask
        // align up to 4-byte boundary and skip len + flags byte + codeword
        add r1, #8
        and r1, #~3
        add r0, r1
        push {{ r0 }}
        next

    // This is a debug helper for bringing up the interpreter. Won't be used afterwards
    defcode ".", 1, print
        syscall dot_op, {EXIT_REASON_DOT_OP}
        next

    defcode "CREATE", 6, create
        // Find where our word will be located
        ldr r0, [r11, #{FORTH_CONTEXT_HERE}]
        ldr r1, [r11, #{FORTH_CONTEXT_LATEST}]

        // Create the link
        str r1, [r0], #4

        pop {{ r2, r3 }} // read name length (r0) and base (r1)
        strb r2, [r0], #1 // store length
        // Copy name
        cmp r2, #0
    1:
        beq 2f
        ldrb r1, [r3], #1
        strb r1, [r0], #1
        subs r2, #1
        b 1b
    2:  // Pad alignment
        add r0, #3
        and r0, #~3

        // Update here and latest
        add r1, r11, #{FORTH_CONTEXT_HERE}
        add r2, r11, #{FORTH_CONTEXT_LATEST}

        ldr r3, [r1]
        str r3, [r2]
        str r0, [r1]
        next

    defcode ",", 1, comma
        pop {{ r0 }}
        add r1, r11, #{FORTH_CONTEXT_HERE}
        ldr r2, [r1]
        str r0, [r2], #4
        str r2, [r1]
        next

    defcode "[", 1, lbrace, f_immed
        mov r0, #0
        str r0, [r11, #{FORTH_CONTEXT_STATE}]
        next

    defcode "]", 1, rbrace
        mov r0, #1
        str r0, [r11, #{FORTH_CONTEXT_STATE}]
        next

    defcode "HIDDEN", 6, hidden
        pop {{ r0 }}
        ldrb r1, [r0, #4]
        eor r1, #f_hidden
        strb r1, [r0, #4]
        next

    defcode "IMMEDIATE", 9, immediate, f_immed
        ldr r0, [r11, #{FORTH_CONTEXT_LATEST}]
        ldr r1, [r0, #4]
        eor r1, #f_immed
        str r1, [r0, #4]
        next

    // defined as immediate so that it works in compiled code
    defword "'", 1, tick, f_immed
        .4byte forth_word // ( addr n )
        // Tuck the length of the string
        .4byte forth_swap // ( n addr )
        .4byte forth_over // ( n addr n )
        .4byte forth_find // ( n newaddr result )
        .4byte forth_zbranch // ( n newaddr )
    1:  .4byte 4f - 1b
        .4byte forth_swap // ( newaddr n )
        .4byte forth_drop // ( newaddr )
        .4byte forth_cfa // ( codeword )
        .4byte forth_state
        .4byte forth_fetch
        .4byte forth_zbranch
    2:  .4byte 3f - 2b
        // Compile word
        .4byte forth_lit
        .4byte forth_lit
        .4byte forth_comma
        .4byte forth_comma
    3:  .4byte forth_exit
        // word not found
    4:  .4byte forth_lit
        .4byte tick_err_str
        .4byte forth_lit
        .4byte tick_err_str_end - tick_err_str
        .4byte forth_tell
        .4byte forth_swap
        .4byte forth_tell
        .4byte forth_lit
        .4byte '\n'
        .4byte forth_emit
        .4byte forth_quit

    tick_err_str:
        .ascii "' Could not find word: "
    tick_err_str_end:

    defword ":", 1, colon
        .4byte forth_word
        .4byte forth_create

        .4byte forth_lit
        .4byte docol
        .4byte forth_comma

        .4byte forth_latest
        .4byte forth_fetch
        .4byte forth_hidden

        .4byte forth_rbrace
        .4byte forth_exit

    defword ";", 1, semicolon, f_immed
        .4byte forth_lit
        .4byte forth_exit
        .4byte forth_comma

        .4byte forth_latest
        .4byte forth_fetch
        .4byte forth_hidden

        .4byte forth_lbrace
        .4byte forth_exit

    // QUIT is a special word that resets the return stack pointer and re-enters the interpreter.
    // It can be called from anywhere .
    defword "QUIT", 4, quit
        .4byte forth_exit_fn

    defcode "RSPSTORE", 9, rspstore
        pop {{ r5 }}
        next

    defcode "BRANCH", 6, branch
        ldr r0, [r4]
        add r4, r0
        next

    defcode "0BRANCH", 7, zbranch
        pop {{ r0 }}
        cmp r0, #0
        itte eq
        // Branch taken
        ldreq r0, [r4]
        addeq r4, r0
        // Branch not taken
        addne r4, #4 // Skip offset
        next

    defcode "FIND", 4, find
        syscall find, {EXIT_REASON_FIND_OP}
        next

    defvar "LATEST", 6, latest, 0, {FORTH_CONTEXT_LATEST}
    defvar "HERE", 4, here, 0, {FORTH_CONTEXT_HERE}
    defvar "STATE", 5, state, 0, {FORTH_CONTEXT_STATE} // 0 -> Immediate mode. 1 -> Compilation mode
    defvar "BASE", 4, base, 0, {FORTH_CONTEXT_BASE}
    defvar "S0", 2, sz, 0, {FORTH_CONTEXT_S0}

    .set forth_version, 48

    defconst "VERSION", 7, version, forth_version
    defconst "F_IMMED", 7, __f_immed, f_immed
    defconst "F_HIDDEN", 8, __f_hidden, f_hidden
    defconst "F_LENMASK", 9, __f_lenmask, f_lenmask
    defconst "DOCOL", 5, __docol, docol

    defcode "R0", 2, rz, 0
        ldr r0, [r11, #{FORTH_CONTEXT_R0}]
        push {{ r0 }}
        next

    defcode ">R", 2, to_rsp
        pop {{ r0 }}
        pushrsp r0
        next

    defcode "R>", 2, from_rsp
        poprsp r0
        push {{ r0 }}
        next

    defcode "RSP@", 4, fetch_rsp
        push {{ r5 }}
        next

    defcode "RSP!", 4, store_rsp
        pop {{ r5 }}
        next

    defcode "RDROP", 5, rdrop
        poprsp r0 // throw away the result
        next

    defcode "DSP@", 4, fetch_dsp
        mov r0, sp
        push {{ r0 }}
        next

    defcode "DSP!", 4, store_dsp
        pop {{ r0 }}
        mov sp, r0
        next

    defcode "LITSTRING", 9, litstring
        ldr r0, [r4], #4 // Get length
        mov r1, r4 // Get ptr to string
        // r2 <- alignup(r0, 4)
        add r2, r0, #0x03
        and r2, r2, #~0x03
        add r4, r2
        push {{ r0, r1 }}
        next

    defcode "TELL", 4, tell
        syscall tell, {EXIT_REASON_TELL_OP}
        next

    defcode "EXECUTE", 7, execute
        pop {{ r6 }}
        ldr pc, [r6]

    defcode "_EXITFN", 7, exit_fn
        exit_forth {EXIT_REASON_SUCCESS}

        .rodata
        .align 2
        .global initial_latest
    initial_latest:
        .4byte link

        .text
        .align 2
        .thumb_func
        .global enter_forth
        .type enter_forth, %function
    enter_forth:
        // Save execution context to the current stack
        // TODO(javier): Not all of these are actually used. Save some work
        push {{ r4, r5, r6, r7, r8, r10, r11, lr }}
        // TODO(javier): save FP unit context if used
        str sp, [r0, #{HOST_SAVE_CONTEXT_SP}]

        mov r11, r0

        // Restore forth context
        ldr sp, [r0, #{FORTH_CONTEXT_DSP}]
        ldr r5, [r0, #{FORTH_CONTEXT_RSP}]
        ldr r4, [r0, #{FORTH_CONTEXT_IP}]
        ldr pc, [r0, #{FORTH_CONTEXT_LR}]

        .thumb_func
        .global do_word
        .type do_word, %function
    do_word:
        next
    "#,

    HOST_SAVE_CONTEXT_SP = const HOST_SAVE_CONTEXT_SP,

    FORTH_CONTEXT_DSP = const FORTH_CONTEXT_DSP,
    FORTH_CONTEXT_RSP = const FORTH_CONTEXT_RSP,
    FORTH_CONTEXT_IP = const FORTH_CONTEXT_IP,
    FORTH_CONTEXT_LR = const FORTH_CONTEXT_LR,
    FORTH_CONTEXT_LATEST = const FORTH_CONTEXT_LATEST,
    FORTH_CONTEXT_HERE = const FORTH_CONTEXT_HERE,
    FORTH_CONTEXT_STATE = const FORTH_CONTEXT_STATE,
    FORTH_CONTEXT_BASE = const FORTH_CONTEXT_BASE,
    FORTH_CONTEXT_S0 = const FORTH_CONTEXT_S0,
    FORTH_CONTEXT_R0 = const FORTH_CONTEXT_R0,

    EXIT_REASON_SUCCESS = const super::ExitReason::Success as Word,
    EXIT_REASON_DOT_OP = const super:: ExitReason::DotOp as Word,
    EXIT_REASON_WORD_OP = const super:: ExitReason::WordOp as Word,
    EXIT_REASON_FIND_OP = const super:: ExitReason::FindOp as Word,
    EXIT_REASON_KEY_OP = const super:: ExitReason::KeyOp as Word,
    EXIT_REASON_TELL_OP = const super:: ExitReason::TellOp as Word,
    EXIT_REASON_EMIT_OP = const super:: ExitReason::EmitOp as Word,
}
