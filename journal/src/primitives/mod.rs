use crate::evaluator::Evaluator;
use crate::extensions::crypto::{
    primitive_s7_crypto_generate, primitive_s7_crypto_sign, primitive_s7_crypto_verify,
};
use crate::extensions::system::{primitive_s7_system_time_unix, primitive_s7_system_time_utc};
use crate::serialization::{primitive_s7_sync_deserialize, primitive_s7_sync_serialize};
use crate::{SYNC_NODE_TAG, s7};
use std::ffi::CString;

mod host;
mod network;
mod node;
mod records;
mod sandbox;
mod support;

pub(crate) use network::*;
pub(crate) use node::*;
pub(crate) use records::*;
pub(crate) use sandbox::*;
pub(crate) use support::*;

pub(crate) fn journal_evaluator() -> Evaluator {
    Evaluator::new(
        vec![(SYNC_NODE_TAG, type_s7_sync_node())]
            .into_iter()
            .collect(),
        vec![
            primitive_s7_sync_hash(),
            primitive_s7_sync_null(),
            primitive_s7_sync_state(),
            primitive_s7_sync_stub(),
            primitive_s7_sync_is_node(),
            primitive_s7_sync_is_pair(),
            primitive_s7_sync_is_stub(),
            primitive_s7_sync_is_null(),
            primitive_s7_sync_digest(),
            primitive_s7_sync_cons(),
            primitive_s7_sync_car(),
            primitive_s7_sync_cdr(),
            primitive_s7_sync_cut(),
            primitive_s7_sync_create(),
            primitive_s7_sync_delete(),
            primitive_s7_sync_all(),
            primitive_s7_sync_call(),
            primitive_s7_sync_eval(),
            primitive_s7_sync_serialize(),
            primitive_s7_sync_deserialize(),
            primitive_s7_sync_safe_setter(),
            primitive_s7_sync_safe_setter_set(),
            primitive_s7_sync_let(),
            primitive_s7_sync_let_return(),
            primitive_s7_sync_remote(),
            primitive_s7_sync_http(),
            primitive_s7_crypto_generate(),
            primitive_s7_crypto_sign(),
            primitive_s7_crypto_verify(),
            primitive_s7_system_time_unix(),
            primitive_s7_system_time_utc(),
        ],
    )
}

pub(crate) fn install_sync_let_runtime(evaluator: &Evaluator) {
    let sync_let = CString::new(
        "(define-macro (sync-let bindings . body)\
           (if (or (not (list? bindings)) (null? body))\
               (error 'syntax-error \"sync-let requires a binding list and body\"))\
           (for-each\
            (lambda (binding)\
              (if (not (and (list? binding) (= (length binding) 2)\
                            (symbol? (car binding))))\
                  (error 'syntax-error \"Malformed sync-let binding: ~S\" binding)))\
            bindings)\
           `(%sync-let-return\
             (%sync-let ',(map car bindings)\
                        (list ,@(map cadr bindings))\
                        (expression->byte-vector '(begin ,@body))))))",
    )
    .expect("Failed to construct sync-let macro");
    unsafe {
        let safe_setter = s7::s7_let_ref(
            evaluator.sc,
            s7::s7_rootlet(evaluator.sc),
            s7::s7_make_symbol(evaluator.sc, c"%sync-safe-setter".as_ptr()),
        );
        let safe_setter_set = s7::s7_let_ref(
            evaluator.sc,
            s7::s7_rootlet(evaluator.sc),
            s7::s7_make_symbol(evaluator.sc, c"%sync-safe-setter-set!".as_ptr()),
        );
        s7::s7_set_setter(evaluator.sc, safe_setter, safe_setter_set);
        s7::s7_eval_c_string(evaluator.sc, sync_let.as_ptr());
        for name in [
            c"%sync-safe-setter-set!",
            c"%sync-let-body",
            c"%sync-let-environment",
            c"%sync-let-eval",
            c"%sync-let-ok",
            c"%sync-let-error",
            c"%sync-let-list",
        ] {
            s7::s7_define(
                evaluator.sc,
                s7::s7_rootlet(evaluator.sc),
                s7::s7_make_symbol(evaluator.sc, name.as_ptr()),
                s7::s7_undefined(evaluator.sc),
            );
        }
    }
}
