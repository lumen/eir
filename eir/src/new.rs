use crate::{ FunctionIdent, ConstantTerm, AtomicTerm, LambdaEnvIdx };
use crate::Clause;
use crate::op::OpKind;
use ::cranelift_entity::{ PrimaryMap, SecondaryMap, ListPool, EntityList,
                          entity_impl };
use ::cranelift_entity::packed_option::PackedOption;

/// Basic block in function
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ebb(u32);
entity_impl!(Ebb, "ebb");

/// OP in EBB
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Op(u32);
entity_impl!(Op, "op");

/// Either a SSA variable or a constant
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Value(u32);
entity_impl!(Value, "value");

/// Call from OP to other EBB
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct EbbCall(u32);
entity_impl!(EbbCall, "ebb_call");

/// Reference to other function
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct FunRef(u32);
entity_impl!(FunRef, "fun_ref");

#[derive(Clone, Debug, Default)]
pub struct EbbNode {
    prev: Option<Ebb>,
    next: Option<Ebb>,
    first_op: Option<Op>,
    last_op: Option<Op>,
}

#[derive(Clone, Debug, Default)]
pub struct OpNode {
    ebb: Option<Ebb>,
    prev: Option<Op>,
    next: Option<Op>,
}

#[derive(Debug)]
pub struct Layout {
    ebbs: SecondaryMap<Ebb, EbbNode>,
    ops: SecondaryMap<Op, OpNode>,
    first_ebb: Option<Ebb>,
    last_ebb: Option<Ebb>,
}
impl Layout {

    pub fn new() -> Self {
        Layout {
            ebbs: SecondaryMap::new(),
            ops: SecondaryMap::new(),
            first_ebb: None,
            last_ebb: None,
        }
    }

    pub fn insert_ebb_first(&mut self, ebb: Ebb) {
        assert!(self.first_ebb.is_none());
        assert!(self.last_ebb.is_none());
        self.first_ebb = Some(ebb);
        self.last_ebb = Some(ebb);
    }

    pub fn insert_ebb_after(&mut self, prev: Ebb, ebb: Ebb) {
        // TODO: Validate not inserted
        let next = self.ebbs[prev].next;
        self.ebbs[prev].next = Some(ebb);
        self.ebbs[ebb].prev = Some(prev);
        self.ebbs[ebb].next = next;
        if let Some(next) = next {
            self.ebbs[next].prev = Some(ebb);
        }
    }

    pub fn insert_op_after(&mut self, ebb: Ebb, prev_op: Option<Op>, op: Op) {
        assert!(self.ops[op].ebb == None);
        self.ops[op].ebb = Some(ebb);

        if let Some(prev_op) = prev_op {
            // If a previous operation is selected,

            let next = self.ops[prev_op].next;
            // Update previous and next of current
            self.ops[op].prev = Some(prev_op);
            self.ops[op].next = next;

            // Set the next of previous to current
            self.ops[prev_op].next = Some(op);

            // If there is a next operation,
            // set it's previous to current
            // else,
            // set the last Ebb Op to current
            if let Some(next) = next {
                assert!(self.ops[next].prev == Some(prev_op));
                self.ops[next].prev = Some(op);
            } else {
                assert!(self.ebbs[ebb].last_op == Some(prev_op));
                self.ebbs[ebb].last_op = Some(op);
            }
        } else {
            // No previous operation selected, insert at beginning

            // Get the Op that is at block start
            let next = self.ebbs[ebb].first_op;

            // Set the next of the current Op to that
            self.ops[op].next = next;

            // Set the first Op of the Ebb to the current Op
            self.ebbs[ebb].first_op = Some(op);

            if let Some(next) = next {
                // If there was an Op after this one, set its previous to current
                assert!(self.ops[next].prev == None);
                self.ops[next].prev = Some(op);
            } else {
                // If there was no Op after this one, set the last Op to current
                assert!(self.ebbs[ebb].last_op == None);
                self.ebbs[ebb].last_op = Some(op);
            }

        }

    }

}

#[derive(Debug)]
pub struct OpData {
    kind: OpKind,
    reads: EntityList<Value>,
    writes: EntityList<Value>,
    ebb_calls: EntityList<EbbCall>,
}

