;; Imported from upstream s7test.scm line 16675.
;; Original form:
;; (test (catch #t (lambda () (set! (when #t 3) 21) ) (lambda (type info) (apply format #f info)))
;;       "when (syntactic) does not have a setter: (set! (when #t 3) 21)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (set! (when #t 3) 21) ) (lambda (type info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "when (syntactic) does not have a setter: (set! (when #t 3) 21)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16675 actual expected ok?))
