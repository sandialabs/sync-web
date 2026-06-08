;; Imported from upstream s7test.scm line 30826.
;; Original form:
;; (test (catch #t (lambda () (set! 'a 1)) (lambda (typ info) (apply format #f info)))
;;       "#_quote (syntactic) does not have a setter: (set! 'a 1)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (set! 'a 1)) (lambda (typ info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "#_quote (syntactic) does not have a setter: (set! 'a 1)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30826 actual expected ok?))
