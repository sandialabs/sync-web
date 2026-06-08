;; Imported from upstream s7test.scm line 16699.
;; Original form:
;; (test (catch #t (lambda () (set! (_asdf_ 3) 3)) (lambda (type info) (apply format #f info))) "unbound variable _asdf_ in (set! (_asdf_ 3) 3)")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (set! (_asdf_ 3) 3)) (lambda (type info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "unbound variable _asdf_ in (set! (_asdf_ 3) 3)")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16699 actual expected ok?))
