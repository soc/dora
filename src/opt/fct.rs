use crate::ctxt::FctId;
use crate::gc::Address;

pub struct JitOptFct {
    pub fct_id: FctId,
    pub fct_start: Address,
}

impl JitOptFct {
    pub fn fct_id(&self) -> FctId {
        self.fct_id
    }

    pub fn fct_ptr(&self) -> Address {
        self.fct_start
    }
}
