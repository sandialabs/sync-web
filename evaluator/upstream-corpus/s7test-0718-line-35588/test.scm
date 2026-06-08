;; Imported from upstream s7test.scm line 35588.
;; Original form:
;; (test (let () (values 1 2 3) 4) 4)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let () (values 1 2 3) 4))))
       (expected (upstream-safe (lambda () 4)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35588 actual expected ok?))
