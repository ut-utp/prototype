#![macro_use]

#[doc(inline)]
pub use crate::single_test_inner;

#[doc(inline)]
pub use crate::single_test;

// Setup func runs before anything is set; teardown func runs after everything
// is checked but the order shouldn't matter.
//
// `with os` takes a MemoryDump and a starting address to use as the entrypoint
#[macro_export]
macro_rules! single_test {
    ($(|$panics:literal|)?
        $name:ident,
        $(with io peripherals: ($inp:ident, $out:ident))? $(,)?
        $(with custom peripherals: $custom_per:block -> [$custom_per_ty:ty])? $(,)?
        $(pre: |$peripherals_s:ident| $setup:block,)?
        $(prefill: { $($addr_p:literal: $val_p:expr),* $(,)?},)?
        $(prefill_expr: { $(($addr_expr:expr): $val_expr:expr),* $(,)?},)?
        insns: [ $({ $($insn:tt)* }),* $(,)?],
        $(steps: $steps:expr,)?
        regs: { $($r:tt: $v:expr),* $(,)?},
        memory: { $($addr:literal: $val:expr),* $(,)?} $(,)?
        $(post: |$peripherals_t:ident| $teardown:block)? $(,)?
        $(with os { $os:expr } @ $os_addr:expr)? $(,)?
    ) => {
    $(#[doc = $panics] #[should_panic])?
    #[test]
    fn $name() { with_larger_stack(/*Some(stringify!($name).to_string())*/ None, ||
        $crate::single_test_inner!(
            $(pre: |$peripherals_s| $setup,)?
            $(prefill: { $($addr_p: $val_p),* },)?
            $(prefill_expr: { $(($addr_expr): $val_expr),* },)?
            insns: [ $({ $($insn)* }),* ],
            $(steps: $steps,)?
            regs: { $($r: $v),* },
            memory: { $($addr: $val),* }
            $(post: |$peripherals_t| $teardown)?
            $(with os { $os } @ $os_addr)?
            $(with io peripherals: ($inp, $out))?
            $(with custom peripherals: $custom_per -> $custom_per_ty)?
        ));
    }};
}

#[macro_export]
#[doc(hidden)]
macro_rules! __perip_type {
    // ($regular:ty | io: $($_io:literal $io_ty:ty)? | custom: $($custom_ty:ty)?) => { };
    ($regular:ty | io:                      | custom:              ) => { $regular };
    ($regular:ty | io:                      | custom: $custom_ty:ty) => { $custom_ty };
    ($regular:ty | io: $_io:ident $io_ty:ty | custom:              ) => { $io_ty };
    ($regular:ty | io: $_io:ident $io_ty:ty | custom: $custom_ty:ty) => { $custom_ty };
}

#[macro_export]
macro_rules! single_test_inner {
    (   $(with io peripherals: ($inp:ident, $out:ident))? $(,)?
        $(with custom peripherals: $custom_per:block -> [$custom_per_ty:ty])? $(,)?
        $(pre: |$peripherals_s:ident| $setup:block,)?
        $(prefill: { $($addr_p:literal: $val_p:expr),* $(,)?},)?
        $(prefill_expr: { $(($addr_expr:expr): $val_expr:expr),* $(,)?},)?
        insns: [ $({ $($insn:tt)* }),* $(,)?],
        $(steps: $steps:expr,)?
        regs: { $($r:tt: $v:expr),* $(,)?},
        memory: { $($addr:literal: $val:expr),* $(,)?} $(,)?
        $(post: |$peripherals_t:ident| $teardown:block)? $(,)?
        $(with os { $os:expr } @ $os_addr:expr)? $(,)?
    ) => {{
        use $crate::{Word, Reg, Instruction, ShareablePeripheralsShim, MemoryShim};
        use $crate::{PeripheralInterruptFlags, Interpreter, InstructionInterpreterPeripheralAccess};
        use $crate::{SourceShim, new_shim_peripherals_set};

        use std::sync::Mutex;

        let flags = PeripheralInterruptFlags::new();

        type Per<'int, 'io> = $crate::__perip_type! {
            ShareablePeripheralsShim<'int, 'io>
            | io: $($inp ShareablePeripheralsShim<'int, 'io>)?
            | custom: $($custom_per_ty<'int, 'io>)?
        };

        #[allow(unused_mut)]
        let mut regs: [Option<Word>; Reg::NUM_REGS] = [None, None, None, None, None, None, None, None];
        $(regs[Into::<u8>::into($r) as usize] = Some($v);)*

        #[allow(unused_mut)]
        let mut checks: Vec<(Addr, Word)> = Vec::new();
        $(checks.push(($addr, $val));)*

        #[allow(unused_mut)]
        let mut prefill: Vec<(Addr, Word)> = Vec::new();
        $($(prefill.push(($addr_p, $val_p));)*)?
        $($(prefill.push(($addr_expr, $val_expr));)*)?

        #[allow(unused_mut)]
        let mut insns: Vec<Instruction> = Vec::new();
        $(insns.push(insn!($($insn)*));)*

        #[allow(unused)]
        let steps: Option<usize> = None;
        $(let steps: Option<usize> = Some($steps);)?

        #[allow(unused)]
        let os: Option<(MemoryShim, Addr)> = None;
        $(let os = Some(($os, $os_addr));)?

        #[allow(unused)]
        let custom_peripherals: Option<Per> = None;

        $(
            #[allow(unused)]
            let $inp = SourceShim::new();
            #[allow(unused)]
            let $out = Mutex::new(Vec<u8>);

            let (custom_peripherals, _, _): (Per, _, _) =
                new_shim_peripherals_set(&$inp, &$out);
            #[allow(unused)]
            let custom_peripherals = Some(custom_peripherals);
        )?

        $(
            let custom_peripherals = $custom_per;
            let custom_peripherals = Some(custom_peripherals);
        )?

        fn setup_func_cast<'flags, S>(func: S, _f: &'flags PeripheralInterruptFlags) -> S
        where for<'p> S: FnOnce(&'p mut Per<'flags, '_>) {
            func
        }

        fn teardown_func_cast<'flags, T>(func: T, _f: &'flags PeripheralInterruptFlags) -> T
        where for<'i> T: FnOnce(&'i Interpreter<'flags, MemoryShim, Per<'flags, '_>>) {
            func
        }

        #[allow(unused)]
        let setup_func = setup_func_cast(|_p: &mut Per| { }, &flags); // no-op if not specified
        $(let setup_func = setup_func_cast(|$peripherals_s: &mut Per| $setup, &flags);)?

        #[allow(unused)]
        let teardown_func = teardown_func_cast(|_p: &Interpreter<'_, MemoryShim, Per>| { }, &flags); // no-op if not specified
        $(let teardown_func = teardown_func_cast(|$peripherals_t: &Interpreter<'_, MemoryShim, Per>| $teardown, &flags);)?


        interp_test_runner::<'_, MemoryShim, Per, _, _>(
            prefill,
            insns,
            steps,
            regs,
            None,
            checks,
            setup_func,
            teardown_func,
            &flags,
            os,
        );
    }};
}