#[derive(Debug)]
pub struct EbbData {
    arguments: EntityList<Value>,
    finished: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ValueType {
    Variable,
    Constant(ConstantTerm),
}

#[derive(Debug)]
pub struct EbbCallData {
    block: Ebb,
    values: EntityList<Value>,
}

pub struct EbbIter<'a> {
    fun: &'a Function,
    next: Option<Ebb>,
}
impl<'a> Iterator for EbbIter<'a> {
    type Item = Ebb;
    fn next(&mut self) -> Option<Self::Item> {
        let next = self.next;
        match self.next {
            Some(n) => self.next = self.fun.layout.ebbs[n].next,
            None => (),
        }
        next
    }
}

pub struct OpIter<'a> {
    fun: &'a Function,
    next: Option<Op>,
}
impl<'a> Iterator for OpIter<'a> {
    type Item = Op;
    fn next(&mut self) -> Option<Self::Item> {
        let next = self.next;
        match self.next {
            Some(n) => self.next = self.fun.layout.ops[n].next,
            None => (),
        }
        next
    }
}

#[derive(Debug)]
pub struct Function {

    ident: FunctionIdent,

    layout: Layout,

    ops: PrimaryMap<Op, OpData>,
    ebbs: PrimaryMap<Ebb, EbbData>,
    values: PrimaryMap<Value, ValueType>,
    ebb_calls: PrimaryMap<EbbCall, EbbCallData>,
    fun_refs: PrimaryMap<FunRef, FunctionIdent>,

    ebb_call_pool: ListPool<EbbCall>,
    value_pool: ListPool<Value>,

}

impl Function {

    pub fn new(ident: FunctionIdent) -> Self {
        Function {
            ident: ident,
            layout: Layout::new(),

            ops: PrimaryMap::new(),
            ebbs: PrimaryMap::new(),
            values: PrimaryMap::new(),
            ebb_calls: PrimaryMap::new(),
            fun_refs: PrimaryMap::new(),

            ebb_call_pool: ListPool::new(),
            value_pool: ListPool::new(),
        }
    }

    pub fn new_variable(&mut self) -> Value {
        self.values.push(ValueType::Variable)
    }

    pub fn ident(&self) -> &FunctionIdent {
        &self.ident
    }

    pub fn validate(&self) {
    }

    pub fn iter_ebb<'a>(&'a self) -> EbbIter<'a> {
        EbbIter {
            fun: self,
            next: self.layout.first_ebb,
        }
    }
    pub fn iter_op<'a>(&'a self, ebb: Ebb) -> OpIter<'a> {
        OpIter {
            fun: self,
            next: self.layout.ebbs[ebb].first_op,
        }
    }

    pub fn ebb_args<'a>(&'a self, ebb: Ebb) -> &'a [Value] {
        self.ebbs[ebb].arguments.as_slice(&self.value_pool)
    }

    pub fn ebb_call_target<'a>(&'a self, ebb: EbbCall) -> Ebb {
        self.ebb_calls[ebb].block
    }
    pub fn ebb_call_args<'a>(&'a self, ebb: EbbCall) -> &'a [Value] {
        self.ebb_calls[ebb].values.as_slice(&self.value_pool)
    }

    pub fn op_kind<'a>(&'a self, op: Op) -> &'a OpKind {
        &self.ops[op].kind
    }
    pub fn op_writes<'a>(&'a self, op: Op) -> &[Value] {
        self.ops[op].writes.as_slice(&self.value_pool)
    }
    pub fn op_reads<'a>(&'a self, op: Op) -> &[Value] {
        self.ops[op].reads.as_slice(&self.value_pool)
    }
    pub fn op_branches<'a>(&'a self, op: Op) -> &[EbbCall] {
        self.ops[op].ebb_calls.as_slice(&self.ebb_call_pool)
    }

    pub fn value<'a>(&'a self, value: Value) -> &'a ValueType {
        &self.values[value]
    }

    pub fn to_text(&self) -> String {
        use crate::text::ToEirText;

        let mut out = Vec::new();
        self.to_eir_text(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

}

#[derive(Copy, Clone, Debug)]
pub struct BuilderPosition(Option<Ebb>, Option<Op>);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum BuilderState {
    Build,
    OutstandingEbbCalls(usize),
}

pub struct FunctionBuilder<'a> {
    fun: &'a mut Function,

    current_ebb: Option<Ebb>,
    current_op: Option<Op>,

    state: BuilderState,
}

