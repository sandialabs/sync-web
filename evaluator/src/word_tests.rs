#[cfg(test)]
mod tests{
    use std::{cell::Cell,mem::size_of};
    use crate::bytecode::{AddTerm,BytecodeFunction,BytecodeLayout,Instr};
    use crate::collections::eq;
    use crate::compiled::{BuiltinId,CompiledBody,CompiledLayout};
    use crate::core::{Env,Params,Procedure,Value};
    use crate::word::{Singleton,Word,WordHeap,FIXNUM_MAX,FIXNUM_MIN};
    use crate::word_bytecode::{ResumableFrameStack,ResumableWordMachine,ResumeHostCall,ResumeHostOutcome,ResumeHostStep,ResumeStatus,WordInstr,WordProgram};

    #[test]
    #[ignore="developer trace-eligibility reporter"]
    fn report_word_trace_eligibility(){let path=std::env::var_os("S7_ELIGIBILITY_SOURCE").expect("S7_ELIGIBILITY_SOURCE");let source=std::fs::read_to_string(path).expect("read diagnostic source");std::thread::Builder::new().name("word-eligibility".into()).stack_size(64*1024*1024).spawn(move||{crate::word_bytecode::reset_eligibility();let _=crate::run_source(&source);for (reason,count) in crate::word_bytecode::eligibility_report(){eprintln!("word-eligibility {reason}={count}");}}).expect("spawn eligibility evaluator").join().expect("eligibility evaluator panicked");}

    #[test]
    fn word_is_one_machine_word(){assert_eq!(size_of::<Word>(),8);assert_eq!(size_of::<WordInstr>(),16);}

    #[test]
    fn immediate_round_trips(){
        for value in [FIXNUM_MIN,-1,0,1,FIXNUM_MAX]{assert_eq!(Word::fixnum(value).and_then(Word::as_fixnum),Some(value));}
        for value in [Singleton::False,Singleton::True,Singleton::Nil,Singleton::Undefined]{assert_eq!(Word::singleton(value).as_singleton(),Some(value));}
        assert_eq!(Word::character('λ').as_character(),Some('λ'));
        assert_eq!(Word::symbol(42).as_symbol(),Some(42));
    }

    #[test]
    fn full_i64_values_remain_exact(){
        let mut heap=WordHeap::default();
        for value in [i64::MIN,FIXNUM_MIN-1,FIXNUM_MAX+1,i64::MAX]{let word=heap.integer(value);assert_eq!(heap.integer_value(word),Some(value));}
    }

    #[test]
    fn heap_pairs_are_stable_and_mutable(){
        let mut heap=WordHeap::default();
        let one=heap.integer(1);let two=heap.integer(2);let three=heap.integer(3);
        let pair=heap.pair(one,two);
        assert_eq!(heap.pair_values(pair),Some((one,two)));
        assert!(heap.set_pair_car(pair,three));
        assert_eq!(heap.pair_values(pair),Some((three,two)));
    }

    #[test]
    fn canonical_symbols_keep_keyword_and_gensym_identity(){
        let mut heap=WordHeap::default();
        let symbol=heap.intern_symbol("name",false);let same=heap.intern_symbol("name",false);let keyword=heap.intern_symbol("name",true);let gensym=heap.gensym("name");
        assert_eq!(symbol,same);assert_ne!(symbol,keyword);assert_ne!(symbol,gensym);
        assert_eq!(heap.symbol_meta(keyword),Some(("name",true,false)));
        assert_eq!(heap.symbol_meta(gensym),Some(("name",false,true)));
        let converted=heap.from_value(&Value::keyword("name"));assert!(matches!(heap.to_value(converted),Some(Value::Keyword(_))));
    }

