;; Imported from upstream s7test.scm line 10019.
;; Original form:
;; (test (list-ref '((1 2) (3 4)) 1 1) 4)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list-ref '((1 2) (3 4)) 1 1))))
       (expected (upstream-safe (lambda () 4)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10019 actual expected ok?))
