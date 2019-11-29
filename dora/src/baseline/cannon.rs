use self::codegen::CannonCodeGen;

use crate::baseline::asm::BaselineAssembler;
use crate::baseline::codegen::fct_pattern_match;
use crate::baseline::fct::JitBaselineFct;
use crate::bytecode::astgen::generate_fct;
use crate::ty::TypeList;
use crate::vm::{Fct, FctSrc, VM};

mod codegen;

pub(super) fn compile<'a, 'ast: 'a>(
    vm: &'a VM<'ast>,
    fct: &Fct<'ast>,
    src: &'a mut FctSrc,
    cls_type_params: &TypeList,
    fct_type_params: &TypeList,
) -> JitBaselineFct {
    let bytecode = generate_fct(vm, fct, src, cls_type_params, fct_type_params);

    if should_emit_bytecode(vm, fct) {
        bytecode.dump();
    }

    CannonCodeGen::new(
        vm,
        &fct,
        fct.ast,
        BaselineAssembler::new(vm),
        src,
        &bytecode,
        None,
        None,
        Vec::new(),
        None,
        None,
        None,
        cls_type_params,
        fct_type_params,
    )
    .generate()
}

fn should_emit_bytecode(vm: &VM, fct: &Fct) -> bool {
    if let Some(ref dbg_names) = vm.args.flag_emit_bytecode {
        fct_pattern_match(vm, fct, dbg_names)
    } else {
        false
    }
}
