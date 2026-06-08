;; Imported from upstream s7test.scm line 10025.
;; Original form:
;; (test ((append '(3) () '(1 2)) 0) 3)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () ((append '(3) () '(1 2)) 0))))
       (expected (upstream-safe (lambda () 3)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10025 actual expected ok?))
