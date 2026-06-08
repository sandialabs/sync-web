;; Imported from upstream s7test.scm line 35644.
;; Original form:
;; (test (+ 5 (begin (values 1 2 3) 4)) 9)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (+ 5 (begin (values 1 2 3) 4)))))
       (expected (upstream-safe (lambda () 9)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35644 actual expected ok?))
