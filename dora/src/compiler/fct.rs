use std::collections::{HashMap, HashSet};
use std::fmt;
use std::ptr;
use std::sync::Arc;

use crate::cpu::flush_icache;
use crate::dseg::DSeg;
use crate::gc::Address;
use crate::opt::fct::JitOptFct;
use crate::ty::TypeList;
use crate::utils::GrowableVec;
use crate::vm::VM;
use crate::vm::{ClassDef, FctId};

use dora_parser::Position;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct JitFctId(usize);

impl JitFctId {
    pub fn from(idx: usize) -> JitFctId {
        JitFctId(idx)
    }

    pub fn idx(self) -> usize {
        self.0
    }
}

impl GrowableVec<JitFct> {
    pub fn idx(&self, index: JitFctId) -> Arc<JitFct> {
        self.idx_usize(index.0)
    }
}

impl From<usize> for JitFctId {
    fn from(data: usize) -> JitFctId {
        JitFctId(data)
    }
}

pub enum JitFct {
    Base(JitBaselineFct),
    Opt(JitOptFct),
}

impl JitFct {
    pub fn fct_id(&self) -> FctId {
        match self {
            &JitFct::Base(ref base) => base.fct_id(),
            &JitFct::Opt(ref opt) => opt.fct_id(),
        }
    }

    pub fn fct_ptr(&self) -> Address {
        match self {
            &JitFct::Base(ref base) => base.fct_ptr(),
            &JitFct::Opt(ref opt) => opt.fct_ptr(),
        }
    }

    pub fn fct_end(&self) -> Address {
        match self {
            &JitFct::Base(ref base) => base.fct_end(),
            &JitFct::Opt(_) => unimplemented!(),
        }
    }

    pub fn fct_len(&self) -> usize {
        match self {
            &JitFct::Base(ref base) => base.fct_len(),
            &JitFct::Opt(_) => unimplemented!(),
        }
    }

    pub fn ptr_start(&self) -> Address {
        match self {
            &JitFct::Base(ref base) => base.ptr_start(),
            &JitFct::Opt(_) => unimplemented!(),
        }
    }

    pub fn ptr_end(&self) -> Address {
        match self {
            &JitFct::Base(ref base) => base.ptr_end(),
            &JitFct::Opt(_) => unimplemented!(),
        }
    }

    pub fn to_base(&self) -> Option<&JitBaselineFct> {
        match self {
            &JitFct::Base(ref base) => Some(base),
            &JitFct::Opt(_) => None,
        }
    }

    pub fn gcpoint_for_offset(&self, offset: i32) -> Option<&GcPoint> {
        match self {
            &JitFct::Base(ref base) => base.gcpoint_for_offset(offset),
            &JitFct::Opt(_) => unimplemented!(),
        }
    }

    pub fn get_comment(&self, offset: u32) -> Option<&[String]> {
        match self {
            &JitFct::Base(ref base) => base.get_comment(offset),
            &JitFct::Opt(_) => unimplemented!(),
        }
    }
}

#[derive(Debug)]
pub enum JitDescriptor {
    DoraFct(FctId),
    CompileStub,
    ThrowStub,
    TrapStub,
    AllocStub,
    VerifyStub,
    NativeStub(FctId),
    DoraStub,
}

pub struct JitBaselineFct {
    code_start: Address,
    code_end: Address,

    pub desc: JitDescriptor,
    pub throws: bool,

    // pointer to beginning of function
    pub fct_start: Address,

    // machine code length in bytes
    fct_len: usize,

    pub framesize: i32,
    pub lazy_compilation: LazyCompilationData,
    gcpoints: GcPoints,
    comments: Comments,
    positions: PositionTable,
    pub exception_handlers: Vec<ExHandler>,
}

impl JitBaselineFct {
    pub fn from_optimized_buffer(
        vm: &VM,
        buffer: &[u8],
        desc: JitDescriptor,
        throws: bool,
    ) -> JitBaselineFct {
        let dseg = DSeg::new();

        JitBaselineFct::from_buffer(
            vm,
            &dseg,
            buffer,
            LazyCompilationData::new(),
            GcPoints::new(),
            0,
            Comments::new(),
            PositionTable::new(),
            desc,
            throws,
            Vec::new(),
        )
    }

    pub fn from_buffer(
        vm: &VM,
        dseg: &DSeg,
        buffer: &[u8],
        lazy_compilation: LazyCompilationData,
        gcpoints: GcPoints,
        framesize: i32,
        comments: Comments,
        positions: PositionTable,
        desc: JitDescriptor,
        throws: bool,
        mut exception_handlers: Vec<ExHandler>,
    ) -> JitBaselineFct {
        let size = dseg.size() as usize + buffer.len();
        let ptr = vm.gc.alloc_code(size);

        if ptr.is_null() {
            panic!("out of memory: not enough executable memory left!");
        }

        dseg.finish(ptr.to_ptr());

        let fct_start = ptr.offset(dseg.size() as usize);

        unsafe {
            ptr::copy_nonoverlapping(buffer.as_ptr(), fct_start.to_mut_ptr(), buffer.len());
        }

        flush_icache(ptr.to_ptr(), size);

        for handler in &mut exception_handlers {
            handler.try_start = fct_start.offset(handler.try_start).to_usize();
            handler.try_end = fct_start.offset(handler.try_end).to_usize();
            handler.catch = fct_start.offset(handler.catch).to_usize();
        }

        JitBaselineFct {
            code_start: ptr,
            code_end: ptr.offset(size as usize),
            lazy_compilation,
            gcpoints,
            comments,
            framesize,
            fct_start,
            fct_len: buffer.len(),
            positions,
            desc,
            throws,
            exception_handlers,
        }
    }

