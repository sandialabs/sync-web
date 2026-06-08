;; Imported from upstream s7test.scm line 30877.
;; Original form:
;; (test (apply set! (apply list (list ''(1 2 3) 1)) '(32)) 32)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (apply set! (apply list (list ''(1 2 3) 1)) '(32)))))
       (expected (upstream-safe (lambda () 32)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 30877 actual expected ok?))