    #[test]
    fn vectors_and_environment_chains_use_copy_words(){
        let mut heap=WordHeap::default();
        let key=heap.intern_symbol("x",false);let one=heap.integer(1);let two=heap.integer(2);
        let vector=heap.vector(vec![one]);assert_eq!(heap.vector_ref(vector,0),Some(one));assert!(heap.vector_set(vector,0,two));assert_eq!(heap.vector_ref(vector,0),Some(two));
        let root=heap.environment(None);let child=heap.environment(Some(root));assert!(heap.env_define(root,key,one));assert_eq!(heap.env_get(child,key),Some(one));assert!(heap.env_set(child,key,two));assert_eq!(heap.env_get(root,key),Some(two));
    }

    #[test]
    fn legacy_graph_conversion_preserves_pair_cycles(){
        let pair=Value::cons(Value::Int(1),Value::Nil);pair.set_cdr(pair.clone()).unwrap();
        let mut heap=WordHeap::default();let word=heap.from_value(&pair);let (car,cdr)=heap.pair_values(word).unwrap();
        assert_eq!(heap.integer_value(car),Some(1));assert_eq!(cdr,word);assert_eq!(heap.list_length(word),None);
    }

    #[test]
    fn word_export_restores_legacy_identity_and_shared_aliases(){
        let pair=Value::cons(Value::Int(1),Value::Nil);let vector=Value::Vector(std::rc::Rc::new(crate::core::VectorData::new(vec![pair.clone(),pair.clone()])));
        let mut heap=WordHeap::default();let word=heap.from_value(&vector);let output=heap.to_value(word).unwrap();assert!(eq(&output,&vector));let Value::Vector(output_vector)=output else{panic!("vector")};let first=output_vector.get(0);let second=output_vector.get(1);assert!(eq(&first,&second));assert!(eq(&first,&pair));first.set_car(Value::Int(9)).unwrap();assert!(matches!(pair.car(),Ok(Value::Int(9))));
    }

    #[test]
    fn word_export_preserves_native_cycles(){
        let mut heap=WordHeap::default();let one=heap.integer(1);let pair=heap.pair(one,Word::singleton(Singleton::Nil));assert!(heap.set_pair_cdr(pair,pair));let output=heap.to_value(pair).unwrap();assert!(eq(&output,&output.cdr().unwrap()));
    }

    #[test]
    fn word_export_preserves_native_mutable_sharing(){
        let mut heap=WordHeap::default();let string=heap.string("shared".into());let vector=heap.vector(vec![string,string]);let output=heap.to_value(vector).unwrap();let Value::Vector(vector)=output else{panic!("vector")};assert!(eq(&vector.get(0),&vector.get(1)));
    }

    #[test]
    fn tracing_collection_keeps_only_reachable_nonmoving_objects(){
        let mut heap=WordHeap::default();let one=heap.integer(1);let pair=heap.pair(one,Word::singleton(Singleton::Nil));assert!(heap.set_pair_cdr(pair,pair));let vector=heap.vector(vec![pair]);let _garbage=heap.string("unused".into());
        assert_eq!(heap.object_count(),3);heap.collect(&[vector]);assert_eq!(heap.object_count(),2);assert_eq!(heap.vector_ref(vector,0),Some(pair));assert_eq!(heap.pair_values(pair).unwrap().1,pair);
    }