impl<'a> FunctionBuilder<'a> {

    pub fn new(fun: &'a mut Function) -> FunctionBuilder<'a> {
        FunctionBuilder {
            fun: fun,

            current_ebb: None,
            current_op: None,

            state: BuilderState::Build,
        }
    }

    pub fn gen_variables(&mut self, num: usize, args: &mut Vec<Value>) {
        args.clear();
        for _ in 0..num {
            args.push(self.fun.new_variable());
        }
    }

    fn insert_op(&mut self, data: OpData) -> Op {
        assert!(self.state == BuilderState::Build);

        // Must be in a block
        assert!(self.current_ebb.is_some());
        assert!(!self.fun.ebbs[self.current_ebb.unwrap()].finished);
        // If we are not at the beginning, the last Op can't be a unconditional Jump
        if let Some(op) = self.current_op {
            if let OpKind::Jump = self.fun.ops[op].kind {
                panic!()
            }
        }

        let op = self.fun.ops.push(data);
        self.fun.layout.insert_op_after(
            self.current_ebb.unwrap(), self.current_op, op);

        self.current_op = Some(op);

        op
    }

    /// Can only be called when there are no blocks in the function
    pub fn insert_ebb_entry(&mut self) -> Ebb {
        let ebb = self.fun.ebbs.push(EbbData {
            arguments: EntityList::new(),
            finished: false,
        });
        self.fun.layout.insert_ebb_first(ebb);
        ebb
    }

    pub fn insert_ebb(&mut self) -> Ebb {
        let ebb = self.fun.ebbs.push(EbbData {
            arguments: EntityList::new(),
            finished: false,
        });
        self.fun.layout.insert_ebb_after(self.current_ebb.unwrap(), ebb);
        ebb
    }

    pub fn finish_ebb(&mut self, ebb: Ebb) {
        self.fun.ebbs[ebb].finished = true;
    }

    pub fn add_ebb_argument(&mut self, ebb: Ebb) -> Value {
        assert!(!self.fun.ebbs[ebb].finished);
        let value = self.fun.new_variable();
        self.fun.ebbs[ebb].arguments.push(value, &mut self.fun.value_pool);
        value
    }

    pub fn position_at_end(&mut self, ebb: Ebb) {
        assert!(self.state == BuilderState::Build);
        self.current_ebb = Some(ebb);
        let last_op = self.fun.layout.ebbs[ebb].last_op;
        self.current_op = last_op;
        if let Some(last_op) = last_op {
            assert!(!self.fun.ops[last_op].kind.is_block_terminator())
        }
    }

    pub fn current_ebb(&self) -> Ebb {
        self.current_ebb.unwrap()
    }
    pub fn assert_at_end(&self) {
        if let Some(inner) = self.fun.layout.ebbs[self.current_ebb.unwrap()].last_op {
            assert!(self.current_op.unwrap() == inner);
        } else {
            assert!(self.current_op.is_none());
        }
    }

    pub fn position_store(&self) -> BuilderPosition {
        assert!(self.state == BuilderState::Build);
        BuilderPosition(self.current_ebb, self.current_op)
    }
    pub fn position_load(&mut self, pos: BuilderPosition) {
        assert!(self.state == BuilderState::Build);
        self.current_ebb = pos.0;
        self.current_op = pos.1;
    }

    pub fn create_ebb_call(&mut self, ebb: Ebb, values: &[Value]) -> EbbCall {
        let values_p = EntityList::from_slice(values, &mut self.fun.value_pool);
        let call = self.fun.ebb_calls.push(EbbCallData {
            block: ebb,
            values: values_p,
        });
        call
    }

    pub fn add_op_ebb_call(&mut self, call: EbbCall) {
        if let BuilderState::OutstandingEbbCalls(outstanding) = self.state {
            let outstanding = outstanding - 1;
            self.fun.ops[self.current_op.unwrap()].ebb_calls
                .push(call, &mut self.fun.ebb_call_pool);
            if outstanding == 0 {
                self.state = BuilderState::Build;
            } else {
                self.state = BuilderState::OutstandingEbbCalls(outstanding);
            }
        }
    }

    pub fn deposition(&mut self) {
        self.current_ebb = None;
        self.current_op = None;
    }

    pub fn create_atomic(&mut self, atomic: AtomicTerm) -> Value {
        self.fun.values.push(ValueType::Constant(ConstantTerm::Atomic(atomic)))
    }
    pub fn create_constant(&mut self, constant: ConstantTerm) -> Value {
        self.fun.values.push(ValueType::Constant(constant))
    }

    //pub fn op_arguments(&mut self, results: &mut Vec<Value>) -> Op {
    //    self.gen_variables(self.fun.ident.arity, results);

    //    let writes = EntityList::from_slice(results, &mut self.fun.value_pool);

    //    self.insert_op(OpData {
    //        kind: OpKind::Arguments,
    //        reads: EntityList::new(),
    //        writes: writes,
    //        ebb_calls: EntityList::new(),
    //    })
    //}

    pub fn op_move(&mut self, value: Value) -> Value {
        let result = self.fun.new_variable();

        let writes = EntityList::from_slice(&[result], &mut self.fun.value_pool);
        let reads = EntityList::from_slice(&[value], &mut self.fun.value_pool);

        self.insert_op(OpData {
            kind: OpKind::Move,
            reads: reads,
            writes: writes,
            ebb_calls: EntityList::new(),
        });

        result
    }

    pub fn op_jump(&mut self, ebb_call: EbbCall) {
        let ebb_calls = EntityList::from_slice(
            &[ebb_call], &mut self.fun.ebb_call_pool);
        self.insert_op(OpData {
            kind: OpKind::Jump,
            reads: EntityList::new(),
            writes: EntityList::new(),
            ebb_calls: ebb_calls,
        });
    }

    pub fn op_call(&mut self, module: Value, name: Value,
                   args: &[Value]) -> (Value, Value) {
        let mut reads = EntityList::from_slice(
            &[module, name], &mut self.fun.value_pool);
        reads.extend(args.iter().cloned(), &mut self.fun.value_pool);

        let result_ok = self.fun.new_variable();
        let result_err = self.fun.new_variable();
        let writes = EntityList::from_slice(
            &[result_ok, result_err], &mut self.fun.value_pool);

        self.insert_op(OpData {
            kind: OpKind::Call { tail_call: false },
            reads: reads,
            writes: writes,
            ebb_calls: EntityList::new(),
        });

        self.state = BuilderState::OutstandingEbbCalls(1);

        (result_ok, result_err)
    }

    pub fn op_apply(&mut self, fun: Value, args: &[Value]) -> (Value, Value) {
        let mut reads = EntityList::from_slice(
            &[fun], &mut self.fun.value_pool);
        reads.extend(args.iter().cloned(), &mut self.fun.value_pool);

        let result_ok = self.fun.new_variable();
        let result_err = self.fun.new_variable();
        let writes = EntityList::from_slice(
            &[result_ok, result_err], &mut self.fun.value_pool);

        self.insert_op(OpData {
            kind: OpKind::Apply { tail_call: false },
            reads: reads,
            writes: writes,
            ebb_calls: EntityList::new(),
        });

        self.state = BuilderState::OutstandingEbbCalls(1);

        (result_ok, result_err)
    }

    pub fn op_capture_named_function(&mut self, name: FunctionIdent) -> Value {
        let result = self.fun.new_variable();
        let writes = EntityList::from_slice(
            &[result], &mut self.fun.value_pool);

        self.insert_op(OpData {
            kind: OpKind::CaptureNamedFunction(name),
            reads: EntityList::new(),
            writes: writes,
            ebb_calls: EntityList::new(),
        });

        result
    }

    pub fn op_unpack_value_list(&mut self, val_list: Value, num: usize,
                                result: &mut Vec<Value>) {
        self.gen_variables(num, result);

        let reads = EntityList::from_slice(&[val_list], &mut self.fun.value_pool);
        let writes = EntityList::from_slice(result, &mut self.fun.value_pool);

        self.insert_op(OpData {
            kind: OpKind::UnpackValueList,
            reads: reads,
            writes: writes,
            ebb_calls: EntityList::new(),
        });
    }

    pub fn op_pack_value_list(&mut self, values: &[Value]) -> Value {
        let reads = EntityList::from_slice(values, &mut self.fun.value_pool);

        let result = self.fun.new_variable();
        let writes = EntityList::from_slice(
            &[result], &mut self.fun.value_pool);

        self.insert_op(OpData {
            kind: OpKind::PackValueList,
            reads: reads,
            writes: writes,
            ebb_calls: EntityList::new(),
        });

        result
    }

    pub fn op_return_throw(&mut self, value: Value) {
        let reads = EntityList::from_slice(&[value], &mut self.fun.value_pool);
        self.insert_op(OpData {
            kind: OpKind::ReturnThrow,
            reads: reads,
            writes: EntityList::new(),
            ebb_calls: EntityList::new(),
        });
    }

    pub fn op_return_ok(&mut self, value: Value) {
        let reads = EntityList::from_slice(&[value], &mut self.fun.value_pool);
        self.insert_op(OpData {
            kind: OpKind::ReturnOk,
            reads: reads,
            writes: EntityList::new(),
            ebb_calls: EntityList::new(),
        });
    }

    pub fn op_unpack_env(&mut self, value: Value, num_values: usize,
                         results: &mut Vec<Value>) {
        self.gen_variables(num_values, results);

        let writes = EntityList::from_slice(results, &mut self.fun.value_pool);
        let reads = EntityList::from_slice(&[value], &mut self.fun.value_pool);
        self.insert_op(OpData {
            kind: OpKind::UnpackEnv,
            reads: reads,
            writes: writes,
            ebb_calls: EntityList::new(),
        });
    }

    pub fn op_bind_closure(&mut self, ident: FunctionIdent, env: Value) -> Value {
        let result = self.fun.new_variable();

        let writes = EntityList::from_slice(&[result], &mut self.fun.value_pool);
        let reads = EntityList::from_slice(&[env], &mut self.fun.value_pool);
        self.insert_op(OpData {
            kind: OpKind::BindClosure {
                ident: ident,
            },
            reads: reads,
            writes: writes,
            ebb_calls: EntityList::new(),
        });

        result
    }

    pub fn op_make_tuple(&mut self, values: &[Value]) -> Value {
        let result = self.fun.new_variable();

        let reads = EntityList::from_slice(values, &mut self.fun.value_pool);
        let writes = EntityList::from_slice(&[result], &mut self.fun.value_pool);
        self.insert_op(OpData {
            kind: OpKind::MakeTuple,
            reads: reads,
            writes: writes,
            ebb_calls: EntityList::new(),
        });

        result
    }

    pub fn op_make_list(&mut self, head: &[Value], tail: Value) -> Value {
        let mut reads = EntityList::from_slice(
            &[tail], &mut self.fun.value_pool);
        reads.extend(head.iter().cloned(), &mut self.fun.value_pool);

        let result = self.fun.new_variable();
        let writes = EntityList::from_slice(&[result], &mut self.fun.value_pool);

        self.insert_op(OpData {
            kind: OpKind::MakeList,
            reads: reads,
            writes: writes,
            ebb_calls: EntityList::new(),
        });

        result
    }

    pub fn op_make_map(&mut self, merge: Option<Value>, kv: &[Value]) -> Value {
        assert!(kv.len() % 2 == 0);
        unimplemented!()
    }

    pub fn op_make_binary(&mut self, values: &[Value]) -> Value {
        assert!(values.len() % 2 == 0);
        unimplemented!();
    }

    pub fn op_make_no_value(&mut self) -> Value {
        let result = self.fun.new_variable();
        let writes = EntityList::from_slice(&[result], &mut self.fun.value_pool);

        self.insert_op(OpData {
            kind: OpKind::MakeList,
            reads: EntityList::new(),
            writes: writes,
            ebb_calls: EntityList::new(),
        });

        result
    }

    pub fn op_make_closure_env(&mut self, lambda_env: LambdaEnvIdx, values: &[Value]) -> Value {
        let result = self.fun.new_variable();
        let writes = EntityList::from_slice(&[result], &mut self.fun.value_pool);

        let reads = EntityList::from_slice(values, &mut self.fun.value_pool);

        self.insert_op(OpData {
            kind: OpKind::MakeClosureEnv { env_idx: lambda_env },
            reads: reads,
            writes: writes,
            ebb_calls: EntityList::new(),
        });

        result
    }

    pub fn op_case_start(&mut self, clauses: Vec<Clause>,
                         value: Value, value_vars: &[Value], body: Ebb) -> Value {
        let result = self.fun.new_variable();
        let writes = EntityList::from_slice(&[result], &mut self.fun.value_pool);

        let mut reads = EntityList::from_slice(&[value], &mut self.fun.value_pool);
        reads.extend(value_vars.iter().cloned(), &mut self.fun.value_pool);

        let call = self.create_ebb_call(body, &[]);
        let branches = EntityList::from_slice(&[call], &mut self.fun.ebb_call_pool);

        self.insert_op(OpData {
            kind: OpKind::CaseStart {
                clauses: clauses,
            },
            reads: reads,
            writes: writes,
            ebb_calls: branches,
        });

        result
    }

    pub fn op_case_body(&mut self, case_val: Value, num_clauses: usize) {
        let reads = EntityList::from_slice(&[case_val], &mut self.fun.value_pool);
        self.insert_op(OpData {
            kind: OpKind::Case(num_clauses),
            reads: reads,
            writes: EntityList::new(),
            ebb_calls: EntityList::new(),
        });
        self.state = BuilderState::OutstandingEbbCalls(num_clauses + 1);
    }

    pub fn op_case_values(&mut self, case_val: Value, num_results: usize,
                          results: &mut Vec<Value>) {
        self.gen_variables(num_results, results);
        let reads = EntityList::from_slice(&[case_val], &mut self.fun.value_pool);
        let writes = EntityList::from_slice(results, &mut self.fun.value_pool);
        self.insert_op(OpData {
            kind: OpKind::CaseValues,
            reads: reads,
            writes: writes,
            ebb_calls: EntityList::new(),
        });
    }

    pub fn op_case_guard_ok(&mut self, case_val: Value) {
        let reads = EntityList::from_slice(&[case_val], &mut self.fun.value_pool);
        self.insert_op(OpData {
            kind: OpKind::CaseGuardOk,
            reads: reads,
            writes: EntityList::new(),
            ebb_calls: EntityList::new(),
        });
    }
    pub fn op_case_guard_fail(&mut self, case_val: Value, clause_num: usize) {
        let reads = EntityList::from_slice(&[case_val], &mut self.fun.value_pool);
        self.insert_op(OpData {
            kind: OpKind::CaseGuardFail { clause_num },
            reads: reads,
            writes: EntityList::new(),
            ebb_calls: EntityList::new(),
        });
    }

    pub fn op_branch_not_truthy(&mut self, value: Value, call: EbbCall) {
        let reads = EntityList::from_slice(&[value], &mut self.fun.value_pool);
        let calls = EntityList::from_slice(&[call], &mut self.fun.ebb_call_pool);
        self.insert_op(OpData {
            kind: OpKind::IfTruthy,
            reads: reads,
            writes: EntityList::new(),
            ebb_calls: calls,
        });
    }

    pub fn op_receive_start(&mut self, timeout: Value) -> Value {
        unimplemented!()
    }

    pub fn op_receive_wait(&mut self, structure: Value,
                           match_call: EbbCall, timeout_call: EbbCall) -> Value {
        unimplemented!()
    }

    pub fn op_receive_get_message(&mut self, structure: Value) -> Value {
        unimplemented!()
    }

    pub fn op_unreachable(&mut self) {
        unimplemented!()
    }

    pub fn op_exc_trace(&mut self, val: Value) -> Value {
        unimplemented!()
    }

}