    pub fn position_for_offset(&self, offset: u32) -> Option<Position> {
        self.positions.get(offset)
    }

    pub fn gcpoint_for_offset(&self, offset: i32) -> Option<&GcPoint> {
        self.gcpoints.get(offset)
    }

    pub fn ptr_start(&self) -> Address {
        self.code_start
    }

    pub fn ptr_end(&self) -> Address {
        self.code_end
    }

    pub fn fct_id(&self) -> FctId {
        match self.desc {
            JitDescriptor::NativeStub(fct_id) => fct_id,
            JitDescriptor::DoraFct(fct_id) => fct_id,
            _ => panic!("no fctid found"),
        }
    }

    pub fn fct_ptr(&self) -> Address {
        self.fct_start
    }

    pub fn fct_end(&self) -> Address {
        self.fct_start.offset(self.fct_len())
    }

    pub fn fct_len(&self) -> usize {
        self.fct_len
    }

    pub fn get_comment(&self, offset: u32) -> Option<&[String]> {
        self.comments.get(offset)
    }
}

impl fmt::Debug for JitBaselineFct {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "JitBaselineFct {{ start: {:?}, end: {:?}, desc: {:?} }}",
            self.ptr_start(),
            self.ptr_end(),
            self.desc,
        )
    }
}

#[derive(Debug)]
pub struct GcPoints {
    points: HashMap<i32, GcPoint>,
}

impl GcPoints {
    pub fn new() -> GcPoints {
        GcPoints {
            points: HashMap::new(),
        }
    }

    pub fn get(&self, offset: i32) -> Option<&GcPoint> {
        self.points.get(&offset)
    }

    pub fn insert(&mut self, offset: i32, gcpoint: GcPoint) {
        assert!(self.points.insert(offset, gcpoint).is_none());
    }
}

#[derive(Debug)]
pub struct GcPoint {
    pub offsets: Vec<i32>,
}

impl GcPoint {
    pub fn new() -> GcPoint {
        GcPoint {
            offsets: Vec::new(),
        }
    }

    pub fn merge(lhs: GcPoint, rhs: GcPoint) -> GcPoint {
        let mut offsets = HashSet::new();

        for offset in lhs.offsets {
            offsets.insert(offset);
        }

        for offset in rhs.offsets {
            offsets.insert(offset);
        }

        GcPoint::from_offsets(offsets.drain().collect())
    }

    pub fn from_offsets(offsets: Vec<i32>) -> GcPoint {
        GcPoint { offsets }
    }
}

pub struct Comments {
    entries: Vec<(u32, Vec<String>)>,
}

impl Comments {
    pub fn new() -> Comments {
        Comments {
            entries: Vec::new(),
        }
    }

    pub fn get(&self, offset: u32) -> Option<&[String]> {
        let result = self
            .entries
            .binary_search_by_key(&offset, |&(offset, _)| offset);

        match result {
            Ok(idx) => Some(&self.entries[idx].1),
            Err(_) => None,
        }
    }

    pub fn insert(&mut self, offset: u32, comment: String) {
        if let Some(last) = self.entries.last_mut() {
            debug_assert!(offset >= last.0);

            if last.0 == offset {
                last.1.push(comment);
                return;
            }
        }

        self.entries.push((offset, vec![comment]));
    }
}

#[derive(Debug)]
pub struct PositionTable {
    entries: Vec<(u32, Position)>,
}

impl PositionTable {
    pub fn new() -> PositionTable {
        PositionTable {
            entries: Vec::new(),
        }
    }

    pub fn insert(&mut self, offset: u32, position: Position) {
        if let Some(last) = self.entries.last() {
            debug_assert!(offset > last.0);
        }

        self.entries.push((offset, position));
    }

    pub fn get(&self, offset: u32) -> Option<Position> {
        let result = self
            .entries
            .binary_search_by_key(&offset, |&(offset, _)| offset);

        match result {
            Ok(idx) => Some(self.entries[idx].1),
            Err(_) => None,
        }
    }
}

#[derive(Debug)]
pub struct ExHandler {
    pub try_start: usize,
    pub try_end: usize,
    pub catch: usize,
    pub offset: Option<i32>,
    pub catch_type: CatchType,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CatchType {
    Any,
    Class(*const ClassDef),
}

#[derive(Debug)]
pub struct LazyCompilationData {
    entries: Vec<(u32, LazyCompilationSite)>,
}

impl LazyCompilationData {
    pub fn new() -> LazyCompilationData {
        LazyCompilationData {
            entries: Vec::new(),
        }
    }

    pub fn insert(&mut self, offset: u32, info: LazyCompilationSite) {
        if let Some(last) = self.entries.last() {
            debug_assert!(offset > last.0);
        }

        self.entries.push((offset, info));
    }

    pub fn get(&self, offset: u32) -> Option<&LazyCompilationSite> {
        let result = self
            .entries
            .binary_search_by_key(&offset, |&(offset, _)| offset);

        match result {
            Ok(idx) => Some(&self.entries[idx].1),
            Err(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum LazyCompilationSite {
    Compile(FctId, i32, TypeList, TypeList),
    VirtCompile(u32, TypeList, TypeList),
}
