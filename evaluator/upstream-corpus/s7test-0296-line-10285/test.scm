;; Imported from upstream s7test.scm line 10285.
;; Original form:
;; (test (make-list 1 ()) '(()))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (make-list 1 ()))))
       (expected (upstream-safe (lambda () '(()))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10285 actual expected ok?))