    #[test]
    fn resumable_word_frame_continues_after_host_call_without_replay(){
        fn make_machine()->ResumableWordMachine{let function=BytecodeFunction{layout:BytecodeLayout::DynamicEnv,constants:vec![Value::Int(0),Value::Int(2)],code:vec![
            Instr::LoadConst(0),Instr::LoadConst(0),Instr::BindTemp{index:1,name:std::rc::Rc::new("acc".into()),sequential:false},Instr::BindTemp{index:0,name:std::rc::Rc::new("i".into()),sequential:false},
            Instr::IntBinaryTerms{id:BuiltinId::NumEq,lhs:AddTerm::Slot(0),rhs:AddTerm::Const(2)},Instr::JumpIfFalsePop(8),Instr::LoadTemp(1),Instr::Jump(17),
            Instr::LoadTemp(1),Instr::LoadDynamic(std::rc::Rc::new("host".into())),Instr::LoadTemp(0),Instr::GenericCall{argc:1},Instr::BuiltinCall{id:BuiltinId::Add,argc:2},Instr::BindTemp{index:1,name:std::rc::Rc::new("acc".into()),sequential:true},
            Instr::IntAddTermsRecur{terms:vec![AddTerm::Slot(0),AddTerm::Const(1)]},Instr::LoadTemp(1),Instr::Recur{argc:2,target:4,param_start:0,param_count:2,name:std::rc::Rc::new("loop".into())},Instr::Return],max_temps:2,cache_stable_builtins:true,required_builtins:vec!["+"],lambdas:vec![],validated_env:Cell::new((0,0)),word_program:None,word_calls_safe:true};let program=std::rc::Rc::new(WordProgram::compile(&function).unwrap());let env=Env::new(None);env.define("host",Value::string("host"));ResumableWordMachine::new(program,&[],&env).unwrap()}
        let mut machine=make_machine();let effects=Cell::new(0);let mut host=|call|match call{ResumeHostCall::Procedure{args,..}=>{effects.set(effects.get()+1);if effects.get()==1{ResumeHostOutcome::Suspend}else{let Value::Int(i)=args[0] else{panic!("integer")};ResumeHostOutcome::Value(Value::Int(i+10))}},ResumeHostCall::Builtin{id:BuiltinId::Add,args}=>ResumeHostOutcome::Value(Value::Int(args.iter().map(|value|if let Value::Int(i)=value{*i}else{0}).sum())),_=>panic!("unexpected host call")};
        let whole_trace=machine.compile_native_trace().expect("whole native trace");assert_eq!(machine.run_native_trace(&whole_trace),0);assert_eq!(machine.continuation_pc(),11);assert!(matches!(machine.resume_host_once(&mut host),ResumeHostStep::Suspended{pc:12}));assert_eq!(effects.get(),1);machine.collect_roots();assert!(machine.supply_call_result(Value::Int(10)));let add_trace=crate::native_jit::compile_resume_add().expect("native resume add");assert_eq!(add_trace.run(i64::MAX,1),None);assert_eq!(machine.run_native_trace(&whole_trace),0);assert_eq!(machine.continuation_pc(),11);assert!(matches!(machine.resume_host_once(&mut host),ResumeHostStep::Advanced));assert_eq!(effects.get(),2);assert_eq!(machine.run_native_trace(&whole_trace),1);assert!(matches!(machine.finish_native(),ResumeStatus::Success(Value::Int(21))));
        let mut failed=make_machine();let failed_effects=Cell::new(0);let mut error_host=|call|match call{ResumeHostCall::Procedure{..}=>{failed_effects.set(failed_effects.get()+1);ResumeHostOutcome::SchemeError(crate::core::SchemeError::new("test-error",vec![]))},_=>panic!("unexpected")};assert!(matches!(failed.resume(&mut error_host),ResumeStatus::SchemeError(error) if error.tag=="test-error"));assert_eq!(failed_effects.get(),1);
    }

