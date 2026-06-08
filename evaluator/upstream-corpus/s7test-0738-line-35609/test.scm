;; Imported from upstream s7test.scm line 35609.
;; Original form:
;; (test (+ (with-input-from-string "(values 1 2 3)" (lambda () (eval (read)))) 2) 8)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (+ (with-input-from-string "(values 1 2 3)" (lambda () (eval (read)))) 2))))
       (expected (upstream-safe (lambda () 8)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 35609 actual expected ok?))
