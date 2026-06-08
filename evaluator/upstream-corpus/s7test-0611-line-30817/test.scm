;; Imported from upstream s7test.scm line 30817.
;; Original form:
;; (test (catch #t (lambda () (set! (lambda () 1) 4)) (lambda (typ info) (apply format #f info)))
;;       "lambda (syntactic) does not have a setter: (set! (lambda () 1) 4)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (set! (lambda () 1) 4)) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "lambda (syntactic) does not have a setter: (set! (lambda () 1) 4)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30817 actual expected ok?))