    #[test]
    fn rooted_frame_stack_resumes_nested_host_mutation_once(){
        fn function(constants:Vec<Value>,code:Vec<Instr>)->BytecodeFunction{BytecodeFunction{layout:BytecodeLayout::DynamicEnv,constants,code,max_temps:0,cache_stable_builtins:true,required_builtins:vec![],lambdas:vec![],validated_env:Cell::new((0,0)),word_program:None,word_calls_safe:true}}
        let root_function=function(vec![Value::Int(10)],vec![Instr::LoadDynamic(std::rc::Rc::new("child".into())),Instr::LoadConst(0),Instr::GenericCall{argc:1},Instr::Return]);let root_program=std::rc::Rc::new(WordProgram::compile(&root_function).unwrap());
        let env=Env::new(None);let child_function=std::rc::Rc::new(function(vec![Value::Int(10)],vec![Instr::LoadDynamic(std::rc::Rc::new("mutate".into())),Instr::LoadDynamic(std::rc::Rc::new("state".into())),Instr::LoadConst(0),Instr::GenericCall{argc:2},Instr::Return]));let compiled=CompiledBody{env:env.clone(),exprs:vec![],layout:CompiledLayout::DynamicEnv,bytecode:Some(child_function),native:None,native_lambda:None,capture_values:vec![],valid:std::rc::Rc::new(Cell::new(true))};let child=Value::Procedure(std::rc::Rc::new(Procedure::Lambda{params:Params{required:vec!["x".into()],rest:None,star:false,defaults:vec![None],allow_other_keys:false,rest_before_formals:false},body:std::rc::Rc::new(std::cell::RefCell::new(vec![])),env:env.clone(),name:Some("child".into()),compiled:Some(std::rc::Rc::new(compiled))}));
        let state=std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));env.define("child",child);env.define("mutate",Value::string("mutate"));env.define("state",Value::HashTable(state.clone()));let root=ResumableWordMachine::new(root_program,&[],&env).unwrap();let mut frames=ResumableFrameStack::new(root).unwrap();let effects=Cell::new(0);let mut host=|call|match call{ResumeHostCall::Procedure{args,..}=>{effects.set(effects.get()+1);let Value::HashTable(table)=&args[0] else{panic!("state")};table.borrow_mut().push((Value::symbol("count"),Value::Int(effects.get())));let Value::Int(value)=args[1] else{panic!("integer")};ResumeHostOutcome::Value(Value::Int(value+1))},_=>panic!("unexpected")};assert!(matches!(frames.run(&mut host),ResumeStatus::Success(Value::Int(11))));assert_eq!(effects.get(),1);assert_eq!(state.borrow().len(),1);
    }

    #[test]
    fn dynamic_hash_reads_refresh_from_legacy_source(){let function=BytecodeFunction{layout:BytecodeLayout::DynamicEnv,constants:vec![Value::symbol("key")],code:vec![Instr::ApplicableRefDynamic{target:std::rc::Rc::new("state".into()),index:crate::bytecode::ValueOperand::Const(0)},Instr::Return],max_temps:0,cache_stable_builtins:true,required_builtins:vec![],lambdas:vec![],validated_env:Cell::new((0,0)),word_program:None,word_calls_safe:true};let program=WordProgram::compile(&function).expect("dynamic hash read is eligible");let table=std::rc::Rc::new(std::cell::RefCell::new(vec![(Value::symbol("key"),Value::Int(1))]));let env=Env::new(None);env.define("state",Value::HashTable(table.clone()));assert!(matches!(program.execute(&[],Some(&env),|_,_|None),Some(Value::Int(1))));table.borrow_mut()[0].1=Value::Int(2);assert!(matches!(program.execute(&[],Some(&env),|_,_|None),Some(Value::Int(2))));}

    #[test]
    fn fixed_word_bytecode_executes_complete_integer_loop(){
        let function=BytecodeFunction{layout:BytecodeLayout::DynamicEnv,constants:vec![Value::Int(0)],code:vec![
            Instr::LoadConst(0),Instr::BindTemp{index:0,name:std::rc::Rc::new("i".into()),sequential:false},
            Instr::IntBinaryTerms{id:BuiltinId::Less,lhs:AddTerm::Slot(0),rhs:AddTerm::Const(10)},Instr::JumpIfFalsePop(6),
            Instr::IntAddTermsRecur{terms:vec![AddTerm::Slot(0),AddTerm::Const(1)]},Instr::Recur{argc:1,target:2,param_start:0,param_count:1,name:std::rc::Rc::new("loop".into())},
            Instr::LoadTemp(0),Instr::Return],max_temps:1,cache_stable_builtins:true,required_builtins:vec![],lambdas:vec![],validated_env:Cell::new((0,0)),word_program:None,word_calls_safe:true};
        let program=WordProgram::compile(&function).expect("eligible word program");
        assert!(matches!(program.execute(&[],None,|_,_|None),Some(Value::Int(10))));
    }
}
