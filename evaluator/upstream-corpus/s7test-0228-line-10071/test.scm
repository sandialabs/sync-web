;; Imported from upstream s7test.scm line 10071.
;; Original form:
;; (test (list-set! '(1 2 . 3) 1 23) 23)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (list-set! '(1 2 . 3) 1 23))))
       (expected (upstream-safe (lambda () 23)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10071 actual expected ok?))
