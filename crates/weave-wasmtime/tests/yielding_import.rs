//! Yielding imports and per-instance memory limits (host-embedding hooks).

use wasmtime::{Caller, Val};
use weave_core::snapshot::Snapshot;
use weave_transform::TransformOptions;
use weave_wasmtime::instance::LinkFn;
use weave_wasmtime::{
    default_engine, request_unwind, Ctx, InstanceState, WeaveInstance, WeaveModule, WorkResult,
};

const GUEST: &str = r#"(module
    (import "env" "ask" (func $ask (param i32) (result i32)))
    (memory (export "memory") 1)
    ;; a loop calling the yielding import
    (func (export "run") (result i32)
      (local $i i32) (local $acc i32)
      (loop $l
        (local.set $acc (i32.add (local.get $acc) (call $ask (local.get $i))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br_if $l (i32.lt_u (local.get $i) (i32.const 5))))
      (local.get $acc))
    ;; no loop, no recursion: only the yielding import makes it checkpointable
    (func $leaf (param i32) (result i32)
      (i32.mul (call $ask (local.get 0)) (i32.const 2)))
    (func (export "once") (result i32)
      (i32.add (call $leaf (i32.const 7)) (i32.const 1)))
    (func (export "grow") (param i32) (result i32)
      (memory.grow (local.get 0))))"#;

/// Host side of `env.ask`: the first call stops the guest, the call made on
/// resume returns the answer.
#[derive(Default)]
struct Asker {
    question: Option<i32>,
    answer: Option<i32>,
    unwinds: u32,
}

fn module() -> WeaveModule {
    WeaveModule::from_raw(
        &wat::parse_str(GUEST).unwrap(),
        &TransformOptions {
            yielding_imports: vec![("env".into(), "ask".into())],
            ..Default::default()
        },
    )
    .unwrap()
}

fn link() -> LinkFn {
    Box::new(|linker| {
        linker.func_wrap("env", "ask", |mut caller: Caller<'_, Ctx>, q: i32| -> i32 {
            let a = caller.data_mut().service_state::<Asker>(0);
            if let Some(v) = a.answer.take() {
                return v;
            }
            a.question = Some(q);
            a.unwinds += 1;
            request_unwind(&mut caller).unwrap();
            0
        })?;
        Ok(())
    })
}

fn fresh(engine: &wasmtime::Engine, m: &WeaveModule) -> WeaveInstance {
    WeaveInstance::new_fresh(engine, m, vec![], vec![Box::new(Asker::default())], link()).unwrap()
}

/// Answer each question with q * 10 until the entry completes.
fn drive(inst: &mut WeaveInstance, mut r: WorkResult) -> i32 {
    loop {
        match r {
            WorkResult::Done(v) => match v.as_slice() {
                [Val::I32(n)] => return *n,
                other => panic!("unexpected results {other:?}"),
            },
            WorkResult::Unwound => {
                let a = inst.ctx_mut().service_state::<Asker>(0);
                let q = a.question.take().expect("unwound without a question");
                a.answer = Some(q * 10);
                r = inst.resume().unwrap();
            }
        }
    }
}

#[test]
fn yielding_import_unwinds_and_returns_on_resume() {
    let engine = default_engine().unwrap();
    let m = module();
    let mut inst = fresh(&engine, &m);
    let r = inst.call_entry("run", &[]).unwrap();
    assert!(matches!(r, WorkResult::Unwound));
    assert_eq!(drive(&mut inst, r), 0 + 10 + 20 + 30 + 40);
    assert_eq!(inst.ctx_mut().service_state::<Asker>(0).unwinds, 5);
}

#[test]
fn caller_of_yielding_import_is_instrumented() {
    let engine = default_engine().unwrap();
    let m = module();
    let mut inst = fresh(&engine, &m);
    let r = inst.call_entry("once", &[]).unwrap();
    assert!(matches!(r, WorkResult::Unwound));
    assert_eq!(drive(&mut inst, r), 7 * 10 * 2 + 1);
}

#[test]
fn checkpoint_inside_yielding_import_restores_into_a_new_instance() {
    let engine = default_engine().unwrap();
    let m = module();
    let mut a = fresh(&engine, &m);
    let mut r = a.call_entry("run", &[]).unwrap();
    // answer two questions, then move to a fresh instance mid-loop
    for _ in 0..2 {
        let st = a.ctx_mut().service_state::<Asker>(0);
        let q = st.question.take().unwrap();
        st.answer = Some(q * 10);
        r = a.resume().unwrap();
    }
    assert!(matches!(r, WorkResult::Unwound));
    let pending = a.ctx_mut().service_state::<Asker>(0).question;
    let snap = Snapshot::decode(&a.checkpoint().unwrap().encode()).unwrap();
    drop(a);

    let mut b = WeaveInstance::new_restored(
        &engine,
        &m,
        vec![],
        vec![Box::new(Asker { question: pending, ..Default::default() })],
        link(),
    )
    .unwrap();
    b.restore(&snap).unwrap();
    assert_eq!(b.state(), InstanceState::Paused);
    assert_eq!(drive(&mut b, WorkResult::Unwound), 100);
}

#[test]
fn memory_limit_makes_grow_fail_instead_of_trapping() {
    let engine = default_engine().unwrap();
    let m = module();
    let mut inst = fresh(&engine, &m);
    // Physical size, including Weave's shadow stack; the cap applies to it.
    let physical = {
        let view = inst.mem_view().unwrap();
        weave_host::MemRead::size(&view, 0) / 65536
    };
    inst.set_memory_limit(Some((physical + 2) * 65536));
    let grow = |inst: &mut WeaveInstance, n: i32| match inst.call_entry("grow", &[Val::I32(n)]).unwrap() {
        WorkResult::Done(v) => match v.as_slice() {
            [Val::I32(r)] => *r,
            _ => panic!(),
        },
        WorkResult::Unwound => panic!("grow must not unwind"),
    };
    // memory.grow reports the guest-visible size, which excludes the stack.
    let visible = grow(&mut inst, 0);
    assert_eq!(grow(&mut inst, 3), -1, "3 pages exceed the cap");
    assert_eq!(grow(&mut inst, 2), visible, "2 pages fit");
    assert_eq!(grow(&mut inst, 1), -1, "the cap is reached");
    inst.set_memory_limit(None);
    assert_eq!(grow(&mut inst, 1), visible + 2);
}
